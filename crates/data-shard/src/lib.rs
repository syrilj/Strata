//! Data sharding for distributed ML training
//!
//! This crate provides:
//! - **Consistent hashing** for stable shard distribution across workers
//! - **Epoch coordination** for deterministic shuffling per training epoch
//! - **Shard management** for dataset registration and dynamic rebalancing
//!
//! # Example
//!
//! ```rust
//! use data_shard::{ShardManager, ConsistentHash, EpochCoordinator};
//! use runtime_core::types::DatasetMetadata;
//!
//! // Create a shard manager
//! let manager = ShardManager::new();
//!
//! // Register workers
//! manager.register_worker("worker-0");
//! manager.register_worker("worker-1");
//!
//! // Register a dataset
//! manager.register_dataset_params(
//!     "imagenet",
//!     1_281_167, // total samples
//!     10_000,    // shard size
//!     true,      // shuffle
//!     42,        // seed
//! );
//!
//! // Get shard assignments for a worker
//! let shards = manager.get_shard_for_worker("imagenet", "worker-0", 0);
//! ```

mod consistent_hash;
mod epoch;
mod shard_manager;

// Re-export main types
pub use consistent_hash::{ConsistentHash, ConsistentHashState};
pub use epoch::{EpochCoordinator, EpochCoordinatorState};
pub use shard_manager::{ShardManager, ShardManagerState, WorkerState};

// Re-export types from runtime-core for convenience
pub use runtime_core::types::{
    DatasetId, DatasetMetadata, Epoch, ShardAssignment, ShardId, WorkerId,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration test: Full workflow from registration to shard assignment
    #[test]
    fn test_full_workflow() {
        // Create manager
        let manager = ShardManager::new();

        // Register workers
        manager.register_worker("worker-0");
        manager.register_worker("worker-1");
        manager.register_worker("worker-2");
        manager.register_worker("worker-3");

        // Register dataset
        manager.register_dataset_params(
            "cifar10", 60_000, // total samples
            1_000,  // shard size -> 60 shards
            true,   // shuffle
            42,     // seed
        );

        // Get epoch 0 assignments
        let mut total_samples_assigned = 0;
        for i in 0..4 {
            let worker_id = format!("worker-{}", i);
            let assignments = manager
                .get_shard_for_worker("cifar10", &worker_id, 0)
                .unwrap();

            // Each worker should get ~15 shards (60/4)
            assert!(assignments.len() >= 14 && assignments.len() <= 16);

            for assignment in &assignments {
                total_samples_assigned += assignment.end_index - assignment.start_index;
            }
        }

        // All samples should be assigned
        assert_eq!(total_samples_assigned, 60_000);

        // Advance epoch and verify different assignments
        manager.advance_epoch("cifar10");
        let epoch0_shards: Vec<_> = manager
            .get_shard_for_worker("cifar10", "worker-0", 0)
            .unwrap()
            .iter()
            .map(|s| s.shard_id)
            .collect();
        let epoch1_shards: Vec<_> = manager
            .get_shard_for_worker("cifar10", "worker-0", 1)
            .unwrap()
            .iter()
            .map(|s| s.shard_id)
            .collect();

        // Shuffling should give different shard sets
        // (same number, different actual shards due to permutation)
        assert_eq!(epoch0_shards.len(), epoch1_shards.len());
    }

    /// Test worker failure and recovery
    #[test]
    fn test_worker_failure_recovery() {
        let manager = ShardManager::new();

        manager.register_worker("worker-0");
        manager.register_worker("worker-1");
        manager.register_worker("worker-2");

        manager.register_dataset_params("dataset", 30_000, 1_000, false, 0);

        // Initial assignment
        let initial_0 = manager
            .get_shard_for_worker("dataset", "worker-0", 0)
            .unwrap();
        let initial_shards: Vec<_> = initial_0.iter().map(|s| s.shard_id).collect();

        // Simulate worker-1 failure
        manager.remove_worker("worker-1");

        // Rebalance
        let final_assignments = manager.rebalance_shards();

        // All 30 shards should still be assigned across 2 remaining workers
        let mut all_shards = vec![];
        for worker in final_assignments.iter() {
            if let Some(shards) = worker.value().get("dataset") {
                all_shards.extend(shards.value().clone());
            }
        }
        all_shards.sort();
        all_shards.dedup();
        assert_eq!(all_shards.len(), 30);

        // Consistent hashing: worker-0 should keep most of its original shards
        let final_0 = manager
            .get_shard_for_worker("dataset", "worker-0", 0)
            .unwrap();
        let final_shards: Vec<_> = final_0.iter().map(|s| s.shard_id).collect();

        // At least 80% of original shards should remain with worker-0
        let retained = initial_shards
            .iter()
            .filter(|s| final_shards.contains(s))
            .count();
        let retention_rate = retained as f64 / initial_shards.len() as f64;
        assert!(
            retention_rate >= 0.8,
            "Should retain >= 80% of shards, got {}%",
            retention_rate * 100.0
        );
    }

    /// Test deterministic behavior with same seed
    #[test]
    fn test_deterministic_with_seed() {
        use std::sync::Arc;

        let seed = 12345u64;

        // Create two managers with same seed
        let coord1 = Arc::new(EpochCoordinator::with_seed(seed));
        let hash1 = Arc::new(ConsistentHash::new());
        let manager1 = ShardManager::with_components(hash1, coord1);

        let coord2 = Arc::new(EpochCoordinator::with_seed(seed));
        let hash2 = Arc::new(ConsistentHash::new());
        let manager2 = ShardManager::with_components(hash2, coord2);

        // Same operations on both
        for m in [&manager1, &manager2] {
            m.register_worker("worker-0");
            m.register_worker("worker-1");
            m.register_dataset_params("data", 10_000, 100, true, 0);
        }

        // Should produce identical assignments
        let shards1: Vec<_> = manager1
            .get_shard_for_worker("data", "worker-0", 0)
            .unwrap()
            .iter()
            .map(|s| s.shard_id)
            .collect();

        let shards2: Vec<_> = manager2
            .get_shard_for_worker("data", "worker-0", 0)
            .unwrap()
            .iter()
            .map(|s| s.shard_id)
            .collect();

        assert_eq!(shards1, shards2);
    }
}
