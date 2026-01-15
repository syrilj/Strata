//! Checkpoint manager for coordinating distributed checkpoints

use bytes::Bytes;
use chrono::Utc;
use parking_lot::RwLock;
use runtime_core::{
    CheckpointId, CheckpointMetadata, CheckpointType, Epoch, Error, Result, Step,
};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::writer::{AsyncCheckpointWriter, WriteRequest, WriterEvent};

/// Checkpoint manager configuration
#[derive(Debug, Clone)]
pub struct CheckpointManagerConfig {
    /// Base path for checkpoints
    pub base_path: PathBuf,

    /// Number of checkpoints to keep
    pub keep_count: usize,

    /// Buffer size for async writes
    pub write_buffer_size: usize,

    /// Enable compression
    pub compression: bool,

    /// Compression level (1-9)
    pub compression_level: u32,
}

impl Default for CheckpointManagerConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("./checkpoints"),
            keep_count: 5,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            compression: true,
            compression_level: 3,
        }
    }
}

/// Pending checkpoint write status
#[derive(Debug, Clone)]
pub struct PendingCheckpoint {
    /// Checkpoint ID
    pub id: CheckpointId,

    /// Training step
    pub step: Step,

    /// Training epoch
    pub epoch: Epoch,

    /// Write status
    pub status: WriteStatus,

    /// Error message if failed
    pub error: Option<String>,
}

/// Write status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteStatus {
    /// Write is pending
    Pending,

    /// Write is in progress
    InProgress,

    /// Write completed successfully
    Completed,

    /// Write failed
    Failed,
}

/// Checkpoint manager for handling async writes and versioning
pub struct CheckpointManager {
    /// Configuration
    config: CheckpointManagerConfig,

    /// Checkpoint metadata indexed by step
    checkpoints: Arc<RwLock<BTreeMap<Step, CheckpointMetadata>>>,

    /// Pending writes
    pending: Arc<RwLock<HashMap<CheckpointId, PendingCheckpoint>>>,

    /// Channel to send write requests
    write_tx: mpsc::Sender<WriteRequest>,

