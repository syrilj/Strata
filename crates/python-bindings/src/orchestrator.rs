//! Training orchestrator Python bindings
//!
//! High-level orchestration wrapper connecting to coordinator gRPC server.

use coordinator::proto::coordinator_client::CoordinatorClient;
use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, RwLock};
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

/// Recovery checkpoint info returned by coordinator
#[pyclass]
#[derive(Clone)]
pub struct CoordinatorCheckpointInfo {
    #[pyo3(get)]
    pub checkpoint_id: String,
    #[pyo3(get)]
    pub step: i64,
    #[pyo3(get)]
    pub epoch: i64,
    #[pyo3(get)]
    pub storage_path: String,
    #[pyo3(get)]
    pub size_bytes: i64,
    #[pyo3(get)]
    pub resume_step: i64,
    #[pyo3(get)]
    pub resume_epoch: i64,
}

#[pymethods]
impl CoordinatorCheckpointInfo {
    fn __repr__(&self) -> String {
        format!(
            "CoordinatorCheckpointInfo(id='{}', step={}, epoch={}, path='{}')",
            self.checkpoint_id, self.step, self.epoch, self.storage_path
        )
    }
}

// Type alias for the gRPC client (Clone + HTTP/2 multiplexed)
type Client = CoordinatorClient<Channel>;

pub(crate) fn normalize_coordinator_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    }
}

fn parse_worker_state(state: &str) -> i32 {
    match state.trim().to_ascii_lowercase().as_str() {
        "initializing" | "init" => coordinator::proto::worker_status::State::Initializing as i32,
        "idle" => coordinator::proto::worker_status::State::Idle as i32,
        "loading_data" | "loading" => coordinator::proto::worker_status::State::LoadingData as i32,
        "training" | "active" => coordinator::proto::worker_status::State::Training as i32,
        "checkpointing" => coordinator::proto::worker_status::State::Checkpointing as i32,
        "recovering" => coordinator::proto::worker_status::State::Recovering as i32,
        "error" | "failed" => coordinator::proto::worker_status::State::Error as i32,
        _ => coordinator::proto::worker_status::State::Training as i32,
    }
}

fn parse_checkpoint_type(checkpoint_type: &str) -> i32 {
    match checkpoint_type.trim().to_ascii_lowercase().as_str() {
        "incremental" => coordinator::proto::CheckpointType::Incremental as i32,
        "optimizer" | "optimizer_only" => coordinator::proto::CheckpointType::OptimizerOnly as i32,
        "model" | "model_only" => coordinator::proto::CheckpointType::ModelOnly as i32,
        _ => coordinator::proto::CheckpointType::Full as i32,
    }
}

/// High-level training orchestrator for distributed training coordination
///
/// Connects to a coordinator gRPC server to manage worker registration,
/// heartbeats, data sharding, checkpoint notifications, and synchronization barriers.
#[pyclass]
pub struct TrainingOrchestrator {
    client: Arc<RwLock<Option<Client>>>,
    coordinator_url: String,
    runtime: Arc<Runtime>,
    worker_id: Arc<RwLock<Option<String>>>,
    world_size: Option<i32>,
    rank: Option<i32>,
    last_commands: Arc<Mutex<Vec<String>>>,
}

