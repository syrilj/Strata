//! Dataset registry Python bindings
//!
//! Exposes `ShardManager` functionality for dataset registration and shard assignment.

use data_shard::ShardManager;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Information about a shard assignment
#[pyclass]
#[derive(Clone)]
pub struct ShardInfo {
    /// Dataset identifier
    #[pyo3(get)]
    pub dataset_id: String,

    /// Shard identifier within the dataset
    #[pyo3(get)]
    pub shard_id: u64,

    /// Total number of shards in the dataset
    #[pyo3(get)]
    pub total_shards: u64,

    /// Start sample index (inclusive)
    #[pyo3(get)]
    pub start_index: u64,

    /// End sample index (exclusive)
    #[pyo3(get)]
    pub end_index: u64,

    /// Current training epoch
    #[pyo3(get)]
    pub epoch: u64,

    /// File paths for this shard (if available)
    #[pyo3(get)]
    pub file_paths: Vec<String>,
}

#[pymethods]
impl ShardInfo {
    fn __repr__(&self) -> String {
        format!(
            "ShardInfo(dataset_id='{}', shard_id={}, range=[{}, {}), epoch={})",
            self.dataset_id, self.shard_id, self.start_index, self.end_index, self.epoch
        )
    }

    /// Number of samples in this shard
    #[getter]
    fn num_samples(&self) -> u64 {
        self.end_index - self.start_index
    }
}

/// Dataset registry for managing distributed data sharding
///
/// Example:
///     registry = DatasetRegistry()
///     registry.register_worker("worker-0")
///     registry.register_dataset("imagenet", total_samples=1281167, shard_size=10000)
///     shards = registry.get_shards("imagenet", "worker-0", epoch=0)
#[pyclass]
pub struct DatasetRegistry {
    manager: Arc<ShardManager>,
}

#[pymethods]
impl DatasetRegistry {
    /// Create a new dataset registry
    ///
    /// Args:
    ///     coordinator_url: Optional URL of the coordinator server (for distributed mode)
    #[new]
    #[pyo3(signature = (coordinator_url=None))]
    fn new(coordinator_url: Option<&str>) -> PyResult<Self> {
        // For now, we use a local shard manager
        // Future: connect to coordinator via gRPC if coordinator_url is provided
        let _ = coordinator_url; // Reserved for future use

        Ok(Self {
            manager: Arc::new(ShardManager::new()),
        })
    }

    /// Register a worker with the registry
    ///
    /// Args:
    ///     worker_id: Unique identifier for the worker
    fn register_worker(&self, worker_id: &str) {
        self.manager.register_worker(worker_id);
    }

    /// Remove a worker from the registry
    ///
    /// Args:
    ///     worker_id: Identifier of the worker to remove
    fn remove_worker(&self, worker_id: &str) {
        self.manager.remove_worker(worker_id);
    }

    /// Register a dataset for sharding
    ///
    /// Args:
    ///     dataset_id: Unique identifier for the dataset
    ///     total_samples: Total number of samples in the dataset
    ///     shard_size: Number of samples per shard
    ///     shuffle: Whether to shuffle shards each epoch (default: True)
    ///     seed: Random seed for shuffling (default: 42)
    #[pyo3(signature = (dataset_id, total_samples, shard_size, shuffle=true, seed=42))]
    fn register_dataset(
        &self,
        dataset_id: &str,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
    ) {
        self.manager
            .register_dataset_params(dataset_id, total_samples, shard_size, shuffle, seed);
    }

    /// Get shard assignments for a worker in a given epoch
    ///
    /// Args:
    ///     dataset_id: Dataset identifier
    ///     worker_id: Worker identifier  
    ///     epoch: Training epoch number
    ///
    /// Returns:
    ///     List of ShardInfo objects assigned to this worker
    fn get_shards(&self, dataset_id: &str, worker_id: &str, epoch: u64) -> PyResult<Vec<ShardInfo>> {
        let assignments = self
            .manager
            .get_shard_for_worker(dataset_id, worker_id, epoch)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Failed to get shards for worker '{}' on dataset '{}'",
                    worker_id, dataset_id
                ))
            })?;

        Ok(assignments
            .into_iter()
            .map(|a| ShardInfo {
                dataset_id: a.dataset_id,
                shard_id: a.shard_id,
                total_shards: a.total_shards,
                start_index: a.start_index,
                end_index: a.end_index,
                epoch: a.epoch,
                file_paths: a.file_paths,
            })
            .collect())
    }

    /// Advance to the next epoch for a dataset
    ///
    /// Args:
    ///     dataset_id: Dataset identifier
    ///
    /// Returns:
    ///     The new epoch number
    fn advance_epoch(&self, dataset_id: &str) -> PyResult<u64> {
        self.manager.advance_epoch(dataset_id).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Dataset '{}' not found",
                dataset_id
            ))
        })
    }

    /// Get the current epoch for a dataset
    ///
    /// Args:
    ///     dataset_id: Dataset identifier
    ///
    /// Returns:
    ///     Current epoch number
    fn current_epoch(&self, dataset_id: &str) -> u64 {
        self.manager.current_epoch(dataset_id)
    }

    /// Get the number of active workers
    #[getter]
    fn worker_count(&self) -> usize {
        self.manager.active_worker_count()
    }

    /// Get the number of registered datasets
    #[getter]
    fn dataset_count(&self) -> usize {
        self.manager.dataset_count()
    }

    /// Get list of active worker IDs
    fn active_workers(&self) -> Vec<String> {
        self.manager.active_workers()
    }

    /// Get list of registered dataset IDs
    fn datasets(&self) -> Vec<String> {
        self.manager.datasets()
    }

    fn __repr__(&self) -> String {
        format!(
            "DatasetRegistry(workers={}, datasets={})",
            self.manager.active_worker_count(),
            self.manager.dataset_count()
        )
    }
}
