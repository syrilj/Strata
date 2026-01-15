//! Epoch coordination for ML training
//!
//! Manages epoch progression and shard shuffling for better model generalization.

use dashmap::DashMap;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use runtime_core::types::{DatasetId, Epoch};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Epoch coordinator for managing epoch progression and shuffling
#[derive(Debug)]
pub struct EpochCoordinator {
    /// Current epoch for each dataset
    epochs: DashMap<DatasetId, Epoch>,

    /// Base seed for deterministic shuffling
    base_seed: u64,

    /// Shuffle cache: (dataset_id, epoch) -> shuffled shard indices
    shuffle_cache: DashMap<(DatasetId, Epoch), Arc<Vec<u64>>>,
}

impl Default for EpochCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl EpochCoordinator {
    /// Create a new epoch coordinator with random seed
    pub fn new() -> Self {
        Self::with_seed(rand::random())
    }

    /// Create a new epoch coordinator with specific seed for reproducibility
    pub fn with_seed(seed: u64) -> Self {
        Self {
            epochs: DashMap::new(),
            base_seed: seed,
            shuffle_cache: DashMap::new(),
        }
    }

    /// Get current epoch for a dataset
    pub fn current_epoch(&self, dataset_id: &str) -> Epoch {
        self.epochs.get(dataset_id).map(|e| *e).unwrap_or(0)
    }

    /// Initialize or reset epoch for a dataset
    pub fn init_epoch(&self, dataset_id: &str, epoch: Epoch) {
        self.epochs.insert(dataset_id.to_string(), epoch);
        tracing::info!(dataset = dataset_id, epoch = epoch, "Initialized epoch");
    }

    /// Advance to the next epoch for a dataset
    /// Returns the new epoch number
    pub fn advance_epoch(&self, dataset_id: &str) -> Epoch {
        let new_epoch = self
            .epochs
            .entry(dataset_id.to_string())
            .and_modify(|e| *e += 1)
            .or_insert(1);

        tracing::info!(dataset = dataset_id, epoch = *new_epoch, "Advanced epoch");
        *new_epoch
    }

    /// Get shuffled shard indices for a specific epoch
    /// Uses deterministic shuffling based on epoch and seed
    pub fn get_shuffled_shards(
        &self,
        dataset_id: &str,
        epoch: Epoch,
        total_shards: u64,
    ) -> Arc<Vec<u64>> {
        let key = (dataset_id.to_string(), epoch);

        // Check cache first
        if let Some(cached) = self.shuffle_cache.get(&key) {
            return cached.clone();
        }

        // Generate shuffled order
        let mut shards: Vec<u64> = (0..total_shards).collect();

        // Combine base seed, dataset ID, and epoch for unique but reproducible shuffling
        let epoch_seed = self.compute_epoch_seed(dataset_id, epoch);
        let mut rng = ChaCha8Rng::seed_from_u64(epoch_seed);
        shards.shuffle(&mut rng);

        let result = Arc::new(shards);
        self.shuffle_cache.insert(key, result.clone());

        tracing::debug!(
            dataset = dataset_id,
            epoch = epoch,
            total_shards = total_shards,
            "Generated shuffled shard order"
        );

        result
    }

