//! gRPC service implementation for coordinator
//!
//! Implements all methods defined in coordinator.proto

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info};

use checkpoint::{CheckpointManager, CheckpointManagerConfig, CheckpointManagerHandle};
use data_shard::ShardManager;
use runtime_core::{
    ResourceMetrics, WorkerInfo as CoreWorkerInfo, WorkerRegistry, WorkerRegistryHandle,
    WorkerState as CoreWorkerState,
};

use crate::http_api::{
    BarrierResponse as ApiBarrierResponse, CheckpointResponse, DatasetResponse, MetricsResponse,
    WorkerResponse,
};
use crate::proto::{
    self, coordinator_server::Coordinator, BarrierRequest, BarrierResponse, CheckpointAck,
    CheckpointInfo, DatasetAck, DatasetInfo, HeartbeatRequest, HeartbeatResponse, RecoveryRequest,
    RecoveryResponse, ShardAssignment, ShardRequest, WorkerConfig, WorkerInfo,
};

/// Active barrier tracking
struct BarrierState {
    /// Expected participants
    expected: u64,
    /// Arrived participants
    arrived: AtomicU64,
    /// Channels to notify waiting workers
    waiters: parking_lot::Mutex<Vec<tokio::sync::oneshot::Sender<u64>>>,
}

/// Coordinator gRPC service
#[derive(Clone)]
pub struct CoordinatorService {
    /// Worker registry from runtime-core
    workers: WorkerRegistryHandle,

    /// Checkpoint manager
    checkpoint_manager: CheckpointManagerHandle,

    /// Shard manager for data distribution
    shard_manager: Arc<ShardManager>,

    /// Active barriers: barrier_id -> BarrierState
    barriers: Arc<DashMap<String, Arc<BarrierState>>>,

    /// Registered datasets for tracking
    datasets: Arc<DashMap<String, DatasetInfo>>,

    /// Default heartbeat interval in ms
    heartbeat_interval_ms: u64,

    /// Server start time for uptime tracking
    start_time: Instant,

    /// Request counter for metrics
    request_count: Arc<AtomicU64>,
}

