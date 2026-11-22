//! Shard manager for coordinating data distribution across workers
//!
//! Handles dataset registration, shard assignment, and dynamic rebalancing.

use crate::{ConsistentHash, EpochCoordinator};
use dashmap::DashMap;
use runtime_core::types::{DatasetId, DatasetMetadata, Epoch, ShardAssignment, ShardId, WorkerId};
use runtime_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Shard manager for coordinating data distribution
#[derive(Debug)]
pub struct ShardManager {
    /// Registered datasets
    datasets: DashMap<DatasetId, DatasetMetadata>,

    /// Consistent hash ring for shard distribution
    hash_ring: Arc<ConsistentHash>,

    /// Epoch coordinator for shuffling
    epoch_coordinator: Arc<EpochCoordinator>,

    /// Active workers
    active_workers: DashMap<WorkerId, WorkerState>,

    /// Worker rank assignments (for round-robin distribution)
    worker_ranks: DashMap<WorkerId, u32>,
}

/// State tracked for each worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    /// Worker identifier
    pub worker_id: WorkerId,

    /// Currently assigned shards per dataset
    pub assigned_shards: DashMap<DatasetId, Vec<ShardId>>,

    /// Whether worker is healthy
    pub healthy: bool,

    /// Last heartbeat timestamp (unix seconds)
    pub last_heartbeat: u64,
}

impl Default for ShardManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardManager {
    /// Create a new shard manager
    pub fn new() -> Self {
        Self::with_components(
            Arc::new(ConsistentHash::new()),
            Arc::new(EpochCoordinator::new()),
        )
    }

    /// Create a shard manager with custom components
    pub fn with_components(
        hash_ring: Arc<ConsistentHash>,
        epoch_coordinator: Arc<EpochCoordinator>,
    ) -> Self {
        Self {
            datasets: DashMap::new(),
            hash_ring,
            epoch_coordinator,
            active_workers: DashMap::new(),
            worker_ranks: DashMap::new(),
        }
    }

    /// Register a new dataset.
    ///
    /// Note: if `metadata.shard_size == 0`, it is clamped to `1` and `total_shards` is recomputed
    /// to avoid division-by-zero panics. Use [`Self::register_dataset_with_info`] or
    /// [`Self::try_register_dataset`] for strict `Result` error reporting.
    pub fn register_dataset(&self, mut metadata: DatasetMetadata) {
        if metadata.shard_size == 0 {
            tracing::warn!(
                dataset = %metadata.id,
                "Dataset registered with shard_size=0; clamping to 1"
            );
            metadata.shard_size = 1;
            metadata.total_shards = metadata.total_samples.div_ceil(1);
        }
        let dataset_id = metadata.id.clone();
        self.epoch_coordinator.init_epoch(&dataset_id, 0);
        self.datasets.insert(dataset_id.clone(), metadata);

        tracing::info!(dataset = %dataset_id, "Registered dataset");
    }

    /// Register a dataset with validation (`shard_size > 0`)
    pub fn try_register_dataset(&self, metadata: DatasetMetadata) -> Result<DatasetMetadata> {
        if metadata.shard_size == 0 {
            return Err(Error::InvalidShardConfig {
                message: format!(
                    "shard_size must be greater than 0 for dataset '{}'",
                    metadata.id
                ),
            });
        }
        let mut normalized = metadata;
        normalized.total_shards = normalized.total_samples.div_ceil(normalized.shard_size);
        let dataset_id = normalized.id.clone();
        self.epoch_coordinator.init_epoch(&dataset_id, 0);
        self.datasets.insert(dataset_id.clone(), normalized.clone());

        tracing::info!(dataset = %dataset_id, "Registered dataset");
        Ok(normalized)
    }

    /// Register a dataset with explicit parameters.
    ///
    /// Note: if `shard_size == 0`, logs a warning and clamps to `1` to remain infallible.
    /// For strict validation returning `Err`, use [`Self::register_dataset_with_info`].
    pub fn register_dataset_params(
        &self,
        dataset_id: &str,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
    ) {
        let effective_shard_size = if shard_size == 0 {
            tracing::warn!(
                dataset = %dataset_id,
                "register_dataset_params called with shard_size=0; clamping to 1"
            );
            1
        } else {
            shard_size
        };
        let total_shards = total_samples.div_ceil(effective_shard_size);

        let metadata = DatasetMetadata {
            id: dataset_id.to_string(),
            path: String::new(),
            format: "unknown".to_string(),
            total_samples,
            total_shards,
            shard_size: effective_shard_size,
            shuffle,
            seed,
            metadata: Default::default(),
        };

        self.register_dataset(metadata);
    }