    /// Get the shard assignment for a specific worker in an epoch
    /// Returns a subset of shards for the worker to process
    pub fn get_worker_shards(
        &self,
        dataset_id: &str,
        epoch: Epoch,
        total_shards: u64,
        worker_rank: u32,
        total_workers: u32,
    ) -> Vec<u64> {
        if total_workers == 0 {
            return vec![];
        }

        let shuffled = self.get_shuffled_shards(dataset_id, epoch, total_shards);

        // Distribute shards round-robin across workers
        shuffled
            .iter()
            .enumerate()
            .filter_map(|(idx, &shard)| {
                if (idx as u32) % total_workers == worker_rank {
                    Some(shard)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Clear shuffle cache for a dataset (useful when dataset is modified)
    pub fn clear_cache(&self, dataset_id: &str) {
        self.shuffle_cache.retain(|(id, _), _| id != dataset_id);
        tracing::debug!(dataset = dataset_id, "Cleared shuffle cache");
    }

    /// Clear all caches
    pub fn clear_all_caches(&self) {
        self.shuffle_cache.clear();
    }

    /// Compute epoch-specific seed deterministically
    fn compute_epoch_seed(&self, dataset_id: &str, epoch: Epoch) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.base_seed.hash(&mut hasher);
        dataset_id.hash(&mut hasher);
        epoch.hash(&mut hasher);
        hasher.finish()
    }

    /// Get the base seed for reproducibility
    pub fn base_seed(&self) -> u64 {
        self.base_seed
    }

    /// Get all tracked datasets and their epochs
    pub fn all_epochs(&self) -> Vec<(DatasetId, Epoch)> {
        self.epochs
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }
}

/// Serializable state for epoch coordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochCoordinatorState {
    /// Current epochs per dataset
    pub epochs: Vec<(DatasetId, Epoch)>,

    /// Base seed for shuffling
    pub base_seed: u64,
}

impl From<&EpochCoordinator> for EpochCoordinatorState {
    fn from(coord: &EpochCoordinator) -> Self {
        Self {
            epochs: coord.all_epochs(),
            base_seed: coord.base_seed(),
        }
    }
}

impl From<EpochCoordinatorState> for EpochCoordinator {
    fn from(state: EpochCoordinatorState) -> Self {
        let coord = EpochCoordinator::with_seed(state.base_seed);
        for (dataset_id, epoch) in state.epochs {
            coord.init_epoch(&dataset_id, epoch);
        }
        coord
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_progression() {
        let coord = EpochCoordinator::new();

        assert_eq!(coord.current_epoch("dataset-1"), 0);

        coord.init_epoch("dataset-1", 0);
        assert_eq!(coord.current_epoch("dataset-1"), 0);

        assert_eq!(coord.advance_epoch("dataset-1"), 1);
        assert_eq!(coord.current_epoch("dataset-1"), 1);

        assert_eq!(coord.advance_epoch("dataset-1"), 2);
        assert_eq!(coord.current_epoch("dataset-1"), 2);
    }

    #[test]
    fn test_deterministic_shuffling() {
        let seed = 42;
        let coord1 = EpochCoordinator::with_seed(seed);
        let coord2 = EpochCoordinator::with_seed(seed);

        let shards1 = coord1.get_shuffled_shards("dataset-1", 0, 100);
        let shards2 = coord2.get_shuffled_shards("dataset-1", 0, 100);

        assert_eq!(*shards1, *shards2);
    }

    #[test]
    fn test_different_epochs_different_shuffle() {
        let coord = EpochCoordinator::with_seed(42);

        let epoch0 = coord.get_shuffled_shards("dataset-1", 0, 100);
        let epoch1 = coord.get_shuffled_shards("dataset-1", 1, 100);

        assert_ne!(*epoch0, *epoch1);
    }

    #[test]
    fn test_different_datasets_different_shuffle() {
        let coord = EpochCoordinator::with_seed(42);

        let ds1 = coord.get_shuffled_shards("dataset-1", 0, 100);
        let ds2 = coord.get_shuffled_shards("dataset-2", 0, 100);

        assert_ne!(*ds1, *ds2);
    }

    #[test]
    fn test_worker_shard_distribution() {
        let coord = EpochCoordinator::with_seed(42);

        let w0_shards = coord.get_worker_shards("dataset-1", 0, 100, 0, 4);
        let w1_shards = coord.get_worker_shards("dataset-1", 0, 100, 1, 4);
        let w2_shards = coord.get_worker_shards("dataset-1", 0, 100, 2, 4);
        let w3_shards = coord.get_worker_shards("dataset-1", 0, 100, 3, 4);

        // Each worker should get 25 shards
        assert_eq!(w0_shards.len(), 25);
        assert_eq!(w1_shards.len(), 25);
        assert_eq!(w2_shards.len(), 25);
        assert_eq!(w3_shards.len(), 25);

        // No overlap between workers
        let mut all_shards: Vec<u64> = vec![];
        all_shards.extend(w0_shards.iter().copied());
        all_shards.extend(&w1_shards);
        all_shards.extend(&w2_shards);
        all_shards.extend(&w3_shards);

        all_shards.sort();
        all_shards.dedup();
        assert_eq!(all_shards.len(), 100);
    }

    #[test]
    fn test_shuffle_cache() {
        let coord = EpochCoordinator::with_seed(42);

        // First call computes
        let first = coord.get_shuffled_shards("dataset-1", 0, 100);
        // Second call should use cache (same Arc)
        let second = coord.get_shuffled_shards("dataset-1", 0, 100);

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn test_state_serialization() {
        let coord = EpochCoordinator::with_seed(42);
        coord.init_epoch("dataset-1", 5);
        coord.init_epoch("dataset-2", 10);

        let state = EpochCoordinatorState::from(&coord);
        let json = serde_json::to_string(&state).unwrap();
        let restored_state: EpochCoordinatorState = serde_json::from_str(&json).unwrap();
        let restored = EpochCoordinator::from(restored_state);

        assert_eq!(restored.base_seed(), 42);
        assert_eq!(restored.current_epoch("dataset-1"), 5);
        assert_eq!(restored.current_epoch("dataset-2"), 10);
    }

    #[test]
    fn test_clear_cache() {
        let coord = EpochCoordinator::with_seed(42);

        coord.get_shuffled_shards("dataset-1", 0, 100);
        coord.get_shuffled_shards("dataset-1", 1, 100);
        coord.get_shuffled_shards("dataset-2", 0, 100);

        coord.clear_cache("dataset-1");

        // dataset-2 cache should still exist
        assert!(coord
            .shuffle_cache
            .contains_key(&("dataset-2".to_string(), 0)));
    }
}
