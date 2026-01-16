//! Runtime configuration types

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main runtime configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Coordinator settings
    pub coordinator: CoordinatorConfig,

    /// Worker settings
    pub worker: WorkerConfig,

    /// Checkpoint settings
    pub checkpoint: CheckpointConfig,

    /// Storage settings
    pub storage: StorageConfig,

    /// Network settings
    pub network: NetworkConfig,
}

/// Coordinator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Address to bind the coordinator server
    pub bind_address: String,

    /// Port for gRPC server
    pub port: u16,

    /// Maximum number of workers
    pub max_workers: usize,

    /// Worker heartbeat timeout
    #[serde(with = "humantime_serde")]
    pub heartbeat_timeout: Duration,

    /// How often to check for dead workers
    #[serde(with = "humantime_serde")]
    pub dead_worker_check_interval: Duration,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 50051,
            max_workers: 10000,
            heartbeat_timeout: Duration::from_secs(30),
            dead_worker_check_interval: Duration::from_secs(5),
        }
    }
}

/// Worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Coordinator address to connect to
    pub coordinator_address: String,

    /// Worker identifier (auto-generated if not set)
    pub worker_id: Option<String>,

    /// Heartbeat interval
    #[serde(with = "humantime_serde")]
    pub heartbeat_interval: Duration,

    /// Number of async I/O threads
    pub io_threads: usize,

    /// Size of data prefetch buffer
    pub prefetch_buffer_size: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            coordinator_address: "localhost:50051".to_string(),
            worker_id: None,
            heartbeat_interval: Duration::from_secs(5),
            io_threads: 4,
            prefetch_buffer_size: 16,
        }
    }
}

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Checkpoint strategy
    pub strategy: CheckpointStrategy,

    /// Number of checkpoints to keep
    pub keep_count: usize,

    /// Async write buffer size in bytes
    pub write_buffer_size: usize,

    /// Enable compression
    pub compression: bool,

    /// Compression level (1-9)
    pub compression_level: u32,

    /// Write timeout
    #[serde(with = "humantime_serde")]
    pub write_timeout: Duration,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            strategy: CheckpointStrategy::default(),
            keep_count: 5,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            compression: true,
            compression_level: 3,
            write_timeout: Duration::from_secs(300),
        }
    }
}

/// Checkpoint strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointStrategy {
    /// Checkpoint every N steps
    Steps { interval: u64 },

    /// Checkpoint every N seconds
    Time {
        #[serde(with = "humantime_serde")]
        interval: Duration,
    },

    /// Adaptive: checkpoint based on loss improvement
    Adaptive {
        min_steps: u64,
        max_steps: u64,
        loss_threshold: f64,
    },

    /// No automatic checkpointing (manual only)
    Manual,
}

impl Default for CheckpointStrategy {
    fn default() -> Self {
        CheckpointStrategy::Steps { interval: 1000 }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type
    pub backend: StorageBackend,

    /// Base path for storage
    pub base_path: String,

    /// Maximum concurrent I/O operations
    pub max_concurrent_ops: usize,

    /// Retry configuration
    pub retry: RetryConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            base_path: "./data".to_string(),
            max_concurrent_ops: 64,
            retry: RetryConfig::default(),
        }
    }
}

/// Storage backend type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackend {
    /// Local filesystem
    Local,

    /// S3-compatible storage
    S3 {
        endpoint: Option<String>,
        region: String,
        bucket: String,
    },
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,

    /// Initial delay before first retry
    #[serde(with = "humantime_serde")]
    pub initial_delay: Duration,

    /// Maximum delay between retries
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,

    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,

    /// Add jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Connection timeout
    #[serde(with = "humantime_serde")]
    pub connect_timeout: Duration,

    /// Request timeout
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,

    /// Keep-alive interval
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable TLS
    pub tls_enabled: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            keepalive_interval: Duration::from_secs(10),
            max_message_size: 256 * 1024 * 1024, // 256MB
            tls_enabled: false,
        }
    }
}

/// Duration serialization helper for human-readable formats
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert_eq!(config.coordinator.port, 50051);
        assert_eq!(config.checkpoint.keep_count, 5);
    }

    #[test]
    fn test_config_serialization() {
        let config = RuntimeConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RuntimeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coordinator.port, config.coordinator.port);
    }
}