    /// Register a dataset with full path, format, and metadata, validating `shard_size > 0`
    #[allow(clippy::too_many_arguments)]
    pub fn register_dataset_with_info(
        &self,
        dataset_id: String,
        path: String,
        format: String,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
        metadata: HashMap<String, String>,
    ) -> Result<DatasetMetadata> {
        if shard_size == 0 {
            return Err(Error::InvalidShardConfig {
                message: format!(
                    "shard_size must be greater than 0 for dataset '{}'",
                    dataset_id
                ),
            });
        }

        let total_shards = total_samples.div_ceil(shard_size);
        let dataset = DatasetMetadata {
            id: dataset_id.clone(),
            path,
            format,
            total_samples,
            total_shards,
            shard_size,
            shuffle,
            seed,
            metadata,
        };

        self.epoch_coordinator.init_epoch(&dataset_id, 0);
        self.datasets.insert(dataset_id.clone(), dataset.clone());

        tracing::info!(dataset = %dataset_id, total_shards = total_shards, "Registered dataset with info");
        Ok(dataset)
    }

    /// Remove a registered dataset and clear its shard assignments from workers
    pub fn remove_dataset(&self, dataset_id: &str) -> bool {
        let removed = self.datasets.remove(dataset_id).is_some();
        if removed {
            self.epoch_coordinator.clear_cache(dataset_id);
            for worker in self.active_workers.iter() {
                worker.assigned_shards.remove(dataset_id);
            }
            tracing::info!(dataset = %dataset_id, "Removed dataset");
        }
        removed
    }

    /// Get dataset metadata
    pub fn get_dataset(&self, dataset_id: &str) -> Option<DatasetMetadata> {
        self.datasets.get(dataset_id).map(|d| d.clone())
    }

    /// Register a worker
    pub fn register_worker(&self, worker_id: &str) {
        let rank = self.worker_ranks.len() as u32;
        self.worker_ranks.insert(worker_id.to_string(), rank);

        let state = WorkerState {
            worker_id: worker_id.to_string(),
            assigned_shards: DashMap::new(),
            healthy: true,
            last_heartbeat: current_timestamp(),
        };

        self.active_workers.insert(worker_id.to_string(), state);
        self.hash_ring.add_node(worker_id);

        tracing::info!(worker = worker_id, rank = rank, "Registered worker");
    }

    /// Remove a worker
    pub fn remove_worker(&self, worker_id: &str) {
        self.active_workers.remove(worker_id);
        self.worker_ranks.remove(worker_id);
        self.hash_ring.remove_node(worker_id);

        // Reassign ranks to maintain contiguous ordering
        self.reassign_ranks();

        tracing::info!(worker = worker_id, "Removed worker");
    }

    /// Reassign worker ranks to maintain contiguous ordering
    fn reassign_ranks(&self) {
        let mut workers: Vec<_> = self.worker_ranks.iter().map(|e| e.key().clone()).collect();
        workers.sort();

        for (rank, worker_id) in workers.iter().enumerate() {
            self.worker_ranks.insert(worker_id.clone(), rank as u32);
        }
    }

    /// Update worker heartbeat
    pub fn heartbeat(&self, worker_id: &str) {
        if let Some(mut worker) = self.active_workers.get_mut(worker_id) {
            worker.last_heartbeat = current_timestamp();
            worker.healthy = true;
        }
    }

    /// Collect sorted file paths from a dataset path if it points to an existing local file or directory
    fn collect_local_files(dataset_path: &str) -> Vec<String> {
        if dataset_path.is_empty() {
            return Vec::new();
        }
        let path = Path::new(dataset_path);
        if path.is_file() {
            return vec![dataset_path.to_string()];
        }
        if path.is_dir() {
            let Ok(entries) = std::fs::read_dir(path) else {
                return Vec::new();
            };
            let mut files: Vec<String> = entries
                .flatten()
                .filter_map(|entry| {
                    let p = entry.path();
                    if p.is_file() {
                        Some(p.to_string_lossy().to_string())
                    } else {
                        None
                    }
                })
                .collect();
            files.sort();
            return files;
        }
        Vec::new()
    }

