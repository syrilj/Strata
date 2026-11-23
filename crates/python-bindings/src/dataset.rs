//! Dataset registry Python bindings
//!
//! Exposes `ShardManager` functionality for local dataset registration and shard assignment,
//! as well as gRPC-backed coordinator dataset registration and sharding when `coordinator_url`
//! is provided.

use coordinator::proto::coordinator_client::CoordinatorClient;
use data_shard::ShardManager;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tonic::transport::Channel;

use crate::orchestrator::normalize_coordinator_url;

type Client = CoordinatorClient<Channel>;

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
/// Supports both local mode (`coordinator_url=None`, backed by `ShardManager`)
/// and coordinator-backed mode (`coordinator_url="http://..."`, backed by gRPC).
#[pyclass]
pub struct DatasetRegistry {
    manager: Arc<ShardManager>,
    coordinator_url: Option<String>,
    client: Arc<RwLock<Option<Client>>>,
    runtime: Arc<Runtime>,
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
        let runtime = Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create async runtime: {}",
                e
            ))
        })?;

        Ok(Self {
            manager: Arc::new(ShardManager::new()),
            coordinator_url: coordinator_url.map(normalize_coordinator_url),
            client: Arc::new(RwLock::new(None)),
            runtime: Arc::new(runtime),
        })
    }

    #[getter]
    fn coordinator_url(&self) -> Option<String> {
        self.coordinator_url.clone()
    }

    /// Register a worker with the registry (local and coordinator if configured)
    fn register_worker(&self, py: Python<'_>, worker_id: &str) -> PyResult<()> {
        self.manager.register_worker(worker_id);
        if let Some(url) = &self.coordinator_url {
            let mut grpc_client = self.get_client_sync(py, url)?;
            let wid = worker_id.to_string();
            py.detach(|| {
                self.runtime.block_on(async move {
                    let _ = grpc_client
                        .register_worker(coordinator::proto::WorkerInfo {
                            worker_id: wid,
                            hostname: "127.0.0.1".to_string(),
                            port: 50052,
                            gpu_count: 0,
                            memory_bytes: 0,
                            metadata: HashMap::new(),
                        })
                        .await;
                })
            });
        }
        Ok(())
    }

    /// Remove a worker from the registry (local and coordinator if configured)
    fn remove_worker(&self, py: Python<'_>, worker_id: &str) -> PyResult<()> {
        self.manager.remove_worker(worker_id);
        if let Some(url) = &self.coordinator_url {
            let mut grpc_client = self.get_client_sync(py, url)?;
            let wid = worker_id.to_string();
            py.detach(|| {
                self.runtime.block_on(async move {
                    let _ = grpc_client
                        .deregister_worker(coordinator::proto::WorkerInfo {
                            worker_id: wid,
                            hostname: String::new(),
                            port: 0,
                            gpu_count: 0,
                            memory_bytes: 0,
                            metadata: HashMap::new(),
                        })
                        .await;
                })
            });
        }
        Ok(())
    }

    /// Register a dataset for sharding (local or coordinator-backed)
    #[pyo3(signature = (dataset_id, total_samples, shard_size, shuffle=true, seed=42, path="", format="parquet"))]
    #[allow(clippy::too_many_arguments)]
    fn register_dataset(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
        path: &str,
        format: &str,
    ) -> PyResult<()> {
        if shard_size == 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "shard_size must be greater than 0",
            ));
        }

        // Always update local shard manager for introspection/fallback
        let _ = self.manager.register_dataset_with_info(
            dataset_id.to_string(),
            path.to_string(),
            format.to_string(),
            total_samples,
            shard_size,
            shuffle,
            seed,
            HashMap::new(),
        );

        if let Some(url) = &self.coordinator_url {
            let mut grpc_client = self.get_client_sync(py, url)?;
            let request = coordinator::proto::DatasetInfo {
                dataset_id: dataset_id.to_string(),
                path: path.to_string(),
                format: format.to_string(),
                total_samples: total_samples as i64,
                shard_size: shard_size as i64,
                shuffle,
                seed: seed as i64,
                metadata: HashMap::new(),
            };

            py.detach(|| {
                self.runtime.block_on(async move {
                    grpc_client.register_dataset(request).await.map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to register dataset with coordinator: {}",
                            e
                        ))
                    })?;
                    Ok::<(), PyErr>(())
                })
            })?;
        }

        Ok(())
    }

    /// Convenience alias for registering a dataset with path and format first
    #[pyo3(signature = (dataset_id, path, format="parquet", total_samples=10000, shard_size=1000, shuffle=true, seed=42))]
    #[allow(clippy::too_many_arguments)]
    fn register(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        path: &str,
        format: &str,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
    ) -> PyResult<()> {
        self.register_dataset(
            py,
            dataset_id,
            total_samples,
            shard_size,
            shuffle,
            seed,
            path,
            format,
        )
    }

    /// Get all shard assignments for a worker in a given epoch (local mode, or coordinator fallback)
    #[pyo3(signature = (dataset_id, worker_id, epoch=0))]
    fn get_shards(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        worker_id: &str,
        epoch: u64,
    ) -> PyResult<Vec<ShardInfo>> {
        if let Some(url) = &self.coordinator_url {
            if let Ok(mut grpc_client) = self.get_client_sync(py, url) {
                let did = dataset_id.to_string();
                let wid = worker_id.to_string();
                let res: Result<ShardInfo, PyErr> = py.detach(|| {
                    self.runtime.block_on(async move {
                        let response = grpc_client
                            .get_data_shard(coordinator::proto::ShardRequest {
                                worker_id: wid,
                                dataset_id: did,
                                epoch: epoch as i64,
                            })
                            .await
                            .map_err(|e| {
                                pyo3::exceptions::PyRuntimeError::new_err(format!(
                                    "Failed to get shard from coordinator: {}",
                                    e
                                ))
                            })?;
                        let s = response.into_inner();
                        Ok(ShardInfo {
                            dataset_id: s.dataset_id,
                            shard_id: s.shard_id as u64,
                            total_shards: s.total_shards as u64,
                            start_index: s.start_index as u64,
                            end_index: s.end_index as u64,
                            epoch: s.epoch as u64,
                            file_paths: s.file_paths,
                        })
                    })
                });
                if let Ok(shard) = res {
                    return Ok(vec![shard]);
                }
            }
        }

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

    /// Convenience method returning a single ShardInfo for a worker or rank
    #[pyo3(signature = (dataset_id, worker_id=None, worker_rank=None, epoch=0))]
    fn get_shard(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        worker_id: Option<&str>,
        worker_rank: Option<u64>,
        epoch: u64,
    ) -> PyResult<ShardInfo> {
        let resolved_worker = match worker_id {
            Some(wid) if !wid.is_empty() => wid.to_string(),
            _ => match worker_rank {
                Some(rank) => format!("worker-{}", rank),
                None => "worker-0".to_string(),
            },
        };

        if let Some(url) = &self.coordinator_url {
            let mut grpc_client = self.get_client_sync(py, url)?;
            let did = dataset_id.to_string();
            let wid = resolved_worker.clone();

            return py.detach(|| {
                self.runtime.block_on(async move {
                    // Try get_data_shard first; if coordinator has no registered workers yet,
                    // auto-register the worker id so standalone DatasetRegistry usage succeeds.
                    let first_attempt = grpc_client
                        .get_data_shard(coordinator::proto::ShardRequest {
                            worker_id: wid.clone(),
                            dataset_id: did.clone(),
                            epoch: epoch as i64,
                        })
                        .await;

                    let response = match first_attempt {
                        Ok(resp) => resp,
                        Err(_) => {
                            let _ = grpc_client
                                .register_worker(coordinator::proto::WorkerInfo {
                                    worker_id: wid.clone(),
                                    hostname: "127.0.0.1".to_string(),
                                    port: 50052,
                                    gpu_count: 0,
                                    memory_bytes: 0,
                                    metadata: HashMap::new(),
                                })
                                .await;

                            grpc_client
                                .get_data_shard(coordinator::proto::ShardRequest {
                                    worker_id: wid,
                                    dataset_id: did,
                                    epoch: epoch as i64,
                                })
                                .await
                                .map_err(|e| {
                                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                                        "Failed to get shard from coordinator: {}",
                                        e
                                    ))
                                })?
                        }
                    };

                    let s = response.into_inner();
                    Ok(ShardInfo {
                        dataset_id: s.dataset_id,
                        shard_id: s.shard_id as u64,
                        total_shards: s.total_shards as u64,
                        start_index: s.start_index as u64,
                        end_index: s.end_index as u64,
                        epoch: s.epoch as u64,
                        file_paths: s.file_paths,
                    })
                })
            });
        }

        if self.manager.active_worker_count() == 0 {
            self.manager.register_worker(&resolved_worker);
        }
        let shards = self.get_shards(py, dataset_id, &resolved_worker, epoch)?;
        shards.into_iter().next().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "No shards available for worker '{}' on dataset '{}'",
                resolved_worker, dataset_id
            ))
        })
    }

    /// Advance to the next epoch for a dataset
    fn advance_epoch(&self, dataset_id: &str) -> PyResult<u64> {
        self.manager.advance_epoch(dataset_id).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("Dataset '{}' not found", dataset_id))
        })
    }

    /// Get the current epoch for a dataset
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
            "DatasetRegistry(workers={}, datasets={}, coordinator_url={:?})",
            self.manager.active_worker_count(),
            self.manager.dataset_count(),
            self.coordinator_url
        )
    }
}

impl DatasetRegistry {
    fn get_client_sync(&self, py: Python<'_>, url: &str) -> PyResult<Client> {
        let client_lock = self.client.clone();
        let target_url = url.to_string();

        py.detach(|| {
            self.runtime.block_on(async move {
                {
                    let guard = client_lock.read().await;
                    if let Some(client) = guard.as_ref() {
                        return Ok(client.clone());
                    }
                }

                let channel = Channel::from_shared(target_url)
                    .map_err(|e| {
                        pyo3::exceptions::PyValueError::new_err(format!(
                            "Invalid coordinator URL: {}",
                            e
                        ))
                    })?
                    .connect()
                    .await
                    .map_err(|e| {
                        pyo3::exceptions::PyConnectionError::new_err(format!(
                            "Failed to connect to coordinator: {}",
                            e
                        ))
                    })?;

                let new_client = CoordinatorClient::new(channel);
                let mut guard = client_lock.write().await;
                if let Some(existing) = guard.as_ref() {
                    return Ok(existing.clone());
                }
                *guard = Some(new_client.clone());
                Ok(new_client)
            })
        })
    }
}
