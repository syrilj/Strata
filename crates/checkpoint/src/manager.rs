//! Checkpoint manager for coordinating distributed checkpoints

use bytes::Bytes;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use runtime_core::{CheckpointId, CheckpointMetadata, CheckpointType, Epoch, Error, Result, Step};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use storage::{LocalStorage, StorageBackend};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::writer::{u8_to_checkpoint_type, AsyncCheckpointWriter, WriteRequest, WriterEvent};

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

    /// Checkpoint type
    pub checkpoint_type: CheckpointType,

    /// Caller metadata
    pub metadata: HashMap<String, String>,

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

/// Internal stored checkpoint record
#[derive(Debug, Clone)]
struct StoredCheckpoint {
    metadata: CheckpointMetadata,
    is_external: bool,
    storage_key: String,
}

/// Thread-safe checkpoint index allowing multiple workers at the same step
#[derive(Debug, Default)]
struct CheckpointIndex {
    by_id: HashMap<CheckpointId, StoredCheckpoint>,
    ordered: BTreeMap<(Step, CheckpointId), CheckpointMetadata>,
}

impl CheckpointIndex {
    fn insert(&mut self, stored: StoredCheckpoint) {
        let key = (stored.metadata.step, stored.metadata.id.clone());
        self.ordered.insert(key, stored.metadata.clone());
        self.by_id.insert(stored.metadata.id.clone(), stored);
    }

    fn remove_by_id(&mut self, id: &str) -> Option<StoredCheckpoint> {
        if let Some(stored) = self.by_id.remove(id) {
            self.ordered
                .remove(&(stored.metadata.step, stored.metadata.id.clone()));
            Some(stored)
        } else {
            None
        }
    }

    fn oldest_local_id(&self) -> Option<CheckpointId> {
        self.ordered
            .values()
            .find(|meta| {
                self.by_id
                    .get(&meta.id)
                    .is_some_and(|stored| !stored.is_external)
            })
            .map(|meta| meta.id.clone())
    }

    fn local_count(&self) -> usize {
        self.by_id.values().filter(|s| !s.is_external).count()
    }
}

/// Checkpoint manager for handling async writes and versioning
pub struct CheckpointManager {
    /// Configuration
    config: CheckpointManagerConfig,

    /// Storage backend
    storage: Arc<dyn StorageBackend>,

    /// Checkpoint metadata index
    checkpoints: Arc<RwLock<CheckpointIndex>>,

    /// Pending writes
    pending: Arc<RwLock<HashMap<CheckpointId, PendingCheckpoint>>>,

    /// Channel to send write requests
    write_tx: mpsc::Sender<WriteRequest>,

    /// Async writer handle
    _writer: AsyncCheckpointWriter,
}

impl CheckpointManager {
    /// Create a new checkpoint manager using `LocalStorage` at `config.base_path`
    pub async fn new(config: CheckpointManagerConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&config.base_path)
            .await
            .map_err(|e| Error::Storage {
                message: format!("Failed to create checkpoint directory: {}", e),
            })?;