    /// Async writer handle
    _writer: AsyncCheckpointWriter,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub async fn new(config: CheckpointManagerConfig) -> Result<Self> {
        // Create checkpoint directory
        tokio::fs::create_dir_all(&config.base_path)
            .await
            .map_err(|e| Error::Storage {
                message: format!("Failed to create checkpoint directory: {}", e),
            })?;

        // Shared state
        let checkpoints = Arc::new(RwLock::new(BTreeMap::new()));
        let pending = Arc::new(RwLock::new(HashMap::<CheckpointId, PendingCheckpoint>::new()));
        let base_path = config.base_path.clone();
        let keep_count = config.keep_count;

        // Create completion channel
        let (event_tx, mut event_rx) = mpsc::channel(100);

        // Create async writer
        let (write_tx, writer) = AsyncCheckpointWriter::new(
            config.base_path.clone(),
            config.write_buffer_size,
            config.compression,
            event_tx,
        )
        .await?;

        // Spawn event listener task
        let checkpoints_clone = checkpoints.clone();
        let pending_clone = pending.clone();
        
        tokio::spawn(async move {
            debug!("Checkpoint event listener started");
            while let Some(event) = event_rx.recv().await {
                match event {
                    WriterEvent::Completed { checkpoint_id, size_bytes } => {
                        let mut pending_lock = pending_clone.write();
                        
                        if let Some(entry) = pending_lock.get_mut(&checkpoint_id) {
                            entry.status = WriteStatus::Completed;

                             // Create metadata and store
                            let metadata = CheckpointMetadata {
                                id: checkpoint_id.clone(),
                                step: entry.step,
                                epoch: entry.epoch,
                                path: base_path
                                    .join(format!("{}.ckpt", checkpoint_id))
                                    .to_string_lossy()
                                    .to_string(),
                                size_bytes,
                                created_at: Utc::now(),
                                checkpoint_type: CheckpointType::Full, // TODO: preserve type
                                model_hash: None,
                                metadata: HashMap::new(),
                            };

                            checkpoints_clone.write().insert(entry.step, metadata);
                             info!(
                                checkpoint_id = %checkpoint_id,
                                step = entry.step,
                                size_bytes = size_bytes,
                                "Checkpoint write completed"
                            );
                            
                            // Cleanup old checkpoints
                             let mut checkpoints_lock = checkpoints_clone.write();
                             while checkpoints_lock.len() > keep_count {
                                if let Some((&step, _)) = checkpoints_lock.first_key_value() {
                                    if let Some(meta) = checkpoints_lock.remove(&step) {
                                        let path = meta.path.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = tokio::fs::remove_file(&path).await {
                                                warn!(path = %path, error = %e, "Failed to delete old checkpoint");
                                            } else {
                                                debug!(path = %path, "Deleted old checkpoint");
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                    WriterEvent::Failed { checkpoint_id, error } => {
                        let mut pending_lock = pending_clone.write();
                        if let Some(entry) = pending_lock.get_mut(&checkpoint_id) {
                            entry.status = WriteStatus::Failed;
                            entry.error = Some(error.clone());
                            error!(
                                checkpoint_id = %checkpoint_id,
                                error = %error,
                                "Checkpoint write failed"
                            );
                        }
                    }
                }
            }
            debug!("Checkpoint event listener stopped");
        });

        Ok(Self {
            config,
            checkpoints,
            pending,
            write_tx,
            _writer: writer,
        })
    }

    /// Save a checkpoint asynchronously (non-blocking)
    pub async fn save_async(
        &self,
        data: Bytes,
        step: Step,
        epoch: Epoch,
        checkpoint_type: CheckpointType,
        metadata: HashMap<String, String>,
    ) -> Result<CheckpointId> {
        let checkpoint_id = format!("ckpt-{}-{}", step, Uuid::new_v4());

        // Create pending entry
        let pending = PendingCheckpoint {
            id: checkpoint_id.clone(),
            step,
            epoch,
            status: WriteStatus::Pending,
            error: None,
        };
        self.pending.write().insert(checkpoint_id.clone(), pending);

        // Generate path
        let filename = format!("{}.ckpt", checkpoint_id);
        let path = self.config.base_path.join(&filename);

        // Create write request
        let request = WriteRequest {
            checkpoint_id: checkpoint_id.clone(),
            data,
            path: path.clone(),
            step,
            epoch,
            checkpoint_type,
            metadata: metadata.clone(),
        };

        // Send to async writer
        self.write_tx.send(request).await.map_err(|e| {
            Error::ChannelClosed {
                channel: format!("checkpoint write channel: {}", e),
            }
        })?;

        debug!(checkpoint_id = %checkpoint_id, step = step, "Queued checkpoint for async write");

        Ok(checkpoint_id)
    }

    /// Mark a checkpoint as completed (called by writer or coordinator)
    pub fn mark_completed(&self, checkpoint_id: &str, size_bytes: u64) -> Result<()> {
        let mut pending = self.pending.write();

        // Even if not in pending (e.g. from coordinator), we might want to register it
        // But typically we expect it to be in pending if we are tracking it
        
        // If entry exists, update it. If not, we might be registering a remote checkpoint.
        // For now, let's assume if it's not in pending, we just add it to checkpoints directly?
        // But PendingCheckpoint stores step/epoch. If we don't have it, we can't easily add to checkpoints map 
        // without more info.
        // However, the coordinator calls this after `NotifyCheckpoint`.
        
        if let Some(entry) = pending.get_mut(checkpoint_id) {
            entry.status = WriteStatus::Completed;
            let step = entry.step;
            let epoch = entry.epoch;
            
            // Allow releasing lock before acquiring checkpoints lock to avoid deadlock?
            // RwLock is reentrant? No. parking_lot::RwLock is not reentrant.
            // But we are taking write lock on pending, then write lock on checkpoints. 
            // We need to be careful about lock order. 
            // In listener we did: pending.write(), then checkpoints.write().
            // Here we should do the same.
            
            // Create metadata and store
            let metadata = CheckpointMetadata {
                id: checkpoint_id.to_string(),
                step,
                epoch,
                path: self
                    .config
                    .base_path
                    .join(format!("{}.ckpt", checkpoint_id))
                    .to_string_lossy()
                    .to_string(),
                size_bytes,
                created_at: Utc::now(),
                checkpoint_type: CheckpointType::Full, // TODO: preserve type
                model_hash: None,
                metadata: HashMap::new(),
            };

            self.checkpoints.write().insert(step, metadata);
            info!(
                checkpoint_id = %checkpoint_id,
                step = step,
                size_bytes = size_bytes,
                "Checkpoint write completed"
            );

            // Cleanup old checkpoints
            self.cleanup_old_checkpoints();
        } else {
            // Case for coordinator receiving notification for a checkpoint it didn't initiate?
            // If coordinator just uses this to track state, maybe it inserted a pending entry first?
            // Let's check coordinator usage if possible, but for now restoring the logic 
            // that relies on pending entry validation is safer.
            warn!(checkpoint_id = %checkpoint_id, "Attempted to mark unknown checkpoint as completed");
        }

        Ok(())
    }

    /// Mark a checkpoint as failed
    pub fn mark_failed(&self, checkpoint_id: &str, error: String) {
        let mut pending = self.pending.write();
        if let Some(entry) = pending.get_mut(checkpoint_id) {
            entry.status = WriteStatus::Failed;
            entry.error = Some(error.clone());
            error!(
                checkpoint_id = %checkpoint_id,
                error = %error,
                "Checkpoint write failed"
            );
        }
    }

    /// Register an external checkpoint (from remote workers via gRPC)
    /// This is used when the coordinator receives a checkpoint notification 
    /// that it didn't initiate locally
    pub fn register_external_checkpoint(
        &self,
        checkpoint_id: &str,
        step: Step,
        epoch: Epoch,
        path: &str,
        size_bytes: u64,
        metadata: HashMap<String, String>,
    ) {
        let checkpoint_metadata = CheckpointMetadata {
            id: checkpoint_id.to_string(),
            step,
            epoch,
            path: path.to_string(),
            size_bytes,
            created_at: Utc::now(),
            checkpoint_type: CheckpointType::Full,
            model_hash: None,
            metadata,
        };

        let mut checkpoints = self.checkpoints.write();
        // Check if a checkpoint already exists at this step and log a warning
        if checkpoints.contains_key(&step) {
            tracing::warn!(
                checkpoint_id = %checkpoint_id,
                step = step,
                "Overwriting existing checkpoint at step"
            );
        }
        checkpoints.insert(step, checkpoint_metadata);
        drop(checkpoints);
        
        info!(
            checkpoint_id = %checkpoint_id,
            step = step,
            epoch = epoch,
            size_bytes = size_bytes,
            "External checkpoint registered"
        );

        // Cleanup old checkpoints
        self.cleanup_old_checkpoints();
    }

    /// Get the latest checkpoint
    pub fn latest(&self) -> Option<CheckpointMetadata> {
        self.checkpoints
            .read()
            .values()
            .last()
            .cloned()
    }

    /// Get checkpoint by step
    pub fn get_by_step(&self, step: Step) -> Option<CheckpointMetadata> {
        self.checkpoints.read().get(&step).cloned()
    }

    /// Get all checkpoints
    pub fn all_checkpoints(&self) -> Vec<CheckpointMetadata> {
        self.checkpoints.read().values().cloned().collect()
    }

    /// Get pending writes
    pub fn pending_writes(&self) -> Vec<PendingCheckpoint> {
        self.pending.read().values().cloned().collect()
    }

    /// Wait for all pending writes to complete
    pub async fn wait_pending(&self) -> Result<()> {
        loop {
            let has_pending = {
                let pending = self.pending.read();
                pending.values().any(|p| {
                    p.status == WriteStatus::Pending || p.status == WriteStatus::InProgress
                })
            };

            if !has_pending {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Check for failures
        let failures: Vec<_> = self
            .pending
            .read()
            .values()
            .filter(|p| p.status == WriteStatus::Failed)
            .cloned()
            .collect();

        if !failures.is_empty() {
            let errors: Vec<_> = failures
                .iter()
                .map(|f| {
                    format!(
                        "{}: {}",
                        f.id,
                        f.error.as_deref().unwrap_or("unknown error")
                    )
                })
                .collect();
            return Err(Error::CheckpointWriteFailed {
                message: errors.join(", "),
            });
        }

        Ok(())
    }

    /// Cleanup old checkpoints beyond keep_count
    fn cleanup_old_checkpoints(&self) {
        let mut checkpoints = self.checkpoints.write();

        while checkpoints.len() > self.config.keep_count {
            if let Some((&step, _)) = checkpoints.first_key_value() {
                if let Some(meta) = checkpoints.remove(&step) {
                    // Delete file asynchronously (fire and forget)
                    let path = meta.path.clone();
                    tokio::spawn(async move {
                        if let Err(e) = tokio::fs::remove_file(&path).await {
                            warn!(path = %path, error = %e, "Failed to delete old checkpoint");
                        } else {
                            debug!(path = %path, "Deleted old checkpoint");
                        }
                    });
                }
            }
        }
    }

    /// Load checkpoint data from path
    pub async fn load(&self, checkpoint_id: &str) -> Result<Bytes> {
        let meta = self
            .checkpoints
            .read()
            .values()
            .find(|m| m.id == checkpoint_id)
            .cloned()
            .ok_or_else(|| Error::CheckpointNotFound {
                checkpoint_id: checkpoint_id.to_string(),
            })?;

        AsyncCheckpointWriter::read_checkpoint_data(&PathBuf::from(&meta.path)).await
    }

    /// Find the best checkpoint for recovery
    pub fn find_recovery_checkpoint(&self) -> Option<CheckpointMetadata> {
        // Return the latest complete checkpoint
        self.latest()
    }
}

/// Thread-safe handle to checkpoint manager
pub type CheckpointManagerHandle = Arc<CheckpointManager>;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_checkpoint_manager_creation() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };

        let manager = CheckpointManager::new(config).await.unwrap();
        assert!(manager.latest().is_none());
    }
}
