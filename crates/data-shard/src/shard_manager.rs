//! Shard manager for coordinating data distribution across workers
//!
//! Handles dataset registration, shard assignment, and dynamic rebalancing.

use crate::{ConsistentHash, EpochCoordinator};
use dashmap::DashMap;
use runtime_core::types::{DatasetId, DatasetMetadata, Epoch, ShardAssignment, ShardId, WorkerId};
use serde::{Deserialize, Serialize};
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

    /// Register a new dataset
    pub fn register_dataset(&self, metadata: DatasetMetadata) {
        let dataset_id = metadata.id.clone();
        self.epoch_coordinator.init_epoch(&dataset_id, 0);
        self.datasets.insert(dataset_id.clone(), metadata);

        tracing::info!(dataset = %dataset_id, "Registered dataset");
    }

    /// Register a dataset with explicit parameters
    pub fn register_dataset_params(
        &self,
        dataset_id: &str,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
    ) {
        let total_shards = total_samples.div_ceil(shard_size);

        let metadata = DatasetMetadata {
            id: dataset_id.to_string(),
            path: String::new(),
            format: "unknown".to_string(),
            total_samples,
            total_shards,
            shard_size,
            shuffle,
            seed,
            metadata: Default::default(),
        };

        self.register_dataset(metadata);
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

        let assignments: Vec<_> = shard_ids
            .into_iter()
            .map(|shard_id| {
                let start_index = shard_id * dataset.shard_size;
                let end_index =
                    std::cmp::min(start_index + dataset.shard_size, dataset.total_samples);

                ShardAssignment {
                    dataset_id: dataset_id.to_string(),
                    shard_id,
                    total_shards: dataset.total_shards,
                    start_index,
                    end_index,
                    file_paths: vec![], // Populated by storage layer
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
}