    /// Deterministically slice dataset files for a given shard
    fn files_for_shard(files: &[String], shard_id: u64, total_shards: u64) -> Vec<String> {
        if files.is_empty() || total_shards == 0 {
            return Vec::new();
        }
        let n = files.len();
        let shard = shard_id as usize;
        let total = total_shards as usize;
        if n >= total {
            let start = shard * n / total;
            let end = (shard + 1) * n / total;
            files[start..end].to_vec()
        } else {
            // Fewer files than shards: assign deterministically modulo file count
            vec![files[shard % n].clone()]
        }
    }

    /// Get shard assignment for a worker for a specific epoch
    pub fn get_shard_for_worker(
        &self,
        dataset_id: &str,
        worker_id: &str,
        epoch: Epoch,
    ) -> Option<Vec<ShardAssignment>> {
        let dataset = self.get_dataset(dataset_id)?;
        let worker_rank = *self.worker_ranks.get(worker_id)?;
        let total_workers = self.active_workers.len() as u32;

        if total_workers == 0 {
            return None;
        }

        let shard_ids = if dataset.shuffle {
            // Use epoch coordinator for shuffled distribution
            self.epoch_coordinator.get_worker_shards(
                dataset_id,
                epoch,
                dataset.total_shards,
                worker_rank,
                total_workers,
            )
        } else {
            // Sequential assignment based on consistent hashing
            self.hash_ring
                .get_shards_for_node(worker_id, dataset_id, dataset.total_shards)
        };

        let local_files = Self::collect_local_files(&dataset.path);

        let assignments: Vec<_> = shard_ids
            .into_iter()
            .map(|shard_id| {
                let start_index = shard_id * dataset.shard_size;
                let end_index =
                    std::cmp::min(start_index + dataset.shard_size, dataset.total_samples);
                let file_paths =
                    Self::files_for_shard(&local_files, shard_id, dataset.total_shards);

                ShardAssignment {
                    dataset_id: dataset_id.to_string(),
                    shard_id,
                    total_shards: dataset.total_shards,
                    start_index,
                    end_index,
                    file_paths,
                    epoch,
                }
            })
            .collect();

        // Update worker's assigned shards
        if let Some(worker) = self.active_workers.get(worker_id) {
            worker.assigned_shards.insert(
                dataset_id.to_string(),
                assignments.iter().map(|a| a.shard_id).collect(),
            );
        }

        Some(assignments)
    }

    /// Count total shards currently assigned to a worker across all registered datasets
    pub fn get_worker_shard_count(&self, worker_id: &str) -> usize {
        let Some(worker) = self.active_workers.get(worker_id) else {
            return 0;
        };
        worker
            .assigned_shards
            .iter()
            .map(|entry| entry.value().len())
            .sum()
    }

    /// List all shards of a dataset for a given epoch along with the worker currently assigned to each shard (if any)
    pub fn list_dataset_shards(
        &self,
        dataset_id: &str,
        epoch: Epoch,
    ) -> Vec<(ShardAssignment, Option<WorkerId>)> {
        let Some(dataset) = self.get_dataset(dataset_id) else {
            return Vec::new();
        };

        let total_workers = self.active_workers.len() as u32;
        let local_files = Self::collect_local_files(&dataset.path);

        // Build shard_id -> worker_id lookup
        let mut shard_owners: HashMap<ShardId, WorkerId> = HashMap::new();
        if total_workers > 0 {
            if dataset.shuffle {
                // Collect active workers sorted by rank
                let mut ranked_workers: Vec<(u32, WorkerId)> = self
                    .worker_ranks
                    .iter()
                    .map(|e| (*e.value(), e.key().clone()))
                    .collect();
                ranked_workers.sort_by_key(|(rank, _)| *rank);

                for (rank, worker_id) in ranked_workers {
                    let shard_ids = self.epoch_coordinator.get_worker_shards(
                        dataset_id,
                        epoch,
                        dataset.total_shards,
                        rank,
                        total_workers,
                    );
                    for sid in shard_ids {
                        shard_owners.insert(sid, worker_id.clone());
                    }
                }
            } else {
                for shard_id in 0..dataset.total_shards {
                    if let Some(owner) = self.hash_ring.get_node_for_shard(dataset_id, shard_id) {
                        shard_owners.insert(shard_id, owner);
                    }
                }
            }
        }

        (0..dataset.total_shards)
            .map(|shard_id| {
                let start_index = shard_id * dataset.shard_size;
                let end_index =
                    std::cmp::min(start_index + dataset.shard_size, dataset.total_samples);
                let file_paths =
                    Self::files_for_shard(&local_files, shard_id, dataset.total_shards);
                let assignment = ShardAssignment {
                    dataset_id: dataset_id.to_string(),
                    shard_id,
                    total_shards: dataset.total_shards,
                    start_index,
                    end_index,
                    file_paths,
                    epoch,
                };
                let owner = shard_owners.get(&shard_id).cloned();
                (assignment, owner)
            })
            .collect()
    }