        let storage: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(&config.base_path));
        Self::with_storage(config, storage).await
    }

    /// Create a new checkpoint manager with an explicit `StorageBackend`
    pub async fn with_storage(
        config: CheckpointManagerConfig,
        storage: Arc<dyn StorageBackend>,
    ) -> Result<Self> {
        let index = Arc::new(RwLock::new(CheckpointIndex::default()));

        // Best-effort scan of existing checkpoints in base_path (for local filesystem persistence)
        Self::scan_existing_local_checkpoints(&config.base_path, &index);

        // Also try listing from storage backend for any keys not discovered locally (e.g. S3 or custom backend)
        if let Ok(keys) = storage.list("").await {
            for key in keys {
                if !key.ends_with(".ckpt") {
                    continue;
                }
                let file_stem = Path::new(&key)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&key)
                    .to_string();
                if index.read().by_id.contains_key(&file_stem) {
                    continue;
                }
                if let Ok(raw) = storage.read(&key).await {
                    if let Ok((header, _)) = AsyncCheckpointWriter::parse_header_from_bytes(&raw) {
                        let metadata_map: HashMap<String, String> =
                            serde_json::from_str(&header.metadata_json).unwrap_or_default();
                        let path_str = config.base_path.join(&key).to_string_lossy().to_string();
                        let meta = CheckpointMetadata {
                            id: file_stem,
                            step: header.step,
                            epoch: header.epoch,
                            path: path_str,
                            size_bytes: raw.len() as u64,
                            created_at: Utc::now(),
                            checkpoint_type: u8_to_checkpoint_type(header.checkpoint_type),
                            model_hash: None,
                            metadata: metadata_map,
                        };
                        index.write().insert(StoredCheckpoint {
                            metadata: meta,
                            is_external: false,
                            storage_key: key,
                        });
                    }
                }
            }
        }

        let pending = Arc::new(RwLock::new(
            HashMap::<CheckpointId, PendingCheckpoint>::new(),
        ));
        let base_path = config.base_path.clone();
        let keep_count = config.keep_count;

        // Create completion channel
        let (event_tx, mut event_rx) = mpsc::channel(100);

        // Create async writer wired to storage
        let (write_tx, writer) = AsyncCheckpointWriter::with_storage(
            config.base_path.clone(),
            Arc::clone(&storage),
            config.write_buffer_size,
            config.compression,
            event_tx,
        )
        .await?;

        // Spawn event listener task
        let checkpoints_clone = Arc::clone(&index);
        let pending_clone = Arc::clone(&pending);
        let storage_clone = Arc::clone(&storage);

        tokio::spawn(async move {
            debug!("Checkpoint event listener started");
            while let Some(event) = event_rx.recv().await {
                match event {
                    WriterEvent::Completed {
                        checkpoint_id,
                        size_bytes,
                        checkpoint_type,
                        metadata: event_metadata,
                    } => {
                        let mut pending_lock = pending_clone.write();

                        if let Some(entry) = pending_lock.get_mut(&checkpoint_id) {
                            entry.status = WriteStatus::Completed;

                            let filename = format!("{}.ckpt", checkpoint_id);
                            let metadata = CheckpointMetadata {
                                id: checkpoint_id.clone(),
                                step: entry.step,
                                epoch: entry.epoch,
                                path: base_path.join(&filename).to_string_lossy().to_string(),
                                size_bytes,
                                created_at: Utc::now(),
                                checkpoint_type,
                                model_hash: None,
                                metadata: event_metadata,
                            };

                            let step = entry.step;
                            drop(pending_lock);

                            let mut idx = checkpoints_clone.write();
                            idx.insert(StoredCheckpoint {
                                metadata,
                                is_external: false,
                                storage_key: filename,
                            });

                            info!(
                                checkpoint_id = %checkpoint_id,
                                step = step,
                                size_bytes = size_bytes,
                                "Checkpoint write completed"
                            );

                            Self::cleanup_index_locked(&mut idx, keep_count, &storage_clone);
                        }
                    }
                    WriterEvent::Failed {
                        checkpoint_id,
                        error,
                    } => {
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
            storage,
            checkpoints: index,
            pending,
            write_tx,
            _writer: writer,
        })
    }

    /// Best-effort synchronous scan of existing `*.ckpt` files in `base_path`
    fn scan_existing_local_checkpoints(base_path: &Path, index: &Arc<RwLock<CheckpointIndex>>) {
        let Ok(entries) = std::fs::read_dir(base_path) else {
            return;
        };

        let mut idx = index.write();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("ckpt") {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(checkpoint_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            let Ok(raw) = std::fs::read(&path) else {
                continue;
            };
            let Ok((header, _)) = AsyncCheckpointWriter::parse_header_from_bytes(&raw) else {
                continue;
            };

            let metadata_map: HashMap<String, String> =
                serde_json::from_str(&header.metadata_json).unwrap_or_default();
            let created_at = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(Utc::now);

            let meta = CheckpointMetadata {
                id: checkpoint_id.to_string(),
                step: header.step,
                epoch: header.epoch,
                path: path.to_string_lossy().to_string(),
                size_bytes: raw.len() as u64,
                created_at,
                checkpoint_type: u8_to_checkpoint_type(header.checkpoint_type),
                model_hash: None,
                metadata: metadata_map,
            };

            idx.insert(StoredCheckpoint {
                metadata: meta,
                is_external: false,
                storage_key: file_name.to_string(),
            });
        }
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
            checkpoint_type,
            metadata: metadata.clone(),
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
            path,
            step,
            epoch,
            checkpoint_type,
            metadata,
        };

        // Send to async writer
        self.write_tx
            .send(request)
            .await
            .map_err(|e| Error::ChannelClosed {
                channel: format!("checkpoint write channel: {}", e),
            })?;

        debug!(checkpoint_id = %checkpoint_id, step = step, "Queued checkpoint for async write");

        Ok(checkpoint_id)
    }

    /// Mark a checkpoint as completed (called by writer or coordinator)
    pub fn mark_completed(&self, checkpoint_id: &str, size_bytes: u64) -> Result<()> {
        let mut pending = self.pending.write();

        if let Some(entry) = pending.get_mut(checkpoint_id) {
            entry.status = WriteStatus::Completed;
            let step = entry.step;
            let epoch = entry.epoch;
            let checkpoint_type = entry.checkpoint_type;
            let extra_metadata = entry.metadata.clone();
            drop(pending);

            let filename = format!("{}.ckpt", checkpoint_id);
            let metadata = CheckpointMetadata {
                id: checkpoint_id.to_string(),
                step,
                epoch,
                path: self
                    .config
                    .base_path
                    .join(&filename)
                    .to_string_lossy()
                    .to_string(),
                size_bytes,
                created_at: Utc::now(),
                checkpoint_type,
                model_hash: None,
                metadata: extra_metadata,
            };

            let mut idx = self.checkpoints.write();
            idx.insert(StoredCheckpoint {
                metadata,
                is_external: false,
                storage_key: filename,
            });
            info!(
                checkpoint_id = %checkpoint_id,
                step = step,
                size_bytes = size_bytes,
                "Checkpoint write completed"
            );

            Self::cleanup_index_locked(&mut idx, self.config.keep_count, &self.storage);
        } else {
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

        let mut idx = self.checkpoints.write();
        idx.insert(StoredCheckpoint {
            metadata: checkpoint_metadata,
            is_external: true,
            storage_key: path.to_string(),
        });

        info!(
            checkpoint_id = %checkpoint_id,
            step = step,
            epoch = epoch,
            size_bytes = size_bytes,
            "External checkpoint registered"
        );

        Self::cleanup_index_locked(&mut idx, self.config.keep_count, &self.storage);
    }

    /// Delete a checkpoint by ID (removes from index and deletes underlying local/storage file if not external)
    pub async fn delete(&self, checkpoint_id: &str) -> Result<bool> {
        let removed = self.checkpoints.write().remove_by_id(checkpoint_id);
        let Some(stored) = removed else {
            return Ok(false);
        };

        if !stored.is_external {
            if let Err(e) = self.storage.delete(&stored.storage_key).await {
                if !matches!(e, Error::StoragePathNotFound { .. }) {
                    return Err(e);
                }
            }
        }

        Ok(true)
    }

    /// Get checkpoint metadata by ID
    pub fn get_by_id(&self, checkpoint_id: &str) -> Option<CheckpointMetadata> {
        self.checkpoints
            .read()
            .by_id
            .get(checkpoint_id)
            .map(|s| s.metadata.clone())
    }

    /// Get the latest checkpoint
    pub fn latest(&self) -> Option<CheckpointMetadata> {
        self.checkpoints.read().ordered.values().last().cloned()
    }

    /// Get checkpoint by step (returns the first/matching checkpoint at `step`)
    pub fn get_by_step(&self, step: Step) -> Option<CheckpointMetadata> {
        self.checkpoints
            .read()
            .ordered
            .range((step, String::new())..)
            .take_while(|((s, _), _)| *s == step)
            .map(|(_, m)| m.clone())
            .next()
    }

    /// Get all checkpoints ordered by `(step, checkpoint_id)`
    pub fn all_checkpoints(&self) -> Vec<CheckpointMetadata> {
        self.checkpoints.read().ordered.values().cloned().collect()
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

            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
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

    /// Cleanup old local checkpoints beyond keep_count without deleting remote/external files
    #[allow(dead_code)]
    fn cleanup_old_checkpoints(&self) {
        let mut idx = self.checkpoints.write();
        Self::cleanup_index_locked(&mut idx, self.config.keep_count, &self.storage);
    }

    fn cleanup_index_locked(
        idx: &mut CheckpointIndex,
        keep_count: usize,
        storage: &Arc<dyn StorageBackend>,
    ) {
        while idx.local_count() > keep_count {
            let Some(oldest_id) = idx.oldest_local_id() else {
                break;
            };
            if let Some(stored) = idx.remove_by_id(&oldest_id) {
                if !stored.is_external {
                    let key = stored.storage_key;
                    let storage_ref = Arc::clone(storage);
                    tokio::spawn(async move {
                        if let Err(e) = storage_ref.delete(&key).await {
                            warn!(key = %key, error = %e, "Failed to delete old checkpoint");
                        } else {
                            debug!(key = %key, "Deleted old checkpoint");
                        }
                    });
                }
            }
        }
    }

    /// Load checkpoint data by checkpoint ID
    pub async fn load(&self, checkpoint_id: &str) -> Result<Bytes> {
        let stored = self
            .checkpoints
            .read()
            .by_id
            .get(checkpoint_id)
            .cloned()
            .ok_or_else(|| Error::CheckpointNotFound {
                checkpoint_id: checkpoint_id.to_string(),
            })?;

        if stored.is_external {
            AsyncCheckpointWriter::read_checkpoint_data(&PathBuf::from(&stored.metadata.path)).await
        } else {
            AsyncCheckpointWriter::read_checkpoint_from_storage(&*self.storage, &stored.storage_key)
                .await
        }
    }

    /// Find the best checkpoint for recovery
    pub fn find_recovery_checkpoint(&self) -> Option<CheckpointMetadata> {
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

    #[tokio::test]
    async fn test_preserves_type_and_metadata_and_roundtrips_lz4() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            compression: true,
            ..Default::default()
        };

        let manager = CheckpointManager::new(config).await.unwrap();
        let mut meta = HashMap::new();
        meta.insert("worker_id".to_string(), "worker-0".to_string());
        meta.insert("loss".to_string(), "0.123".to_string());

        let payload = Bytes::from(vec![7u8; 4096]);
        let ckpt_id = manager
            .save_async(
                payload.clone(),
                10,
                1,
                CheckpointType::OptimizerOnly,
                meta.clone(),
            )
            .await
            .unwrap();
        manager.wait_pending().await.unwrap();

        let saved = manager.get_by_id(&ckpt_id).unwrap();
        assert_eq!(saved.checkpoint_type, CheckpointType::OptimizerOnly);
        assert_eq!(
            saved.metadata.get("worker_id").map(String::as_str),
            Some("worker-0")
        );
        assert_eq!(
            saved.metadata.get("loss").map(String::as_str),
            Some("0.123")
        );

        let loaded = manager.load(&ckpt_id).await.unwrap();
        assert_eq!(loaded, payload);
    }

    #[tokio::test]
    async fn test_concurrent_same_step_checkpoints_do_not_overwrite() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            keep_count: 10,
            ..Default::default()
        };

        let manager = CheckpointManager::new(config).await.unwrap();

        let mut meta1 = HashMap::new();
        meta1.insert("worker_id".to_string(), "w1".to_string());
        let mut meta2 = HashMap::new();
        meta2.insert("worker_id".to_string(), "w2".to_string());

        let id1 = manager
            .save_async(
                Bytes::from_static(b"data1"),
                50,
                2,
                CheckpointType::Full,
                meta1,
            )
            .await
            .unwrap();
        let id2 = manager
            .save_async(
                Bytes::from_static(b"data2"),
                50,
                2,
                CheckpointType::ModelOnly,
                meta2,
            )
            .await
            .unwrap();
        manager.wait_pending().await.unwrap();

        let all = manager.all_checkpoints();
        assert_eq!(all.len(), 2);
        assert!(manager.get_by_id(&id1).is_some());
        assert!(manager.get_by_id(&id2).is_some());
        assert_eq!(
            manager.load(&id1).await.unwrap(),
            Bytes::from_static(b"data1")
        );
        assert_eq!(
            manager.load(&id2).await.unwrap(),
            Bytes::from_static(b"data2")
        );
    }

    #[tokio::test]
    async fn test_reload_existing_checkpoints_on_startup_and_delete() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            compression: true,
            ..Default::default()
        };

        let ckpt_id = {
            let manager = CheckpointManager::new(config.clone()).await.unwrap();
            let mut meta = HashMap::new();
            meta.insert("tag".to_string(), "persist".to_string());
            let id = manager
                .save_async(
                    Bytes::from_static(b"persisted-state"),
                    42,
                    3,
                    CheckpointType::Incremental,
                    meta,
                )
                .await
                .unwrap();
            manager.wait_pending().await.unwrap();
            id
        };

        // Re-open manager in the same directory
        let reloaded = CheckpointManager::new(config).await.unwrap();
        let meta = reloaded.get_by_id(&ckpt_id).unwrap();
        assert_eq!(meta.step, 42);
        assert_eq!(meta.epoch, 3);
        assert_eq!(meta.checkpoint_type, CheckpointType::Incremental);
        assert_eq!(
            meta.metadata.get("tag").map(String::as_str),
            Some("persist")
        );
        assert_eq!(
            reloaded.load(&ckpt_id).await.unwrap(),
            Bytes::from_static(b"persisted-state")
        );

        assert!(reloaded.delete(&ckpt_id).await.unwrap());
        assert!(reloaded.get_by_id(&ckpt_id).is_none());
        assert!(!reloaded.delete(&ckpt_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_external_checkpoint_not_deleted_on_cleanup() {
        let dir = tempdir().unwrap();
        let external_file = dir.path().join("remote_worker_ckpt.bin");
        tokio::fs::write(&external_file, b"external").await.unwrap();

        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            keep_count: 1,
            ..Default::default()
        };
        let manager = CheckpointManager::new(config).await.unwrap();

        manager.register_external_checkpoint(
            "ext-1",
            1,
            0,
            external_file.to_str().unwrap(),
            8,
            HashMap::new(),
        );
        manager.register_external_checkpoint(
            "ext-2",
            2,
            0,
            external_file.to_str().unwrap(),
            8,
            HashMap::new(),
        );

        // Give any spawned tasks a moment to run if they were erroneously spawned
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(external_file.exists());
        assert!(manager.get_by_id("ext-1").is_some());
        assert!(manager.get_by_id("ext-2").is_some());
    }
}
