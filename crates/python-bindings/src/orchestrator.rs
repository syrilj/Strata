//! Training orchestrator Python bindings
//!
//! High-level orchestration wrapper connecting to coordinator gRPC server.

use coordinator::proto::coordinator_client::CoordinatorClient;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use tonic::transport::Channel;

/// Worker configuration returned after registration
#[pyclass]
#[derive(Clone)]
pub struct WorkerConfig {
    /// Assigned worker ID
    #[pyo3(get)]
    pub worker_id: String,

    /// Assigned rank in the worker group
    #[pyo3(get)]
    pub rank: i32,

    /// Total number of workers (world size)
    #[pyo3(get)]
    pub world_size: i32,

    /// Recommended heartbeat interval in milliseconds
    #[pyo3(get)]
    pub heartbeat_interval_ms: i64,
}

#[pymethods]
impl WorkerConfig {
    fn __repr__(&self) -> String {
        format!(
            "WorkerConfig(worker_id='{}', rank={}, world_size={})",
            self.worker_id, self.rank, self.world_size
        )
    }
}

/// Shard assignment from coordinator
#[pyclass]
#[derive(Clone)]
pub struct CoordinatorShardInfo {
    /// Dataset identifier
    #[pyo3(get)]
    pub dataset_id: String,

    /// Shard identifier
    #[pyo3(get)]
    pub shard_id: i64,

    /// Total shards in dataset
    #[pyo3(get)]
    pub total_shards: i64,

    /// Start sample index
    #[pyo3(get)]
    pub start_index: i64,

    /// End sample index
    #[pyo3(get)]
    pub end_index: i64,

    /// File paths for this shard
    #[pyo3(get)]
    pub file_paths: Vec<String>,

    /// Training epoch
    #[pyo3(get)]
    pub epoch: i64,
}

#[pymethods]
impl CoordinatorShardInfo {
    fn __repr__(&self) -> String {
        format!(
            "CoordinatorShardInfo(dataset='{}', shard={}, range=[{}, {}))",
            self.dataset_id, self.shard_id, self.start_index, self.end_index
        )
    }
}

/// Barrier synchronization result
#[pyclass]
#[derive(Clone)]
pub struct BarrierResult {
    /// Whether the barrier was released
    #[pyo3(get)]
    pub released: bool,

    /// Total participants that arrived
    #[pyo3(get)]
    pub participants: i64,

    /// This worker's arrival order
    #[pyo3(get)]
    pub arrival_order: i64,
}

#[pymethods]
impl BarrierResult {
    fn __repr__(&self) -> String {
        format!(
            "BarrierResult(released={}, participants={}, arrival_order={})",
            self.released, self.participants, self.arrival_order
        )
    }
}

// Type alias for the gRPC client
type Client = CoordinatorClient<Channel>;

/// High-level training orchestrator for distributed training coordination
///
/// Connects to a coordinator gRPC server to manage worker registration,
/// heartbeats, data sharding, and synchronization barriers.
///
/// Example:
///     orch = TrainingOrchestrator("http://localhost:50051")
///     config = orch.register_worker("worker-0", "localhost", 50052, gpu_count=8)
///     print(f"Registered as rank {config.rank} of {config.world_size}")
///     
///     # Get data shard for this worker
///     shard = orch.get_shard("imagenet", epoch=0)
///     
///     # Synchronize with other workers
///     orch.barrier("epoch-0", step=100)
#[pyclass]
pub struct TrainingOrchestrator {
    client: Arc<Mutex<Option<Client>>>,
    coordinator_url: String,
    runtime: Arc<Runtime>,
    worker_id: Arc<Mutex<Option<String>>>,
}