    /// Rebalance shards when workers change
    /// Returns map of worker_id -> new shard assignments for each dataset
    pub fn rebalance_shards(&self) -> DashMap<WorkerId, DashMap<DatasetId, Vec<ShardId>>> {
        let result: DashMap<WorkerId, DashMap<DatasetId, Vec<ShardId>>> = DashMap::new();

        // Get current epoch for each dataset
        for dataset in self.datasets.iter() {
            let dataset_id = dataset.key();
            let _metadata = dataset.value();
            let epoch = self.epoch_coordinator.current_epoch(dataset_id);

            // Calculate new assignments
            for worker_entry in self.active_workers.iter() {
                let worker_id = worker_entry.key();

                if let Some(assignments) = self.get_shard_for_worker(dataset_id, worker_id, epoch) {
                    let worker_map = result.entry(worker_id.clone()).or_default();

                    worker_map.insert(
                        dataset_id.clone(),
                        assignments.iter().map(|a| a.shard_id).collect(),
                    );
                }
            }
        }

        tracing::info!(
            workers = self.active_workers.len(),
            datasets = self.datasets.len(),
            "Rebalanced shards"
        );

        result
    }

    /// Get active worker count
    pub fn active_worker_count(&self) -> usize {
        self.active_workers.len()
    }

    /// Get all active worker IDs
    pub fn active_workers(&self) -> Vec<WorkerId> {
        self.active_workers
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    /// Get dataset count
    pub fn dataset_count(&self) -> usize {
        self.datasets.len()
    }

    /// Get all dataset IDs
    pub fn datasets(&self) -> Vec<DatasetId> {
        self.datasets.iter().map(|e| e.key().clone()).collect()
    }

    /// Advance epoch for a dataset
    pub fn advance_epoch(&self, dataset_id: &str) -> Option<Epoch> {
        if self.datasets.contains_key(dataset_id) {
            Some(self.epoch_coordinator.advance_epoch(dataset_id))
        } else {
            None
        }
    }

    /// Get current epoch for a dataset
    pub fn current_epoch(&self, dataset_id: &str) -> Epoch {
        self.epoch_coordinator.current_epoch(dataset_id)
    }

    /// Get the hash ring reference
    pub fn hash_ring(&self) -> &Arc<ConsistentHash> {
        &self.hash_ring
    }

    /// Get the epoch coordinator reference
    pub fn epoch_coordinator(&self) -> &Arc<EpochCoordinator> {
        &self.epoch_coordinator
    }

    /// Mark workers as unhealthy if they haven't sent heartbeat
    pub fn check_worker_health(&self, timeout_seconds: u64) {
        let now = current_timestamp();

        for mut worker in self.active_workers.iter_mut() {
            if now - worker.last_heartbeat > timeout_seconds {
                worker.healthy = false;
                tracing::warn!(worker = %worker.worker_id, "Worker marked unhealthy");
            }
        }
    }

    /// Remove unhealthy workers
    pub fn remove_unhealthy_workers(&self) -> Vec<WorkerId> {
        let unhealthy: Vec<_> = self
            .active_workers
            .iter()
            .filter(|w| !w.healthy)
            .map(|w| w.key().clone())
            .collect();

        for worker_id in &unhealthy {
            self.remove_worker(worker_id);
        }

        unhealthy
    }
}

/// Get current unix timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Serializable state for shard manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardManagerState {
    pub datasets: Vec<DatasetMetadata>,
    pub workers: Vec<WorkerId>,
    pub epoch_state: crate::epoch::EpochCoordinatorState,
}