impl CoordinatorService {
    /// Create a new coordinator service with default configuration
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::with_config(
            CheckpointManagerConfig::default(),
            10000,
            Duration::from_secs(30),
        )
        .await
    }

    /// Create a new coordinator service with custom configuration
    pub async fn with_config(
        checkpoint_config: CheckpointManagerConfig,
        max_workers: usize,
        heartbeat_timeout: Duration,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let workers = Arc::new(WorkerRegistry::new(max_workers, heartbeat_timeout));
        let checkpoint_manager = Arc::new(
            CheckpointManager::new(checkpoint_config)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        );
        let shard_manager = Arc::new(ShardManager::new());

        Ok(Self {
            workers,
            checkpoint_manager,
            shard_manager,
            barriers: Arc::new(DashMap::new()),
            datasets: Arc::new(DashMap::new()),
            heartbeat_interval_ms: 5000,
            start_time: Instant::now(),
            request_count: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Convert proto WorkerStatus::State to core WorkerState
    fn proto_to_core_state(state: i32) -> CoreWorkerState {
        match proto::worker_status::State::try_from(state) {
            Ok(proto::worker_status::State::Unknown) => CoreWorkerState::Initializing,
            Ok(proto::worker_status::State::Initializing) => CoreWorkerState::Initializing,
            Ok(proto::worker_status::State::Idle) => CoreWorkerState::Idle,
            Ok(proto::worker_status::State::LoadingData) => CoreWorkerState::LoadingData,
            Ok(proto::worker_status::State::Training) => CoreWorkerState::Training,
            Ok(proto::worker_status::State::Checkpointing) => CoreWorkerState::Checkpointing,
            Ok(proto::worker_status::State::Recovering) => CoreWorkerState::Recovering,
            Ok(proto::worker_status::State::Error) => CoreWorkerState::Error,
            Err(_) => CoreWorkerState::Initializing,
        }
    }

    /// Convert proto ResourceUsage to core ResourceMetrics
    fn proto_to_core_resources(resources: Option<proto::ResourceUsage>) -> ResourceMetrics {
        let Some(res) = resources else {
            return ResourceMetrics::default();
        };

        ResourceMetrics {
            cpu_percent: res.cpu_percent,
            memory_used_bytes: res.memory_used_bytes as u64,
            disk_read_bytes: res.disk_read_bytes as u64,
            disk_write_bytes: res.disk_write_bytes as u64,
            network_rx_bytes: res.network_rx_bytes as u64,
            network_tx_bytes: res.network_tx_bytes as u64,
            gpu_metrics: res
                .gpu_usage
                .into_iter()
                .map(|g| runtime_core::GpuMetrics {
                    gpu_id: g.gpu_id as u32,
                    utilization_percent: g.utilization_percent,
                    memory_used_bytes: g.memory_used_bytes as u64,
                    memory_total_bytes: g.memory_total_bytes as u64,
                    temperature_celsius: g.temperature_celsius,
                })
                .collect(),
        }
    }

    // ========== HTTP API Helper Methods ==========

    /// Get server uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get workers for API response
    pub fn get_workers_for_api(&self) -> Vec<WorkerResponse> {
        self.workers
            .all_workers()
            .into_iter()
            .map(|w| {
                let status = match w.state {
                    CoreWorkerState::Idle => "idle",
                    CoreWorkerState::Training => "active",
                    CoreWorkerState::Error => "failed",
                    _ => "unknown",
                };
                WorkerResponse {
                    id: w.id.clone(),
                    ip: w.hostname.clone(),
                    port: w.port,
                    status: status.to_string(),
                    gpu_count: w.gpu_count,
                    last_heartbeat: w.last_heartbeat.timestamp_millis(),
                    assigned_shards: 0,
                    current_epoch: w.current_epoch,
                    current_step: w.current_step,
                    current_task: w.current_task.clone(),
                }
            })
            .collect()
    }

    /// Get datasets for API response
    pub fn get_datasets_for_api(&self) -> Vec<DatasetResponse> {
        self.datasets
            .iter()
            .map(|entry| {
                let d = entry.value();
                let shard_count = (d.total_samples as f64 / d.shard_size as f64).ceil() as u64;
                DatasetResponse {
                    id: d.dataset_id.clone(),
                    name: d.dataset_id.clone(), // Use ID as name for now
                    total_samples: d.total_samples as u64,
                    shard_size: d.shard_size as u64,
                    shard_count,
                    format: d.format.clone(),
                    shuffle: d.shuffle,
                    // Note: Using current time as registration time since we don't persist this yet
                    // In production, this should be stored when dataset is first registered
                    registered_at: Utc::now().timestamp_millis(),
                }
            })
            .collect()
    }

    /// Get checkpoints for API response
    pub fn get_checkpoints_for_api(&self) -> Vec<CheckpointResponse> {
        self.checkpoint_manager
            .all_checkpoints()
            .into_iter()
            .take(20)
            .map(|c| CheckpointResponse {
                id: c.id,
                step: c.step,
                epoch: c.epoch,
                size: c.size_bytes,
                path: c.path,
                created_at: c.created_at.timestamp_millis(),
                worker_id: c.metadata.get("worker_id").cloned().unwrap_or_default(),
                status: "completed".to_string(), // All checkpoints in the list are completed
            })
            .collect()
    }

    /// Get barriers for API response
    pub fn get_barriers_for_api(&self) -> Vec<ApiBarrierResponse> {
        self.barriers
            .iter()
            .map(|entry| {
                let id = entry.key().clone();
                let barrier = entry.value();
                let arrived = barrier.arrived.load(Ordering::Relaxed);
                let status = if arrived >= barrier.expected {
                    "complete"
                } else {
                    "waiting"
                };
                ApiBarrierResponse {
                    id: id.clone(),
                    name: id,
                    arrived,
                    total: barrier.expected,
                    status: status.to_string(),
                    created_at: Utc::now().timestamp_millis(),
                }
            })
            .collect()
    }

    /// Get metrics for API response
    pub fn get_metrics_for_api(&self) -> MetricsResponse {
        let workers = self.workers.all_workers();
        let active_workers = workers
            .iter()
            .filter(|w| matches!(w.state, CoreWorkerState::Training | CoreWorkerState::Idle))
            .count() as u32;

        // Calculate actual metrics from tracked data
        let uptime = self.uptime_secs().max(1);
        let total_requests = self.request_count.load(Ordering::Relaxed);

        MetricsResponse {
            // Checkpoint throughput: checkpoints per minute
            // Note: This is a placeholder until we implement checkpoint event tracking
            checkpoint_throughput: 0,
            // Coordinator requests per second
            coordinator_rps: total_requests / uptime,
            active_workers,
            total_workers: workers.len() as u32,
            // Barrier latency P99: would require histogram tracking
            // Note: Placeholder until we implement latency histograms
            barrier_latency_p99: 50,
            // Shard assignment time: would require timing each assignment
            // Note: Placeholder until we implement operation timing
            shard_assignment_time: 10,
        }
    }

    /// Increment request counter (used by gRPC interceptor)
    #[allow(dead_code)]
    fn increment_request_count(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[tonic::async_trait]
impl Coordinator for CoordinatorService {
    /// Register a new worker with the coordinator
    async fn register_worker(
        &self,
        request: Request<WorkerInfo>,
    ) -> Result<Response<WorkerConfig>, Status> {
        let info = request.into_inner();
        info!(
            worker_id = %info.worker_id,
            hostname = %info.hostname,
            port = info.port,
            gpu_count = info.gpu_count,
            "Worker registration request"
        );

        // Create core worker info
        let core_info = CoreWorkerInfo::new(
            info.worker_id.clone(),
            info.hostname.clone(),
            info.port as u16,
            0, // rank assigned by registry
            0, // world_size updated after registration
        );

        // Register with worker registry
        let registered = self
            .workers
            .register(core_info)
            .map_err(|e| Status::already_exists(format!("Worker registration failed: {}", e)))?;

        // Also register with shard manager for data distribution
        self.shard_manager.register_worker(&info.worker_id);

        // Build response
        let config = WorkerConfig {
            assigned_id: registered.id.clone(),
            rank: registered.rank as i32,
            world_size: self.workers.world_size() as i32,
            heartbeat_interval_ms: self.heartbeat_interval_ms as i64,
            config: info.metadata,
        };

        info!(
            worker_id = %registered.id,
            rank = registered.rank,
            world_size = config.world_size,
            "Worker registered successfully"
        );

        Ok(Response::new(config))
    }

    /// Process worker heartbeat
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let hb = request.into_inner();

        let state = hb
            .status
            .as_ref()
            .map(|s| Self::proto_to_core_state(s.state))
            .unwrap_or(CoreWorkerState::Idle);

        let resources = Self::proto_to_core_resources(hb.resources);

        // Update worker registry
        self.workers
            .heartbeat(&hb.worker_id, state, resources)
            .map_err(|e| Status::not_found(format!("Worker not found: {}", e)))?;

        // Update progress if provided
        if let Some(status) = &hb.status {
            let _ = self.workers.update_progress(
                &hb.worker_id,
                status.current_step as u64,
                status.current_epoch as u64,
                Some(status.current_task.clone()),
            );
        }

        debug!(worker_id = %hb.worker_id, "Heartbeat processed");

        Ok(Response::new(HeartbeatResponse {
            acknowledged: true,
            server_timestamp_ms: Utc::now().timestamp_millis(),
            pending_commands: vec![],
        }))
    }

    /// Deregister a worker
    async fn deregister_worker(
        &self,
        request: Request<WorkerInfo>,
    ) -> Result<Response<WorkerConfig>, Status> {
        let info = request.into_inner();
        info!(worker_id = %info.worker_id, "Worker deregistration request");

        // Remove from registries
        let removed = self
            .workers
            .deregister(&info.worker_id)
            .map_err(|e| Status::not_found(format!("Worker not found: {}", e)))?;

        self.shard_manager.remove_worker(&info.worker_id);

        // Rebalance shards after worker removal
        self.shard_manager.rebalance_shards();

        Ok(Response::new(WorkerConfig {
            assigned_id: removed.id,
            rank: removed.rank as i32,
            world_size: self.workers.world_size() as i32,
            heartbeat_interval_ms: self.heartbeat_interval_ms as i64,
            config: HashMap::new(),
        }))
    }

    /// Register a dataset for sharding
    async fn register_dataset(
        &self,
        request: Request<DatasetInfo>,
    ) -> Result<Response<DatasetAck>, Status> {
        let info = request.into_inner();
        info!(
            dataset_id = %info.dataset_id,
            total_samples = info.total_samples,
            shard_size = info.shard_size,
            "Dataset registration request"
        );

        // Calculate total shards
        let total_shards = (info.total_samples as f64 / info.shard_size as f64).ceil() as u64;

        // Register with shard manager
        self.shard_manager.register_dataset_params(
            &info.dataset_id,
            info.total_samples as u64,
            info.shard_size as u64,
            info.shuffle,
            info.seed as u64,
        );

        // Track dataset info
        self.datasets.insert(info.dataset_id.clone(), info.clone());

        Ok(Response::new(DatasetAck {
            success: true,
            dataset_id: info.dataset_id,
            total_shards: total_shards as i64,
            message: format!("Dataset registered with {} shards", total_shards),
        }))
    }

    /// Get shard assignment for a worker
    async fn get_data_shard(
        &self,
        request: Request<ShardRequest>,
    ) -> Result<Response<ShardAssignment>, Status> {
        let req = request.into_inner();
        debug!(
            worker_id = %req.worker_id,
            dataset_id = %req.dataset_id,
            epoch = req.epoch,
            "Shard request"
        );

        // Get dataset info
        let dataset_info = self
            .datasets
            .get(&req.dataset_id)
            .ok_or_else(|| Status::not_found(format!("Dataset not found: {}", req.dataset_id)))?;

        // Get shard assignments from manager
        let shards = self
            .shard_manager
            .get_shard_for_worker(&req.dataset_id, &req.worker_id, req.epoch as u64)
            .ok_or_else(|| {
                Status::internal(format!(
                    "Failed to get shards for worker {} on dataset {}",
                    req.worker_id, req.dataset_id
                ))
            })?;

        // Return first shard (primary assignment)
        // In practice, a worker might request multiple shards
        if let Some(shard) = shards.first() {
            let total_shards =
                (dataset_info.total_samples as f64 / dataset_info.shard_size as f64).ceil() as i64;

            Ok(Response::new(ShardAssignment {
                dataset_id: req.dataset_id,
                shard_id: shard.shard_id as i64,
                total_shards,
                start_index: shard.start_index as i64,
                end_index: shard.end_index as i64,
                file_paths: vec![dataset_info.path.clone()],
                epoch: req.epoch,
            }))
        } else {
            Err(Status::not_found("No shards available for this worker"))
        }
    }

    /// Notify coordinator of a completed checkpoint
    async fn notify_checkpoint(
        &self,
        request: Request<CheckpointInfo>,
    ) -> Result<Response<CheckpointAck>, Status> {
        let info = request.into_inner();

        // Validate required fields
        if info.checkpoint_id.is_empty() {
            return Err(Status::invalid_argument("checkpoint_id cannot be empty"));
        }
        if info.worker_id.is_empty() {
            return Err(Status::invalid_argument("worker_id cannot be empty"));
        }
        if info.step < 0 {
            return Err(Status::invalid_argument("step must be non-negative"));
        }
        if info.size_bytes < 0 {
            return Err(Status::invalid_argument("size_bytes must be non-negative"));
        }

        info!(
            worker_id = %info.worker_id,
            checkpoint_id = %info.checkpoint_id,
            step = info.step,
            epoch = info.epoch,
            size_bytes = info.size_bytes,
            "Checkpoint notification"
        );

        // Register this checkpoint from the remote worker
        let mut metadata = info.metadata.clone();
        metadata.insert("worker_id".to_string(), info.worker_id.clone());

        self.checkpoint_manager.register_external_checkpoint(
            &info.checkpoint_id,
            info.step as u64,
            info.epoch as u64,
            &info.storage_path,
            info.size_bytes as u64,
            metadata,
        );

        Ok(Response::new(CheckpointAck {
            success: true,
            checkpoint_id: info.checkpoint_id,
            message: "Checkpoint acknowledged".to_string(),
            global_step: info.step,
        }))
    }

    /// Get latest checkpoint for recovery
    async fn get_latest_checkpoint(
        &self,
        request: Request<RecoveryRequest>,
    ) -> Result<Response<RecoveryResponse>, Status> {
        let req = request.into_inner();
        info!(
            worker_id = %req.worker_id,
            job_id = %req.job_id,
            "Recovery request"
        );

        // Get latest checkpoint from manager
        let latest = self.checkpoint_manager.find_recovery_checkpoint();

        if let Some(ckpt) = latest {
            // Get shard assignments for all registered datasets
            let mut shard_assignments = Vec::new();
            for entry in self.datasets.iter() {
                let dataset_info = entry.value();
                if let Some(shards) = self.shard_manager.get_shard_for_worker(
                    &dataset_info.dataset_id,
                    &req.worker_id,
                    ckpt.epoch,
                ) {
                    for shard in shards {
                        let total_shards = (dataset_info.total_samples as f64
                            / dataset_info.shard_size as f64)
                            .ceil() as i64;

                        shard_assignments.push(ShardAssignment {
                            dataset_id: dataset_info.dataset_id.clone(),
                            shard_id: shard.shard_id as i64,
                            total_shards,
                            start_index: shard.start_index as i64,
                            end_index: shard.end_index as i64,
                            file_paths: vec![dataset_info.path.clone()],
                            epoch: ckpt.epoch as i64,
                        });
                    }
                }
            }

            Ok(Response::new(RecoveryResponse {
                has_checkpoint: true,
                latest_checkpoint: Some(proto::CheckpointInfo {
                    worker_id: String::new(), // Not applicable for recovery
                    checkpoint_id: ckpt.id,
                    step: ckpt.step as i64,
                    epoch: ckpt.epoch as i64,
                    storage_path: ckpt.path,
                    size_bytes: ckpt.size_bytes as i64,
                    timestamp_ms: ckpt.created_at.timestamp_millis(),
                    r#type: proto::CheckpointType::Full as i32,
                    metadata: ckpt.metadata,
                }),
                resume_step: ckpt.step as i64,
                resume_epoch: ckpt.epoch as i64,
                shard_assignments,
            }))
        } else {
            Ok(Response::new(RecoveryResponse {
                has_checkpoint: false,
                latest_checkpoint: None,
                resume_step: 0,
                resume_epoch: 0,
                shard_assignments: vec![],
            }))
        }
    }

    /// Barrier synchronization
    async fn wait_barrier(
        &self,
        request: Request<BarrierRequest>,
    ) -> Result<Response<BarrierResponse>, Status> {
        let req = request.into_inner();
        let world_size = self.workers.world_size() as u64;

        info!(
            worker_id = %req.worker_id,
            barrier_id = %req.barrier_id,
            step = req.step,
            world_size = world_size,
            "Barrier wait request"
        );

        // Get or create barrier state - avoid holding entry lock
        let barrier_ref = {
            if let Some(existing) = self.barriers.get(&req.barrier_id) {
                existing.clone()
            } else {
                let new_barrier = Arc::new(BarrierState {
                    expected: world_size,
                    arrived: AtomicU64::new(0),
                    waiters: parking_lot::Mutex::new(Vec::new()),
                });
                self.barriers
                    .entry(req.barrier_id.clone())
                    .or_insert_with(|| {
                        info!(
                            barrier_id = %req.barrier_id,
                            expected = world_size,
                            "Creating new barrier"
                        );
                        new_barrier.clone()
                    });
                self.barriers.get(&req.barrier_id).unwrap().clone()
            }
        };

        // Increment arrived counter
        let arrival_order = barrier_ref.arrived.fetch_add(1, Ordering::SeqCst) + 1;

        info!(
            barrier_id = %req.barrier_id,
            worker_id = %req.worker_id,
            arrival_order = arrival_order,
            expected = barrier_ref.expected,
            "Worker arrived at barrier"
        );

        if arrival_order >= barrier_ref.expected {
            // Last worker to arrive - release all waiters
            let waiters: Vec<_> = barrier_ref.waiters.lock().drain(..).collect();
            for waiter in waiters {
                let _ = waiter.send(arrival_order);
            }

            // Remove barrier for cleanup
            self.barriers.remove(&req.barrier_id);

            info!(
                barrier_id = %req.barrier_id,
                participants = arrival_order,
                "Barrier released"
            );

            Ok(Response::new(BarrierResponse {
                released: true,
                barrier_id: req.barrier_id,
                participants: arrival_order as i64,
                arrival_order: arrival_order as i64,
            }))
        } else {
            // Wait for barrier release
            let (tx, rx) = tokio::sync::oneshot::channel();
            barrier_ref.waiters.lock().push(tx);

            // Wait with timeout
            match tokio::time::timeout(Duration::from_secs(300), rx).await {
                Ok(Ok(participants)) => Ok(Response::new(BarrierResponse {
                    released: true,
                    barrier_id: req.barrier_id,
                    participants: participants as i64,
                    arrival_order: arrival_order as i64,
                })),
                Ok(Err(_)) => Err(Status::internal("Barrier channel closed")),
                Err(_) => Err(Status::deadline_exceeded("Barrier timeout")),
            }
        }
    }

    /// Streaming heartbeats for efficient real-time updates
    type StreamHeartbeatsStream =
        Pin<Box<dyn Stream<Item = Result<HeartbeatResponse, Status>> + Send>>;

    async fn stream_heartbeats(
        &self,
        request: Request<Streaming<HeartbeatRequest>>,
    ) -> Result<Response<Self::StreamHeartbeatsStream>, Status> {
        let mut stream = request.into_inner();
        let workers = self.workers.clone();

        // Create response channel
        let (tx, rx) = mpsc::channel(32);

        // Spawn task to process incoming heartbeats
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(hb) => {
                        let state = hb
                            .status
                            .as_ref()
                            .map(|s| CoordinatorService::proto_to_core_state(s.state))
                            .unwrap_or(CoreWorkerState::Idle);

                        let resources = CoordinatorService::proto_to_core_resources(hb.resources);

                        // Update worker state
                        if let Err(e) = workers.heartbeat(&hb.worker_id, state, resources) {
                            error!(worker_id = %hb.worker_id, error = %e, "Failed to process heartbeat");
                        }

                        // Send response
                        let response = HeartbeatResponse {
                            acknowledged: true,
                            server_timestamp_ms: Utc::now().timestamp_millis(),
                            pending_commands: vec![],
                        };

                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Error in heartbeat stream");
                        break;
                    }
                }
            }
        });

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::StreamHeartbeatsStream
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_service_creation() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };

        let service = CoordinatorService::with_config(config, 100, Duration::from_secs(30))
            .await
            .unwrap();

        assert!(service.workers.world_size() == 0);
    }

    #[tokio::test]
    async fn test_worker_registration() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };

        let service = CoordinatorService::with_config(config, 100, Duration::from_secs(30))
            .await
            .unwrap();

        let request = Request::new(WorkerInfo {
            worker_id: "worker-1".to_string(),
            hostname: "localhost".to_string(),
            port: 50052,
            gpu_count: 2,
            memory_bytes: 16 * 1024 * 1024 * 1024,
            metadata: HashMap::new(),
        });

        let response = service.register_worker(request).await.unwrap();
        let config = response.into_inner();

        assert_eq!(config.assigned_id, "worker-1");
        assert_eq!(config.rank, 0);
        assert_eq!(config.world_size, 1);
    }

    #[tokio::test]
    async fn test_dataset_registration() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };

        let service = CoordinatorService::with_config(config, 100, Duration::from_secs(30))
            .await
            .unwrap();

        // Register a worker first
        let worker_req = Request::new(WorkerInfo {
            worker_id: "worker-1".to_string(),
            hostname: "localhost".to_string(),
            port: 50052,
            gpu_count: 1,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            metadata: HashMap::new(),
        });
        service.register_worker(worker_req).await.unwrap();

        // Register dataset
        let dataset_req = Request::new(DatasetInfo {
            dataset_id: "imagenet".to_string(),
            path: "/data/imagenet".to_string(),
            format: "tfrecord".to_string(),
            total_samples: 1_281_167,
            shard_size: 10_000,
            shuffle: true,
            seed: 42,
            metadata: HashMap::new(),
        });

        let response = service.register_dataset(dataset_req).await.unwrap();
        let ack = response.into_inner();

        assert!(ack.success);
        assert_eq!(ack.dataset_id, "imagenet");
        assert!(ack.total_shards > 0);
    }
}