#[pymethods]
impl TrainingOrchestrator {
    /// Create a new training orchestrator
    ///
    /// Args:
    ///     coordinator_url: URL of the coordinator gRPC server (e.g., "http://localhost:50051")
    #[new]
    fn new(coordinator_url: &str) -> PyResult<Self> {
        let runtime = Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create async runtime: {}",
                e
            ))
        })?;

        Ok(Self {
            client: Arc::new(Mutex::new(None)),
            coordinator_url: coordinator_url.to_string(),
            runtime: Arc::new(runtime),
            worker_id: Arc::new(Mutex::new(None)),
        })
    }

    /// Connect to the coordinator server
    ///
    /// This is called automatically by other methods if not already connected.
    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let url = self.coordinator_url.clone();
        let client_lock = self.client.clone();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let channel = Channel::from_shared(url)
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

                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                *guard = Some(CoordinatorClient::new(channel));
                Ok(())
            })
        })
    }

    /// Register this worker with the coordinator
    ///
    /// Args:
    ///     worker_id: Unique identifier for this worker
    ///     hostname: Hostname or IP address
    ///     port: Port number for worker-to-worker communication
    ///     gpu_count: Number of GPUs available (default: 0)
    ///     memory_bytes: Available memory in bytes (default: 0)
    ///     metadata: Optional metadata dictionary
    ///
    /// Returns:
    ///     WorkerConfig with assigned rank and world size
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (worker_id, hostname, port, gpu_count=0, memory_bytes=0, metadata=None))]
    fn register_worker(
        &self,
        py: Python<'_>,
        worker_id: &str,
        hostname: &str,
        port: i32,
        gpu_count: i32,
        memory_bytes: i64,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<WorkerConfig> {
        // Ensure connected
        self.ensure_connected(py)?;

        let client_lock = self.client.clone();
        let worker_id_store = self.worker_id.clone();
        let wid = worker_id.to_string();
        let host = hostname.to_string();
        let meta = metadata.unwrap_or_default();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                let grpc_client = guard.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Not connected to coordinator")
                })?;

                let request = coordinator::proto::WorkerInfo {
                    worker_id: wid.clone(),
                    hostname: host,
                    port,
                    gpu_count,
                    memory_bytes,
                    metadata: meta,
                };

                let response = grpc_client.register_worker(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "Failed to register worker: {}",
                        e
                    ))
                })?;

                let config = response.into_inner();

                // Store worker ID for future calls
                *worker_id_store.lock().await = Some(wid);

                Ok(WorkerConfig {
                    worker_id: config.assigned_id,
                    rank: config.rank,
                    world_size: config.world_size,
                    heartbeat_interval_ms: config.heartbeat_interval_ms,
                })
            })
        })
    }

    /// Send a heartbeat to the coordinator
    ///
    /// Args:
    ///     current_step: Current training step (default: 0)
    ///     current_epoch: Current training epoch (default: 0)
    ///
    /// Returns:
    ///     True if acknowledged
    #[pyo3(signature = (current_step=0, current_epoch=0))]
    fn heartbeat(&self, py: Python<'_>, current_step: i64, current_epoch: i64) -> PyResult<bool> {
        self.ensure_connected(py)?;

        let client_lock = self.client.clone();
        let worker_id = self.get_worker_id(py)?;

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                let grpc_client = guard.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Not connected to coordinator")
                })?;

                let status = coordinator::proto::WorkerStatus {
                    state: coordinator::proto::worker_status::State::Training as i32,
                    current_step,
                    current_epoch,
                    current_task: String::new(),
                };

                let request = coordinator::proto::HeartbeatRequest {
                    worker_id,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    status: Some(status),
                    resources: None,
                };

                let response = grpc_client.heartbeat(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("Heartbeat failed: {}", e))
                })?;

                Ok(response.into_inner().acknowledged)
            })
        })
    }

    /// Register a dataset with the coordinator
    ///
    /// Args:
    ///     dataset_id: Unique dataset identifier
    ///     path: Path to the dataset
    ///     total_samples: Total number of samples
    ///     shard_size: Samples per shard
    ///     shuffle: Whether to shuffle (default: True)
    ///     seed: Random seed (default: 42)
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (dataset_id, path, total_samples, shard_size, shuffle=true, seed=42))]
    fn register_dataset(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        path: &str,
        total_samples: i64,
        shard_size: i64,
        shuffle: bool,
        seed: i64,
    ) -> PyResult<i64> {
        self.ensure_connected(py)?;

        let client_lock = self.client.clone();
        let did = dataset_id.to_string();
        let p = path.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                let grpc_client = guard.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Not connected to coordinator")
                })?;

                let request = coordinator::proto::DatasetInfo {
                    dataset_id: did,
                    path: p,
                    format: "auto".to_string(),
                    total_samples,
                    shard_size,
                    shuffle,
                    seed,
                    metadata: HashMap::new(),
                };

                let response = grpc_client.register_dataset(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "Failed to register dataset: {}",
                        e
                    ))
                })?;

                Ok(response.into_inner().total_shards)
            })
        })
    }

    /// Get shard assignment for this worker
    ///
    /// Args:
    ///     dataset_id: Dataset identifier
    ///     epoch: Training epoch
    ///
    /// Returns:
    ///     CoordinatorShardInfo with shard assignment details
    fn get_shard(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        epoch: i64,
    ) -> PyResult<CoordinatorShardInfo> {
        self.ensure_connected(py)?;

        let client_lock = self.client.clone();
        let worker_id = self.get_worker_id(py)?;
        let did = dataset_id.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                let grpc_client = guard.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Not connected to coordinator")
                })?;

                let request = coordinator::proto::ShardRequest {
                    worker_id,
                    dataset_id: did,
                    epoch,
                };

                let response = grpc_client.get_data_shard(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to get shard: {}", e))
                })?;

                let shard = response.into_inner();
                Ok(CoordinatorShardInfo {
                    dataset_id: shard.dataset_id,
                    shard_id: shard.shard_id,
                    total_shards: shard.total_shards,
                    start_index: shard.start_index,
                    end_index: shard.end_index,
                    file_paths: shard.file_paths,
                    epoch: shard.epoch,
                })
            })
        })
    }

    /// Wait at a synchronization barrier
    ///
    /// Args:
    ///     barrier_id: Unique barrier identifier
    ///     step: Training step for this barrier
    ///
    /// Returns:
    ///     BarrierResult with synchronization details
    fn barrier(&self, py: Python<'_>, barrier_id: &str, step: i64) -> PyResult<BarrierResult> {
        self.ensure_connected(py)?;

        let client_lock = self.client.clone();
        let worker_id = self.get_worker_id(py)?;
        let bid = barrier_id.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                let grpc_client = guard.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Not connected to coordinator")
                })?;

                let request = coordinator::proto::BarrierRequest {
                    worker_id,
                    barrier_id: bid,
                    step,
                };

                let response = grpc_client.wait_barrier(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("Barrier failed: {}", e))
                })?;

                let result = response.into_inner();
                Ok(BarrierResult {
                    released: result.released,
                    participants: result.participants,
                    arrival_order: result.arrival_order,
                })
            })
        })
    }

    /// Deregister this worker from the coordinator
    fn deregister(&self, py: Python<'_>) -> PyResult<()> {
        self.ensure_connected(py)?;

        let client_lock = self.client.clone();
        let worker_id = self.get_worker_id(py)?;

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let mut guard: tokio::sync::MutexGuard<'_, Option<Client>> =
                    client_lock.lock().await;
                let grpc_client = guard.as_mut().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err("Not connected to coordinator")
                })?;

                let request = coordinator::proto::WorkerInfo {
                    worker_id,
                    hostname: String::new(),
                    port: 0,
                    gpu_count: 0,
                    memory_bytes: 0,
                    metadata: HashMap::new(),
                };

                grpc_client.deregister_worker(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "Failed to deregister: {}",
                        e
                    ))
                })?;

                Ok(())
            })
        })
    }

    fn __repr__(&self) -> String {
        format!("TrainingOrchestrator(url='{}')", self.coordinator_url)
    }
}

impl TrainingOrchestrator {
    fn ensure_connected(&self, py: Python<'_>) -> PyResult<()> {
        let client_lock = self.client.clone();
        let is_connected = self.runtime.block_on(async {
            let guard: tokio::sync::MutexGuard<'_, Option<Client>> = client_lock.lock().await;
            guard.is_some()
        });

        if !is_connected {
            self.connect(py)?;
        }
        Ok(())
    }

    fn get_worker_id(&self, py: Python<'_>) -> PyResult<String> {
        let worker_id = self.worker_id.clone();
        py.allow_threads(|| {
            self.runtime.block_on(async {
                let guard = worker_id.lock().await;
                guard.clone().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Worker not registered. Call register_worker() first.",
                    )
                })
            })
        })
    }
}