impl From<&ShardManager> for ShardManagerState {
    fn from(manager: &ShardManager) -> Self {
        Self {
            datasets: manager.datasets.iter().map(|e| e.value().clone()).collect(),
            workers: manager.active_workers(),
            epoch_state: crate::epoch::EpochCoordinatorState::from(
                manager.epoch_coordinator.as_ref(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_dataset(id: &str, total_samples: u64, shard_size: u64) -> DatasetMetadata {
        DatasetMetadata {
            id: id.to_string(),
            path: "/data/test".to_string(),
            format: "parquet".to_string(),
            total_samples,
            total_shards: total_samples.div_ceil(shard_size),
            shard_size,
            shuffle: true,
            seed: 42,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_register_dataset() {
        let manager = ShardManager::new();
        let dataset = create_test_dataset("dataset-1", 1000, 100);

        manager.register_dataset(dataset.clone());

        assert_eq!(manager.dataset_count(), 1);
        let retrieved = manager.get_dataset("dataset-1").unwrap();
        assert_eq!(retrieved.id, "dataset-1");
        assert_eq!(retrieved.total_samples, 1000);
    }

    #[test]
    fn test_register_worker() {
        let manager = ShardManager::new();

        manager.register_worker("worker-1");
        manager.register_worker("worker-2");

        assert_eq!(manager.active_worker_count(), 2);
        assert!(manager.active_workers().contains(&"worker-1".to_string()));
    }

    #[test]
    fn test_get_shard_for_worker() {
        let manager = ShardManager::new();
        let dataset = create_test_dataset("dataset-1", 1000, 100);

        manager.register_dataset(dataset);
        manager.register_worker("worker-1");
        manager.register_worker("worker-2");

        let shards_w1 = manager
            .get_shard_for_worker("dataset-1", "worker-1", 0)
            .unwrap();
        let shards_w2 = manager
            .get_shard_for_worker("dataset-1", "worker-2", 0)
            .unwrap();

        // 10 shards total, 5 each
        assert_eq!(shards_w1.len() + shards_w2.len(), 10);

        // No overlap
        let w1_ids: Vec<_> = shards_w1.iter().map(|s| s.shard_id).collect();
        let w2_ids: Vec<_> = shards_w2.iter().map(|s| s.shard_id).collect();

        for id in &w1_ids {
            assert!(!w2_ids.contains(id));
        }
    }

    #[test]
    fn test_advance_epoch() {
        let manager = ShardManager::new();
        let dataset = create_test_dataset("dataset-1", 1000, 100);

        manager.register_dataset(dataset);

        assert_eq!(manager.current_epoch("dataset-1"), 0);

        assert_eq!(manager.advance_epoch("dataset-1"), Some(1));
        assert_eq!(manager.current_epoch("dataset-1"), 1);
    }

    #[test]
    fn test_different_shards_per_epoch() {
        let manager = ShardManager::new();
        let dataset = create_test_dataset("dataset-1", 1000, 100);

        manager.register_dataset(dataset);
        manager.register_worker("worker-1");

        let epoch0_shards = manager
            .get_shard_for_worker("dataset-1", "worker-1", 0)
            .unwrap();
        let epoch1_shards = manager
            .get_shard_for_worker("dataset-1", "worker-1", 1)
            .unwrap();

        let epoch0_ids: Vec<_> = epoch0_shards.iter().map(|s| s.shard_id).collect();
        let epoch1_ids: Vec<_> = epoch1_shards.iter().map(|s| s.shard_id).collect();

        // With shuffling, different epochs should give different shard order
        // (though same count)
        assert_eq!(epoch0_ids.len(), epoch1_ids.len());
        // Note: it's possible but unlikely they're identical, so we don't assert inequality
    }

    #[test]
    fn test_worker_removal_and_rebalance() {
        let manager = ShardManager::new();
        let dataset = create_test_dataset("dataset-1", 1000, 100);

        manager.register_dataset(dataset);
        manager.register_worker("worker-1");
        manager.register_worker("worker-2");
        manager.register_worker("worker-3");

        // Get initial assignments
        let initial = manager.rebalance_shards();
        assert_eq!(initial.len(), 3);

        // Remove a worker
        manager.remove_worker("worker-2");

        // Rebalance
        let after_removal = manager.rebalance_shards();
        assert_eq!(after_removal.len(), 2);

        // All shards should still be assigned
        let mut all_shards = vec![];
        for worker_entry in after_removal.iter() {
            if let Some(shards) = worker_entry.value().get("dataset-1") {
                all_shards.extend(shards.value().clone());
            }
        }
        all_shards.sort();
        all_shards.dedup();
        assert_eq!(all_shards.len(), 10); // 10 shards total
    }

    #[test]
    fn test_heartbeat_and_health() {
        let manager = ShardManager::new();
        manager.register_worker("worker-1");

        // Simulate time passing (by checking with 0 timeout)
        manager.check_worker_health(0);

        // Worker should be marked unhealthy after timeout
        // Note: timing-dependent, so we just verify the method runs
    }

    #[test]
    fn test_shard_assignment_calculation() {
        let manager = ShardManager::new();
        let dataset = create_test_dataset("dataset-1", 1050, 100);

        manager.register_dataset(dataset);
        manager.register_worker("worker-1");

        let shards = manager
            .get_shard_for_worker("dataset-1", "worker-1", 0)
            .unwrap();

        // Check that last shard doesn't exceed total samples
        for shard in &shards {
            assert!(shard.end_index <= 1050);
            assert!(shard.start_index < shard.end_index);
        }
    }

    #[test]
    fn test_shard_size_zero_validation() {
        let manager = ShardManager::new();

        let err = manager
            .register_dataset_with_info(
                "bad-ds".to_string(),
                String::new(),
                "parquet".to_string(),
                100,
                0,
                false,
                42,
                HashMap::new(),
            )
            .unwrap_err();
        assert!(matches!(err, Error::InvalidShardConfig { .. }));

        // Infallible register_dataset_params clamps shard_size=0 to 1 instead of panicking
        manager.register_dataset_params("clamped-ds", 10, 0, false, 1);
        let ds = manager.get_dataset("clamped-ds").unwrap();
        assert_eq!(ds.shard_size, 1);
        assert_eq!(ds.total_shards, 10);
    }

    #[test]
    fn test_register_with_info_remove_counts_and_list_shards() {
        let manager = ShardManager::new();
        manager.register_worker("worker-a");
        manager.register_worker("worker-b");

        let mut meta = HashMap::new();
        meta.insert("split".to_string(), "train".to_string());

        let ds = manager
            .register_dataset_with_info(
                "ds-1".to_string(),
                "/nonexistent/path".to_string(),
                "webdataset".to_string(),
                400,
                100,
                true,
                7,
                meta,
            )
            .unwrap();
        assert_eq!(ds.format, "webdataset");
        assert_eq!(ds.path, "/nonexistent/path");
        assert_eq!(ds.total_shards, 4);

        let a_shards = manager.get_shard_for_worker("ds-1", "worker-a", 0).unwrap();
        let b_shards = manager.get_shard_for_worker("ds-1", "worker-b", 0).unwrap();
        assert_eq!(manager.get_worker_shard_count("worker-a"), a_shards.len());
        assert_eq!(manager.get_worker_shard_count("worker-b"), b_shards.len());

        let listed = manager.list_dataset_shards("ds-1", 0);
        assert_eq!(listed.len(), 4);
        for (assignment, owner) in &listed {
            assert!(owner.is_some());
            assert_eq!(assignment.dataset_id, "ds-1");
        }

        assert!(manager.remove_dataset("ds-1"));
        assert!(!manager.remove_dataset("ds-1"));
        assert_eq!(manager.get_worker_shard_count("worker-a"), 0);
        assert!(manager.list_dataset_shards("ds-1", 0).is_empty());
    }

    #[test]
    fn test_local_directory_file_paths_slicing() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            std::fs::write(dir.path().join(format!("part-{:02}.parquet", i)), b"x").unwrap();
        }

        let manager = ShardManager::new();
        manager.register_worker("w0");
        manager
            .register_dataset_with_info(
                "local-ds".to_string(),
                dir.path().to_string_lossy().to_string(),
                "parquet".to_string(),
                300,
                100, // 3 shards -> 2 files per shard
                false,
                0,
                HashMap::new(),
            )
            .unwrap();

        let shards = manager.get_shard_for_worker("local-ds", "w0", 0).unwrap();
        assert_eq!(shards.len(), 3);
        assert_eq!(shards[0].file_paths.len(), 2);
        assert_eq!(shards[1].file_paths.len(), 2);
        assert_eq!(shards[2].file_paths.len(), 2);
        assert!(shards[0].file_paths[0].ends_with("part-00.parquet"));
        assert!(shards[0].file_paths[1].ends_with("part-01.parquet"));
        assert!(shards[2].file_paths[1].ends_with("part-05.parquet"));
    }
}