#[pymethods]
impl TrainingOrchestrator {
    /// Create a new training orchestrator
    ///
    /// Args:
    ///     coordinator_url: URL of the coordinator gRPC server (e.g., "http://localhost:50051")
    ///     worker_id: Optional worker identifier
    ///     world_size: Optional expected world size
    ///     rank: Optional worker rank hint
    #[new]
    #[pyo3(signature = (coordinator_url, worker_id=None, world_size=None, rank=None))]
    fn new(
        coordinator_url: &str,
        worker_id: Option<String>,
        world_size: Option<i32>,
        rank: Option<i32>,
    ) -> PyResult<Self> {
        let runtime = Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create async runtime: {}",
                e
            ))
        })?;

        Ok(Self {
            client: Arc::new(RwLock::new(None)),
            coordinator_url: normalize_coordinator_url(coordinator_url),
            runtime: Arc::new(runtime),
            worker_id: Arc::new(RwLock::new(worker_id)),
            world_size,
            rank,
            last_commands: Arc::new(Mutex::new(Vec::new())),
        })
    }

    #[getter]
    fn coordinator_url(&self) -> String {
        self.coordinator_url.clone()
    }

    #[getter]
    fn worker_id(&self) -> Option<String> {
        self.runtime
            .block_on(async { self.worker_id.read().await.clone() })
    }

    #[getter]
    fn world_size(&self) -> Option<i32> {
        self.world_size
    }

    #[getter]
    fn rank(&self) -> Option<i32> {
        self.rank
    }

    #[getter]
    fn last_commands(&self) -> Vec<String> {
        self.runtime
            .block_on(async { self.last_commands.lock().await.clone() })
    }

    /// Connect to the coordinator server
    ///
    /// This is called automatically by other methods if not already connected.
    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let client_lock = self.client.clone();
        let url = self.coordinator_url.clone();

        py.allow_threads(|| {
            self.runtime
                .block_on(Self::get_or_connect_client(client_lock, url))
                .map(|_| ())
        })
    }

    /// Register this worker with the coordinator
    #[pyo3(signature = (worker_id=None, hostname="127.0.0.1", port=50052, gpu_count=0, memory_bytes=0, ip=None, metadata=None))]
    #[allow(clippy::too_many_arguments)]
    fn register_worker(
        &self,
        py: Python<'_>,
        worker_id: Option<&str>,
        hostname: &str,
        port: i32,
        gpu_count: i32,
        memory_bytes: i64,
        ip: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<WorkerConfig> {
        let mut grpc_client = self.get_client_sync(py)?;
        let wid = match worker_id {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => self.get_worker_id_sync(py)?,
        };
        let host = ip.unwrap_or(hostname).to_string();
        let mut meta = metadata.unwrap_or_default();
        if let Some(ws) = self.world_size {
            meta.entry("world_size".to_string())
                .or_insert_with(|| ws.to_string());
        }
        if let Some(r) = self.rank {
            meta.entry("rank".to_string())
                .or_insert_with(|| r.to_string());
        }
        let worker_id_store = self.worker_id.clone();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
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
                let assigned = if config.assigned_id.is_empty() {
                    wid
                } else {
                    config.assigned_id.clone()
                };
                *worker_id_store.write().await = Some(assigned.clone());

                Ok(WorkerConfig {
                    worker_id: assigned,
                    rank: config.rank,
                    world_size: config.world_size,
                    heartbeat_interval_ms: config.heartbeat_interval_ms,
                })
            })
        })
    }

    /// Async version of register_worker
    #[pyo3(signature = (worker_id=None, hostname="127.0.0.1", port=50052, gpu_count=0, memory_bytes=0, ip=None, metadata=None))]
    #[allow(clippy::too_many_arguments)]
    fn register_worker_async<'py>(
        &self,
        py: Python<'py>,
        worker_id: Option<String>,
        hostname: &str,
        port: i32,
        gpu_count: i32,
        memory_bytes: i64,
        ip: Option<String>,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client_lock = self.client.clone();
        let url = self.coordinator_url.clone();
        let worker_id_store = self.worker_id.clone();
        let host = ip.unwrap_or_else(|| hostname.to_string());
        let mut meta = metadata.unwrap_or_default();
        if let Some(ws) = self.world_size {
            meta.entry("world_size".to_string())
                .or_insert_with(|| ws.to_string());
        }
        if let Some(r) = self.rank {
            meta.entry("rank".to_string())
                .or_insert_with(|| r.to_string());
        }

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut grpc_client = Self::get_or_connect_client(client_lock, url).await?;
            let wid = match worker_id {
                Some(id) if !id.is_empty() => id,
                _ => {
                    let guard = worker_id_store.read().await;
                    guard.clone().ok_or_else(|| {
                        pyo3::exceptions::PyRuntimeError::new_err(
                            "Worker ID not provided and not set on orchestrator.",
                        )
                    })?
                }
            };

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
            let assigned = if config.assigned_id.is_empty() {
                wid
            } else {
                config.assigned_id.clone()
            };
            *worker_id_store.write().await = Some(assigned.clone());

            Ok(WorkerConfig {
                worker_id: assigned,
                rank: config.rank,
                world_size: config.world_size,
                heartbeat_interval_ms: config.heartbeat_interval_ms,
            })
        })
    }

    /// Send a heartbeat to the coordinator
    #[pyo3(signature = (current_step=0, current_epoch=0, state="training", current_task="", cpu_percent=0.0, memory_used_bytes=0))]
    #[allow(clippy::too_many_arguments)]
    fn heartbeat(
        &self,
        py: Python<'_>,
        current_step: i64,
        current_epoch: i64,
        state: &str,
        current_task: &str,
        cpu_percent: f64,
        memory_used_bytes: i64,
    ) -> PyResult<bool> {
        let (ack, _) = self.heartbeat_with_commands(
            py,
            current_step,
            current_epoch,
            state,
            current_task,
            cpu_percent,
            memory_used_bytes,
        )?;
        Ok(ack)
    }

    /// Send a heartbeat and return (acknowledged, pending_commands)
    #[pyo3(signature = (current_step=0, current_epoch=0, state="training", current_task="", cpu_percent=0.0, memory_used_bytes=0))]
    #[allow(clippy::too_many_arguments)]
    fn heartbeat_with_commands(
        &self,
        py: Python<'_>,
        current_step: i64,
        current_epoch: i64,
        state: &str,
        current_task: &str,
        cpu_percent: f64,
        memory_used_bytes: i64,
    ) -> PyResult<(bool, Vec<String>)> {
        let mut grpc_client = self.get_client_sync(py)?;
        let worker_id = self.get_worker_id_sync(py)?;
        let state_enum = parse_worker_state(state);
        let task = current_task.to_string();
        let last_commands = self.last_commands.clone();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let status = coordinator::proto::WorkerStatus {
                    state: state_enum,
                    current_step,
                    current_epoch,
                    current_task: task,
                };

                let resources = if cpu_percent > 0.0 || memory_used_bytes > 0 {
                    Some(coordinator::proto::ResourceUsage {
                        cpu_percent,
                        memory_used_bytes,
                        gpu_usage: vec![],
                        disk_read_bytes: 0,
                        disk_write_bytes: 0,
                        network_rx_bytes: 0,
                        network_tx_bytes: 0,
                    })
                } else {
                    None
                };

                let request = coordinator::proto::HeartbeatRequest {
                    worker_id,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    status: Some(status),
                    resources,
                };

                let response = grpc_client.heartbeat(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("Heartbeat failed: {}", e))
                })?;

                let inner = response.into_inner();
                *last_commands.lock().await = inner.pending_commands.clone();
                Ok((inner.acknowledged, inner.pending_commands))
            })
        })
    }

    /// Async version of heartbeat
    #[pyo3(signature = (current_step=0, current_epoch=0, state="training", current_task="", cpu_percent=0.0, memory_used_bytes=0))]
    #[allow(clippy::too_many_arguments)]
    fn heartbeat_async<'py>(
        &self,
        py: Python<'py>,
        current_step: i64,
        current_epoch: i64,
        state: &str,
        current_task: &str,
        cpu_percent: f64,
        memory_used_bytes: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client_lock = self.client.clone();
        let url = self.coordinator_url.clone();
        let worker_id_store = self.worker_id.clone();
        let state_enum = parse_worker_state(state);
        let task = current_task.to_string();
        let last_commands = self.last_commands.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut grpc_client = Self::get_or_connect_client(client_lock, url).await?;
            let worker_id = {
                let guard = worker_id_store.read().await;
                guard.clone().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Worker not registered. Call register_worker() first.",
                    )
                })?
            };

            let status = coordinator::proto::WorkerStatus {
                state: state_enum,
                current_step,
                current_epoch,
                current_task: task,
            };

            let resources = if cpu_percent > 0.0 || memory_used_bytes > 0 {
                Some(coordinator::proto::ResourceUsage {
                    cpu_percent,
                    memory_used_bytes,
                    gpu_usage: vec![],
                    disk_read_bytes: 0,
                    disk_write_bytes: 0,
                    network_rx_bytes: 0,
                    network_tx_bytes: 0,
                })
            } else {
                None
            };

            let request = coordinator::proto::HeartbeatRequest {
                worker_id,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                status: Some(status),
                resources,
            };

            let response = grpc_client.heartbeat(request).await.map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("Heartbeat failed: {}", e))
            })?;

            let inner = response.into_inner();
            *last_commands.lock().await = inner.pending_commands.clone();
            Ok(inner.acknowledged)
        })
    }

    /// Register a dataset with the coordinator
    #[pyo3(signature = (dataset_id, path, total_samples, shard_size, format="parquet", shuffle=true, seed=42))]
    #[allow(clippy::too_many_arguments)]
    fn register_dataset(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        path: &str,
        total_samples: i64,
        shard_size: i64,
        format: &str,
        shuffle: bool,
        seed: i64,
    ) -> PyResult<bool> {
        let mut grpc_client = self.get_client_sync(py)?;
        let did = dataset_id.to_string();
        let p = path.to_string();
        let fmt = format.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let request = coordinator::proto::DatasetInfo {
                    dataset_id: did,
                    path: p,
                    format: fmt,
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

                Ok(response.into_inner().success)
            })
        })
    }

    /// Get shard assignment for this worker
    #[pyo3(signature = (dataset_id, epoch=0))]
    fn get_shard(
        &self,
        py: Python<'_>,
        dataset_id: &str,
        epoch: i64,
    ) -> PyResult<CoordinatorShardInfo> {
        let mut grpc_client = self.get_client_sync(py)?;
        let worker_id = self.get_worker_id_sync(py)?;
        let did = dataset_id.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
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

    /// Notify coordinator about a saved checkpoint
    #[pyo3(signature = (checkpoint_id, step, epoch, storage_path, size_bytes, checkpoint_type="full", metadata=None))]
    #[allow(clippy::too_many_arguments)]
    fn notify_checkpoint(
        &self,
        py: Python<'_>,
        checkpoint_id: &str,
        step: i64,
        epoch: i64,
        storage_path: &str,
        size_bytes: i64,
        checkpoint_type: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<bool> {
        let mut grpc_client = self.get_client_sync(py)?;
        let worker_id = self.get_worker_id_sync(py)?;
        let ckpt_id = checkpoint_id.to_string();
        let path = storage_path.to_string();
        let ckpt_type = parse_checkpoint_type(checkpoint_type);
        let meta = metadata.unwrap_or_default();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let request = coordinator::proto::CheckpointInfo {
                    worker_id,
                    checkpoint_id: ckpt_id,
                    step,
                    epoch,
                    storage_path: path,
                    size_bytes,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    r#type: ckpt_type,
                    metadata: meta,
                };

                let response = grpc_client.notify_checkpoint(request).await.map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "Failed to notify checkpoint: {}",
                        e
                    ))
                })?;

                Ok(response.into_inner().success)
            })
        })
    }

    /// Get the latest checkpoint from the coordinator for recovery
    #[pyo3(signature = (job_id=""))]
    fn get_latest_checkpoint(
        &self,
        py: Python<'_>,
        job_id: &str,
    ) -> PyResult<Option<CoordinatorCheckpointInfo>> {
        let mut grpc_client = self.get_client_sync(py)?;
        let worker_id = self.get_worker_id_sync(py).unwrap_or_default();
        let jid = job_id.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                let request = coordinator::proto::RecoveryRequest {
                    worker_id,
                    job_id: jid,
                };

                let response = grpc_client
                    .get_latest_checkpoint(request)
                    .await
                    .map_err(|e| {
                        pyo3::exceptions::PyRuntimeError::new_err(format!(
                            "Failed to get latest checkpoint: {}",
                            e
                        ))
                    })?;

                let resp = response.into_inner();
                if !resp.has_checkpoint {
                    return Ok(None);
                }

                Ok(resp
                    .latest_checkpoint
                    .map(|ckpt| CoordinatorCheckpointInfo {
                        checkpoint_id: ckpt.checkpoint_id,
                        step: ckpt.step,
                        epoch: ckpt.epoch,
                        storage_path: ckpt.storage_path,
                        size_bytes: ckpt.size_bytes,
                        resume_step: resp.resume_step,
                        resume_epoch: resp.resume_epoch,
                    }))
            })
        })
    }

    /// Wait at a synchronization barrier
    #[pyo3(signature = (barrier_id, step=0))]
    fn barrier(&self, py: Python<'_>, barrier_id: &str, step: i64) -> PyResult<BarrierResult> {
        let mut grpc_client = self.get_client_sync(py)?;
        let worker_id = self.get_worker_id_sync(py)?;
        let bid = barrier_id.to_string();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
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

    /// Alias for barrier() matching async/sync caller expectations
    #[pyo3(signature = (barrier_id, step=0))]
    fn wait_barrier(&self, py: Python<'_>, barrier_id: &str, step: i64) -> PyResult<BarrierResult> {
        self.barrier(py, barrier_id, step)
    }

    /// Async version of wait_barrier
    #[pyo3(signature = (barrier_id, step=0))]
    fn wait_barrier_async<'py>(
        &self,
        py: Python<'py>,
        barrier_id: &str,
        step: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client_lock = self.client.clone();
        let url = self.coordinator_url.clone();
        let worker_id_store = self.worker_id.clone();
        let bid = barrier_id.to_string();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut grpc_client = Self::get_or_connect_client(client_lock, url).await?;
            let worker_id = {
                let guard = worker_id_store.read().await;
                guard.clone().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Worker not registered. Call register_worker() first.",
                    )
                })?
            };

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
    }

    /// Deregister this worker from the coordinator
    fn deregister(&self, py: Python<'_>) -> PyResult<()> {
        let mut grpc_client = self.get_client_sync(py)?;
        let worker_id = self.get_worker_id_sync(py)?;

        py.allow_threads(|| {
            self.runtime.block_on(async move {
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
    async fn get_or_connect_client(
        client_lock: Arc<RwLock<Option<Client>>>,
        url: String,
    ) -> PyResult<Client> {
        {
            let guard = client_lock.read().await;
            if let Some(client) = guard.as_ref() {
                return Ok(client.clone());
            }
        }

        let channel = Channel::from_shared(url)
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid coordinator URL: {}", e))
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
    }

    fn get_client_sync(&self, py: Python<'_>) -> PyResult<Client> {
        let client_lock = self.client.clone();
        let url = self.coordinator_url.clone();
        py.allow_threads(|| {
            self.runtime
                .block_on(Self::get_or_connect_client(client_lock, url))
        })
    }

    fn get_worker_id_sync(&self, py: Python<'_>) -> PyResult<String> {
        let worker_id = self.worker_id.clone();
        py.allow_threads(|| {
            self.runtime.block_on(async {
                let guard = worker_id.read().await;
                guard.clone().ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(
                        "Worker not registered. Call register_worker() first.",
                    )
                })
            })
        })
    }
}
