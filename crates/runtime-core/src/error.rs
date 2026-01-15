//! Error types for the distributed training runtime

use thiserror::Error;

/// Result type alias using the runtime Error
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type for the distributed training runtime
#[derive(Error, Debug)]
pub enum Error {
    // Worker errors
    #[error("Worker not found: {worker_id}")]
    WorkerNotFound { worker_id: String },

    #[error("Worker already registered: {worker_id}")]
    WorkerAlreadyRegistered { worker_id: String },

    #[error("Worker heartbeat timeout: {worker_id} (last seen {last_seen_ms}ms ago)")]
    WorkerHeartbeatTimeout { worker_id: String, last_seen_ms: u64 },

    #[error("Worker in invalid state: expected {expected:?}, got {actual:?}")]
    InvalidWorkerState {
        expected: Vec<String>,
        actual: String,
    },

    // Checkpoint errors
    #[error("Checkpoint not found: {checkpoint_id}")]
    CheckpointNotFound { checkpoint_id: String },

    #[error("Checkpoint write failed: {message}")]
    CheckpointWriteFailed { message: String },

    #[error("Checkpoint corrupted: {checkpoint_id} - {reason}")]
    CheckpointCorrupted { checkpoint_id: String, reason: String },

    #[error("No valid checkpoint found for recovery")]
    NoCheckpointForRecovery,

    // Data shard errors
    #[error("Dataset not found: {dataset_id}")]
    DatasetNotFound { dataset_id: String },

    #[error("Shard not found: dataset={dataset_id}, shard={shard_id}")]
    ShardNotFound { dataset_id: String, shard_id: u64 },

    #[error("Invalid shard configuration: {message}")]
    InvalidShardConfig { message: String },

    // Storage errors
    #[error("Storage error: {message}")]
    Storage { message: String },

    #[error("Storage backend not available: {backend}")]
    StorageUnavailable { backend: String },

    #[error("Storage path not found: {path}")]
    StoragePathNotFound { path: String },

    // Coordination errors
    #[error("Barrier timeout: {barrier_id} (waited {timeout_ms}ms)")]
    BarrierTimeout { barrier_id: String, timeout_ms: u64 },

    #[error("Barrier already exists: {barrier_id}")]
    BarrierExists { barrier_id: String },

    #[error("Coordinator unavailable: {address}")]
    CoordinatorUnavailable { address: String },

    // Configuration errors
    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },

    // I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    // Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    // gRPC errors
    #[error("gRPC error: {0}")]
    Grpc(String),

    // Internal errors
    #[error("Internal error: {message}")]
    Internal { message: String },

    // Timeout errors
    #[error("Operation timeout: {operation} after {timeout_ms}ms")]
    Timeout { operation: String, timeout_ms: u64 },

    // Channel errors
    #[error("Channel closed: {channel}")]
    ChannelClosed { channel: String },
}

impl Error {
    /// Returns true if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Error::WorkerHeartbeatTimeout { .. }
                | Error::Storage { .. }
                | Error::StorageUnavailable { .. }
                | Error::CoordinatorUnavailable { .. }
                | Error::BarrierTimeout { .. }
                | Error::Timeout { .. }
                | Error::Grpc(_)
        )
    }

    /// Returns true if this error indicates a fatal condition
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Error::CheckpointCorrupted { .. }
                | Error::InvalidConfig { .. }
                | Error::Internal { .. }
        )
    }

    /// Returns a retry delay hint in milliseconds, if applicable
    pub fn retry_delay_hint_ms(&self) -> Option<u64> {
        match self {
            Error::WorkerHeartbeatTimeout { .. } => Some(1000),
            Error::Storage { .. } => Some(100),
            Error::StorageUnavailable { .. } => Some(5000),
            Error::CoordinatorUnavailable { .. } => Some(2000),
            Error::BarrierTimeout { .. } => Some(500),
            Error::Timeout { .. } => Some(1000),
            Error::Grpc(_) => Some(100),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_retryable() {
        let err = Error::CoordinatorUnavailable {
            address: "localhost:50051".to_string(),
        };
        assert!(err.is_retryable());

        let err = Error::CheckpointCorrupted {
            checkpoint_id: "ckpt-1".to_string(),
            reason: "checksum mismatch".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_error_fatal() {
        let err = Error::InvalidConfig {
            message: "missing required field".to_string(),
        };
        assert!(err.is_fatal());

        let err = Error::Timeout {
            operation: "write".to_string(),
            timeout_ms: 5000,
        };
        assert!(!err.is_fatal());
    }
}
