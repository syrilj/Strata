//! gRPC service implementation for coordinator
//!
//! Implements all methods defined in coordinator.proto

use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use storage::{LocalStorage, StorageBackend};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

use checkpoint::{CheckpointManager, CheckpointManagerConfig, CheckpointManagerHandle};
use data_shard::ShardManager;
use runtime_core::{
    EventLog, LogEvent, ResourceMetrics, RuntimeConfig, WorkerInfo as CoreWorkerInfo,
    WorkerRegistry, WorkerRegistryHandle, WorkerState as CoreWorkerState,
};

use crate::http_api::{
    BarrierResponse as ApiBarrierResponse, CheckpointResponse, DatasetResponse, GpuUsageResponse,
    LogResponse, MetricsResponse, TaskResponse, ThroughputPoint, WorkerResourcesResponse,
    WorkerResponse,
};
use crate::middleware::{InputValidator, RateLimiter, RequestMetrics};
use crate::proto::{
    self, coordinator_server::Coordinator, BarrierRequest, BarrierResponse, CheckpointAck,
    CheckpointInfo, DatasetAck, DatasetInfo, HeartbeatRequest, HeartbeatResponse, RecoveryRequest,
    RecoveryResponse, ShardAssignment, ShardRequest, WorkerConfig, WorkerInfo,
};

const MAX_THROUGHPUT_HISTORY: usize = 120;
const MAX_COMPLETED_BARRIERS: usize = 20;
const MAX_RECENT_REQUESTS: usize = 10_000;
const MAX_RECENT_CHECKPOINTS: usize = 256;
const RPS_WINDOW_SECS: u64 = 10;
const CHECKPOINT_WINDOW_SECS: f64 = 60.0;

/// Internal tracked dataset entry preserving registration timestamp and name
#[derive(Debug, Clone)]
pub struct TrackedDataset {
    pub info: DatasetInfo,
    pub name: String,
    pub registered_at: i64,
}

/// Internal tracked task entry
#[derive(Debug, Clone)]
pub struct TrackedTask {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub status: String,
    pub worker_ids: Vec<String>,
    pub dataset_id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub current_step: u64,
    pub current_epoch: u64,
    pub total_epochs: u64,
    pub progress: u32,
    pub logs: Vec<String>,
}

impl TrackedTask {
    pub fn to_response(&self) -> TaskResponse {
        TaskResponse {
            id: self.id.clone(),
            name: self.name.clone(),
            r#type: self.task_type.clone(),
            status: self.status.clone(),
            worker_ids: self.worker_ids.clone(),
            dataset_id: self.dataset_id.clone(),
            started_at: self.started_at,
            completed_at: self.completed_at,
            progress: self.progress,
            logs: self.logs.clone(),
        }
    }
}

/// Active barrier tracking
struct BarrierState {
    /// Expected participants
    expected: AtomicU64,
    /// Deduplicated arrived worker IDs
    arrived_workers: parking_lot::Mutex<HashSet<String>>,
    /// Channels to notify waiting workers
    waiters: parking_lot::Mutex<Vec<tokio::sync::oneshot::Sender<u64>>>,
    /// Barrier creation time for latency measurement
    created_at_instant: Instant,
    /// Barrier creation timestamp (Unix ms)
    created_at_ms: i64,
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

    /// Recently completed barriers (capped at 20)
    completed_barriers: Arc<RwLock<VecDeque<ApiBarrierResponse>>>,

    /// Registered datasets for tracking
    datasets: Arc<DashMap<String, TrackedDataset>>,

    /// Tasks store (moved from global static into service state)
    tasks: Arc<RwLock<HashMap<String, TrackedTask>>>,

    /// Per-worker pending command queues returned on heartbeat
    pending_commands: Arc<DashMap<String, VecDeque<String>>>,

    /// Bounded structured event log
    event_log: Arc<EventLog>,

    /// Middleware components
    rate_limiter: Arc<RateLimiter>,
    input_validator: Arc<InputValidator>,
    request_metrics: Arc<RequestMetrics>,

    /// Recent request timestamps for rolling RPS
    recent_requests: Arc<RwLock<VecDeque<Instant>>>,

    /// Recent checkpoint completion timestamps & sizes for throughput calculation
    recent_checkpoints: Arc<RwLock<VecDeque<(Instant, u64)>>>,

    /// Rolling throughput/RPS history (capped at 120 points)
    throughput_history: Arc<RwLock<VecDeque<ThroughputPoint>>>,

    /// Default heartbeat interval in ms
    heartbeat_interval_ms: u64,

    /// Dead worker reaper check interval
    dead_worker_check_interval: Duration,

    /// Server start time for uptime tracking
    start_time: Instant,

    /// Total request counter for metrics
    request_count: Arc<AtomicU64>,

    /// Configured gRPC listen/display address
    grpc_address: String,

    /// Configured HTTP API listen/display address
    http_address: String,

    /// Storage backend label ("local" or "s3")
    storage_backend: String,

    /// Whether demo mode is enabled
    demo_mode: bool,
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
        tokio::fs::create_dir_all(&checkpoint_config.base_path).await?;
        let storage: Arc<dyn StorageBackend> =
            Arc::new(LocalStorage::new(&checkpoint_config.base_path));
        Self::with_runtime_and_storage(
            checkpoint_config,
            storage,
            "local".to_string(),
            max_workers,
            heartbeat_timeout,
            Duration::from_secs(5),
            "0.0.0.0:50051".to_string(),
            "0.0.0.0:51051".to_string(),
            false,
        )
        .await
    }

    /// Create a new coordinator service from `RuntimeConfig` and an explicit `StorageBackend`
    pub async fn from_runtime_config(
        runtime_config: &RuntimeConfig,
        storage: Arc<dyn StorageBackend>,
        grpc_addr: String,
        http_addr: String,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let checkpoint_config = CheckpointManagerConfig {
            base_path: runtime_config.storage.base_path.clone().into(),
            keep_count: runtime_config.checkpoint.keep_count,
            write_buffer_size: runtime_config.checkpoint.write_buffer_size,
            compression: runtime_config.checkpoint.compression,
            compression_level: runtime_config.checkpoint.compression_level,
        };
        let storage_backend = match runtime_config.storage.backend {
            runtime_core::config::StorageBackend::S3 { .. } => "s3".to_string(),
            runtime_core::config::StorageBackend::Local => "local".to_string(),
        };

        Self::with_runtime_and_storage(
            checkpoint_config,
            storage,
            storage_backend,
            runtime_config.coordinator.max_workers,
            runtime_config.coordinator.heartbeat_timeout,
            runtime_config.coordinator.dead_worker_check_interval,
            grpc_addr,
            http_addr,
            runtime_config.coordinator.demo_mode,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn with_runtime_and_storage(
        checkpoint_config: CheckpointManagerConfig,
        storage: Arc<dyn StorageBackend>,
        storage_backend: String,
        max_workers: usize,
        heartbeat_timeout: Duration,
        dead_worker_check_interval: Duration,
        grpc_address: String,
        http_address: String,
        demo_mode: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let workers = Arc::new(WorkerRegistry::new(max_workers, heartbeat_timeout));
        let checkpoint_manager = Arc::new(
            CheckpointManager::with_storage(checkpoint_config, storage)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        );
        let shard_manager = Arc::new(ShardManager::new());
        let event_log = Arc::new(EventLog::new(1000));
        event_log.push(
            "info",
            "coordinator",
            format!(
                "Coordinator service initialized (gRPC={}, HTTP={}, storage={})",
                grpc_address, http_address, storage_backend
            ),
            None,
        );

        let service = Self {
            workers,
            checkpoint_manager,
            shard_manager,
            barriers: Arc::new(DashMap::new()),
            completed_barriers: Arc::new(RwLock::new(VecDeque::with_capacity(
                MAX_COMPLETED_BARRIERS,
            ))),
            datasets: Arc::new(DashMap::new()),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            pending_commands: Arc::new(DashMap::new()),
            event_log,
            rate_limiter: Arc::new(RateLimiter::new(10_000, 20_000)),
            input_validator: Arc::new(InputValidator::new()),
            request_metrics: Arc::new(RequestMetrics::new()),
            recent_requests: Arc::new(RwLock::new(VecDeque::with_capacity(1024))),
            recent_checkpoints: Arc::new(RwLock::new(VecDeque::with_capacity(64))),
            throughput_history: Arc::new(RwLock::new(VecDeque::with_capacity(
                MAX_THROUGHPUT_HISTORY,
            ))),
            heartbeat_interval_ms: 5000,
            dead_worker_check_interval,
            start_time: Instant::now(),
            request_count: Arc::new(AtomicU64::new(0)),
            grpc_address,
            http_address,
            storage_backend,
            demo_mode,
        };

        // Seed initial throughput point on startup
        service.record_throughput_sample();

        Ok(service)
    }

    /// Spawn background tasks for dead-worker reaping and metrics history sampling.
    pub fn spawn_background_tasks(self: &Arc<Self>) {
        // Dead-worker reaper (every dead_worker_check_interval, default 5s)
        let reaper_svc = Arc::clone(self);
        let check_interval = reaper_svc
            .dead_worker_check_interval
            .max(Duration::from_secs(1));
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(check_interval);
            interval.tick().await; // skip immediate initial tick
            loop {
                interval.tick().await;
                reaper_svc.reap_dead_workers();
            }
        });

        // Metrics history sampler (every 2s)
        let sampler_svc = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            interval.tick().await; // initial point already seeded on startup
            loop {
                interval.tick().await;
                sampler_svc.record_throughput_sample();
            }
        });
    }

    /// Reap dead workers, rebalance shards, log warnings, and re-evaluate active barriers
    pub fn reap_dead_workers(&self) -> Vec<String> {
        let dead = self.workers.check_dead_workers();
        if dead.is_empty() {
            return dead;
        }

        for worker_id in &dead {
            self.shard_manager.remove_worker(worker_id);
            self.pending_commands.remove(worker_id);
            self.event_log.push(
                "warn",
                "worker_registry",
                format!("Worker '{}' marked dead (heartbeat timeout)", worker_id),
                None,
            );
        }

        self.shard_manager.rebalance_shards();
        self.reevaluate_active_barriers();
        dead
    }

    /// Re-evaluate active barriers when live worker count drops
    fn reevaluate_active_barriers(&self) {
        let active_count = self.workers.active_workers().len() as u64;
        if active_count == 0 {
            return;
        }

        let barrier_ids: Vec<String> = self.barriers.iter().map(|e| e.key().clone()).collect();
        for barrier_id in barrier_ids {
            if let Some(barrier_ref) = self.barriers.get(&barrier_id).map(|e| e.clone()) {
                let current_expected = barrier_ref.expected.load(Ordering::Acquire);
                if current_expected > active_count {
                    barrier_ref.expected.store(active_count, Ordering::Release);
                }
                let expected = barrier_ref.expected.load(Ordering::Acquire);
                let arrived = barrier_ref.arrived_workers.lock().len() as u64;
                if arrived >= expected {
                    self.release_barrier(&barrier_id, &barrier_ref, arrived);
                }
            }
        }
    }

    /// Record a throughput history sample into the rolling 120-point buffer
    pub fn record_throughput_sample(&self) {
        let metrics = self.get_metrics_for_api();
        let now = Utc::now();
        let point = ThroughputPoint {
            timestamp: now.timestamp_millis(),
            time: now.format("%H:%M:%S").to_string(),
            checkpoint_throughput: metrics.checkpoint_throughput,
            coordinator_rps: metrics.coordinator_rps,
            active_workers: metrics.active_workers as usize,
            barrier_latency_p99: metrics.barrier_latency_p99,
        };

        let mut history = self.throughput_history.write();
        if history.len() >= MAX_THROUGHPUT_HISTORY {
            history.pop_front();
        }
        history.push_back(point);
    }

    /// Record request arrival, latency, and error status in middleware metrics
    pub fn record_request_outcome(&self, method: &str, _client_id: &str, start: Instant, ok: bool) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
        self.request_metrics.record_request(method);
        let elapsed_us = start.elapsed().as_micros() as u64;
        self.request_metrics.record_latency(method, elapsed_us);
        if !ok {
            self.request_metrics.record_error(method);
        }

        let now = Instant::now();
        let mut recent = self.recent_requests.write();
        let cutoff = now - Duration::from_secs(RPS_WINDOW_SECS);
        while recent.front().is_some_and(|t| *t < cutoff) {
            recent.pop_front();
        }
        if recent.len() >= MAX_RECENT_REQUESTS {
            recent.pop_front();
        }
        recent.push_back(now);
    }

    /// Check rate limit for a client and return tonic::Status on excess
    #[allow(clippy::result_large_err)]
    fn check_rate_limit(&self, client_id: &str) -> Result<(), Status> {
        self.rate_limiter.check(client_id).map_err(|retry| {
            Status::resource_exhausted(format!(
                "Rate limit exceeded, retry after {}ms",
                retry.as_millis()
            ))
        })
    }

    /// Enqueue a command to be delivered to a worker on its next heartbeat
    pub fn enqueue_worker_command(&self, worker_id: &str, command: String) {
        let mut entry = self
            .pending_commands
            .entry(worker_id.to_string())
            .or_default();
        entry.push_back(command);
    }

    /// Drain all pending commands for a worker
    fn drain_worker_commands(&self, worker_id: &str) -> Vec<String> {
        if let Some(mut entry) = self.pending_commands.get_mut(worker_id) {
            entry.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    /// Push a log event into the coordinator's bounded event log
    pub fn log_event(
        &self,
        level: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
        task_id: Option<String>,
    ) -> LogEvent {
        self.event_log.push(level, source, message, task_id)
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

    /// Release a barrier, record latency, push completed entry, and notify waiters
    fn release_barrier(
        &self,
        barrier_id: &str,
        barrier_ref: &Arc<BarrierState>,
        participants: u64,
    ) {
        let waiters: Vec<_> = barrier_ref.waiters.lock().drain(..).collect();
        for waiter in waiters {
            let _ = waiter.send(participants);
        }

        // Only record completion if we actually removed the active barrier (prevents duplicate records)
        if self.barriers.remove(barrier_id).is_some() {
            let latency_us = barrier_ref.created_at_instant.elapsed().as_micros() as u64;
            self.request_metrics
                .record_latency("barrier_sync", latency_us);

            let expected = barrier_ref.expected.load(Ordering::Acquire);
            let mut completed = self.completed_barriers.write();
            if completed.len() >= MAX_COMPLETED_BARRIERS {
                completed.pop_front();
            }
            completed.push_back(ApiBarrierResponse {
                id: barrier_id.to_string(),
                name: barrier_id.to_string(),
                arrived: participants,
                total: expected,
                status: "complete".to_string(),
                created_at: barrier_ref.created_at_ms,
            });

            self.event_log.push(
                "info",
                "barrier",
                format!(
                    "Barrier '{}' released ({}/{} participants, {}ms)",
                    barrier_id,
                    participants,
                    expected,
                    latency_us / 1000
                ),
                None,
            );
        }
    }

    /// Update task progress from worker heartbeat status
    fn update_task_from_heartbeat(&self, worker_id: &str, step: u64, epoch: u64, task_id: &str) {
        if task_id.is_empty() {
            return;
        }
        let mut tasks = self.tasks.write();
        if let Some(task) = tasks.get_mut(task_id) {
            if task.status == "stopped" || task.status == "failed" || task.status == "completed" {
                return;
            }
            task.current_step = task.current_step.max(step);
            task.current_epoch = task.current_epoch.max(epoch);
            let total_epochs = task.total_epochs.max(1);
            let pct = ((task.current_epoch as f64 / total_epochs as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u32;
            task.progress = pct;

            if task.current_epoch >= total_epochs {
                task.status = "completed".to_string();
                task.progress = 100;
                task.completed_at = Some(Utc::now().timestamp_millis());
                let msg = format!(
                    "[{}] Task '{}' completed ({} epochs, step {}) by {}",
                    Utc::now().format("%H:%M:%S"),
                    task.name,
                    total_epochs,
                    task.current_step,
                    worker_id
                );
                task.logs.push(msg.clone());
                drop(tasks);
                self.event_log.push(
                    "info",
                    "task_manager",
                    format!("Task '{}' completed at epoch {}", task_id, epoch),
                    Some(task_id.to_string()),
                );
            }
        }
    }

    /// Assign pending tasks when a new worker registers
    fn dispatch_pending_tasks_to_worker(&self, worker_id: &str) {
        let mut tasks = self.tasks.write();
        let mut dispatched = Vec::new();
        for task in tasks.values_mut() {
            if task.status == "pending" {
                if !task.worker_ids.contains(&worker_id.to_string()) {
                    task.worker_ids.push(worker_id.to_string());
                }
                task.status = "running".to_string();
                task.logs.push(format!(
                    "[{}] Assigned worker '{}' and started task",
                    Utc::now().format("%H:%M:%S"),
                    worker_id
                ));
                dispatched.push((task.id.clone(), task.dataset_id.clone()));
            }
        }
        drop(tasks);

        for (task_id, dataset_id) in dispatched {
            self.enqueue_worker_command(
                worker_id,
                format!("START_TASK:{}:{}", task_id, dataset_id),
            );
            self.event_log.push(
                "info",
                "task_manager",
                format!(
                    "Dispatched pending task '{}' to worker '{}'",
                    task_id, worker_id
                ),
                Some(task_id),
            );
        }
    }

    // ========== Accessors & HTTP API Helper Methods ==========

    pub fn grpc_address(&self) -> &str {
        &self.grpc_address
    }

    pub fn http_address(&self) -> &str {
        &self.http_address
    }

    pub fn storage_backend(&self) -> &str {
        &self.storage_backend
    }

    pub fn is_demo_mode(&self) -> bool {
        if let Ok(val) = std::env::var("DEMO_MODE") {
            return matches!(
                val.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            );
        }
        self.demo_mode
    }

    pub fn workers_handle(&self) -> &WorkerRegistryHandle {
        &self.workers
    }

    pub fn shard_manager(&self) -> &Arc<ShardManager> {
        &self.shard_manager
    }

    pub fn checkpoint_manager(&self) -> &CheckpointManagerHandle {
        &self.checkpoint_manager
    }

    pub fn event_log(&self) -> &Arc<EventLog> {
        &self.event_log
    }

    /// Get server uptime in seconds
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    fn worker_to_response(&self, w: &CoreWorkerInfo) -> WorkerResponse {
        let status = match w.state {
            CoreWorkerState::Training => "training",
            CoreWorkerState::LoadingData => "loading_data",
            CoreWorkerState::Checkpointing => "checkpointing",
            CoreWorkerState::Recovering => "recovering",
            CoreWorkerState::Idle | CoreWorkerState::Initializing => "idle",
            CoreWorkerState::Error | CoreWorkerState::Dead | CoreWorkerState::Disconnecting => {
                "failed"
            }
        };
        let assigned_shards = self.shard_manager.get_worker_shard_count(&w.id) as u32;
        WorkerResponse {
            id: w.id.clone(),
            ip: w.hostname.clone(),
            port: w.port,
            status: status.to_string(),
            gpu_count: w.gpu_count,
            last_heartbeat: w.last_heartbeat.timestamp_millis(),
            assigned_shards,
            current_epoch: w.current_epoch,
            current_step: w.current_step,
            current_task: w.current_task.clone(),
            resources: Some(WorkerResourcesResponse {
                cpu_percent: w.resources.cpu_percent,
                memory_used_bytes: w.resources.memory_used_bytes,
                disk_read_bytes: w.resources.disk_read_bytes,
                disk_write_bytes: w.resources.disk_write_bytes,
                network_rx_bytes: w.resources.network_rx_bytes,
                network_tx_bytes: w.resources.network_tx_bytes,
                gpu_usage: w
                    .resources
                    .gpu_metrics
                    .iter()
                    .map(|g| GpuUsageResponse {
                        gpu_id: g.gpu_id,
                        utilization_percent: g.utilization_percent,
                        memory_used_bytes: g.memory_used_bytes,
                        memory_total_bytes: g.memory_total_bytes,
                        temperature_celsius: g.temperature_celsius,
                    })
                    .collect(),
            }),
        }
    }

    /// Get workers for API response
    pub fn get_workers_for_api(&self) -> Vec<WorkerResponse> {
        self.workers
            .all_workers()
            .into_iter()
            .map(|w| self.worker_to_response(&w))
            .collect()
    }

    /// Get a single worker for API response
    pub fn get_worker_for_api(&self, worker_id: &str) -> Option<WorkerResponse> {
        self.workers
            .get(worker_id)
            .map(|w| self.worker_to_response(&w))
    }

    /// Deregister a worker from the HTTP API
    pub fn deregister_worker_by_id(&self, worker_id: &str) -> Result<(), String> {
        self.workers
            .deregister(worker_id)
            .map_err(|e| e.to_string())?;
        self.shard_manager.remove_worker(worker_id);
        self.shard_manager.rebalance_shards();
        self.pending_commands.remove(worker_id);
        self.event_log.push(
            "info",
            "worker_registry",
            format!("Worker '{}' deregistered via API", worker_id),
            None,
        );
        self.reevaluate_active_barriers();
        Ok(())
    }

    /// Register or update a dataset directly from HTTP API or gRPC
    #[allow(clippy::too_many_arguments)]
    pub fn register_dataset_entry(
        &self,
        dataset_id: String,
        name: String,
        path: String,
        format: String,
        total_samples: u64,
        shard_size: u64,
        shuffle: bool,
        seed: u64,
        metadata: HashMap<String, String>,
    ) -> Result<DatasetResponse, String> {
        self.input_validator
            .validate_dataset_id(&dataset_id)
            .map_err(|s| s.message().to_string())?;
        self.input_validator
            .validate_path(&path)
            .map_err(|s| s.message().to_string())?;
        self.input_validator
            .validate_metadata(&metadata)
            .map_err(|s| s.message().to_string())?;

        if shard_size == 0 {
            return Err("shard_size must be greater than 0".to_string());
        }

        let ds_meta = self
            .shard_manager
            .register_dataset_with_info(
                dataset_id.clone(),
                path.clone(),
                format.clone(),
                total_samples,
                shard_size,
                shuffle,
                seed,
                metadata.clone(),
            )
            .map_err(|e| e.to_string())?;

        let registered_at = Utc::now().timestamp_millis();
        let display_name = if name.trim().is_empty() {
            metadata
                .get("name")
                .cloned()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| dataset_id.clone())
        } else {
            name
        };

        let proto_info = DatasetInfo {
            dataset_id: dataset_id.clone(),
            path: path.clone(),
            format: format.clone(),
            total_samples: total_samples as i64,
            shard_size: shard_size as i64,
            shuffle,
            seed: seed as i64,
            metadata,
        };

        self.datasets.insert(
            dataset_id.clone(),
            TrackedDataset {
                info: proto_info,
                name: display_name.clone(),
                registered_at,
            },
        );

        self.event_log.push(
            "info",
            "shard_manager",
            format!(
                "Dataset '{}' registered ({} samples, {} shards, format={})",
                dataset_id, total_samples, ds_meta.total_shards, format
            ),
            None,
        );

        Ok(DatasetResponse {
            id: dataset_id,
            name: display_name,
            path,
            total_samples,
            shard_size,
            shard_count: ds_meta.total_shards,
            format,
            shuffle,
            registered_at,
        })
    }

    /// Delete a dataset by ID
    pub fn delete_dataset_by_id(&self, dataset_id: &str) -> bool {
        let removed_tracked = self.datasets.remove(dataset_id).is_some();
        let removed_shard = self.shard_manager.remove_dataset(dataset_id);
        if removed_tracked || removed_shard {
            self.event_log.push(
                "info",
                "shard_manager",
                format!("Dataset '{}' deleted", dataset_id),
                None,
            );
            true
        } else {
            false
        }
    }

    /// Get tracked dataset metadata by ID
    pub fn get_tracked_dataset(&self, dataset_id: &str) -> Option<TrackedDataset> {
        self.datasets.get(dataset_id).map(|e| e.value().clone())
    }

    /// Get datasets for API response
    pub fn get_datasets_for_api(&self) -> Vec<DatasetResponse> {
        self.datasets
            .iter()
            .map(|entry| {
                let tracked = entry.value();
                let d = &tracked.info;
                let shard_size = (d.shard_size as u64).max(1);
                let total_samples = d.total_samples.max(0) as u64;
                let shard_count = total_samples.div_ceil(shard_size);
                DatasetResponse {
                    id: d.dataset_id.clone(),
                    name: tracked.name.clone(),
                    path: d.path.clone(),
                    total_samples,
                    shard_size,
                    shard_count,
                    format: d.format.clone(),
                    shuffle: d.shuffle,
                    registered_at: tracked.registered_at,
                }
            })
            .collect()
    }

    /// Get checkpoints for API response
    pub fn get_checkpoints_for_api(&self) -> Vec<CheckpointResponse> {
        self.checkpoint_manager
            .all_checkpoints()
            .into_iter()
            .rev()
            .take(50)
            .map(|c| CheckpointResponse {
                id: c.id,
                step: c.step,
                epoch: c.epoch,
                size: c.size_bytes,
                path: c.path,
                created_at: c.created_at.timestamp_millis(),
                worker_id: c.metadata.get("worker_id").cloned().unwrap_or_default(),
                status: "completed".to_string(),
            })
            .collect()
    }

    /// Get a single checkpoint by ID for API response
    pub fn get_checkpoint_for_api(&self, checkpoint_id: &str) -> Option<CheckpointResponse> {
        self.checkpoint_manager
            .get_by_id(checkpoint_id)
            .map(|c| CheckpointResponse {
                id: c.id,
                step: c.step,
                epoch: c.epoch,
                size: c.size_bytes,
                path: c.path,
                created_at: c.created_at.timestamp_millis(),
                worker_id: c.metadata.get("worker_id").cloned().unwrap_or_default(),
                status: "completed".to_string(),
            })
    }

    /// Delete a checkpoint via CheckpointManager
    pub async fn delete_checkpoint_by_id(&self, checkpoint_id: &str) -> Result<bool, String> {
        let deleted = self
            .checkpoint_manager
            .delete(checkpoint_id)
            .await
            .map_err(|e| e.to_string())?;
        if deleted {
            self.event_log.push(
                "info",
                "checkpoint",
                format!("Checkpoint '{}' deleted", checkpoint_id),
                None,
            );
        }
        Ok(deleted)
    }

    /// Get barriers for API response (both active waiting and recently completed)
    pub fn get_barriers_for_api(&self) -> Vec<ApiBarrierResponse> {
        let mut barriers: Vec<ApiBarrierResponse> = self
            .barriers
            .iter()
            .map(|entry| {
                let id = entry.key().clone();
                let barrier = entry.value();
                let arrived = barrier.arrived_workers.lock().len() as u64;
                let expected = barrier.expected.load(Ordering::Acquire);
                let status = if arrived >= expected {
                    "complete"
                } else {
                    "waiting"
                };
                ApiBarrierResponse {
                    id: id.clone(),
                    name: id,
                    arrived,
                    total: expected,
                    status: status.to_string(),
                    created_at: barrier.created_at_ms,
                }
            })
            .collect();

        let completed = self.completed_barriers.read();
        barriers.extend(completed.iter().cloned());
        barriers.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        barriers
    }

    /// Get metrics for API response
    pub fn get_metrics_for_api(&self) -> MetricsResponse {
        let workers = self.workers.all_workers();
        let active_workers = workers.iter().filter(|w| w.state.is_active()).count() as u32;

        // Rolling coordinator RPS over last 10s window
        let coordinator_rps = {
            let now = Instant::now();
            let cutoff = now - Duration::from_secs(RPS_WINDOW_SECS);
            let mut recent = self.recent_requests.write();
            while recent.front().is_some_and(|t| *t < cutoff) {
                recent.pop_front();
            }
            let count = recent.len() as f64;
            if count == 0.0 {
                0
            } else {
                let oldest = *recent.front().unwrap();
                let elapsed = now.duration_since(oldest).as_secs_f64();
                let window = if elapsed < 1.0 {
                    1.0
                } else {
                    elapsed.min(RPS_WINDOW_SECS as f64)
                };
                (count / window).round() as u64
            }
        };

        // Rolling checkpoint throughput in MB/s over recent window
        let checkpoint_throughput = {
            let now = Instant::now();
            let cutoff = now - Duration::from_secs_f64(CHECKPOINT_WINDOW_SECS);
            let mut recent = self.recent_checkpoints.write();
            while recent.front().is_some_and(|(t, _)| *t < cutoff) {
                recent.pop_front();
            }
            if recent.is_empty() {
                0.0
            } else {
                let total_bytes: u64 = recent.iter().map(|(_, b)| *b).sum();
                let oldest = recent.front().unwrap().0;
                let window = now
                    .duration_since(oldest)
                    .as_secs_f64()
                    .clamp(1.0, CHECKPOINT_WINDOW_SECS);
                let mb_per_sec = (total_bytes as f64 / 1_048_576.0) / window;
                (mb_per_sec * 100.0).round() / 100.0
            }
        };

        // Real barrier p99 latency in ms (0 if no barriers completed yet)
        let barrier_latency_p99 = self
            .request_metrics
            .get_p99_latency("barrier_sync")
            .map(|us| us / 1000)
            .unwrap_or(0);

        // Real get_data_shard p99 latency in ms
        let shard_assignment_time = self
            .request_metrics
            .get_p99_latency("get_data_shard")
            .map(|us| us / 1000)
            .unwrap_or(0);

        MetricsResponse {
            checkpoint_throughput,
            coordinator_rps,
            active_workers,
            total_workers: workers.len() as u32,
            barrier_latency_p99,
            shard_assignment_time,
        }
    }

    /// Get rolling throughput history points
    pub fn get_throughput_history_for_api(&self) -> Vec<ThroughputPoint> {
        self.throughput_history.read().iter().cloned().collect()
    }

    /// Get all tasks for API response
    pub fn get_tasks_for_api(&self) -> Vec<TaskResponse> {
        let mut tasks: Vec<TaskResponse> = self
            .tasks
            .read()
            .values()
            .map(TrackedTask::to_response)
            .collect();
        tasks.sort_by_key(|t| std::cmp::Reverse(t.started_at));
        tasks
    }

    /// Create a new real training task
    pub fn create_task_entry(
        &self,
        name: String,
        task_type: String,
        dataset_id: String,
        worker_count: u32,
        config: HashMap<String, serde_json::Value>,
    ) -> Result<String, String> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err("Task name cannot be empty".to_string());
        }
        if !self.datasets.contains_key(&dataset_id) {
            return Err(format!("Dataset '{}' not found", dataset_id));
        }

        let total_epochs = config
            .get("epochs")
            .and_then(|v| v.as_u64())
            .filter(|&e| e > 0)
            .unwrap_or(10);

        // Select up to worker_count available workers
        let available_workers: Vec<String> = self
            .workers
            .all_workers()
            .into_iter()
            .filter(|w| w.state.is_active())
            .map(|w| w.id)
            .take(worker_count as usize)
            .collect();

        let now = Utc::now().timestamp_millis();
        let task_id = format!("task_{}", &uuid::Uuid::new_v4().to_string()[..8]);

        let status = if available_workers.is_empty() {
            "pending".to_string()
        } else {
            "running".to_string()
        };

        let mut logs = vec![format!(
            "[{}] Task '{}' created (dataset={}, requested_workers={}, epochs={})",
            Utc::now().format("%H:%M:%S"),
            trimmed_name,
            dataset_id,
            worker_count,
            total_epochs
        )];

        if available_workers.is_empty() {
            logs.push(format!(
                "[{}] Waiting for available workers (status=pending)",
                Utc::now().format("%H:%M:%S")
            ));
        } else {
            logs.push(format!(
                "[{}] Dispatched to workers: {}",
                Utc::now().format("%H:%M:%S"),
                available_workers.join(", ")
            ));
        }

        let tracked = TrackedTask {
            id: task_id.clone(),
            name: trimmed_name.to_string(),
            task_type,
            status: status.clone(),
            worker_ids: available_workers.clone(),
            dataset_id: dataset_id.clone(),
            started_at: now,
            completed_at: None,
            current_step: 0,
            current_epoch: 0,
            total_epochs,
            progress: 0,
            logs,
        };

        self.tasks.write().insert(task_id.clone(), tracked);

        for worker_id in &available_workers {
            self.enqueue_worker_command(
                worker_id,
                format!("START_TASK:{}:{}", task_id, dataset_id),
            );
        }

        self.event_log.push(
            "info",
            "task_manager",
            format!(
                "Task '{}' ({}) {} with {} assigned workers",
                trimmed_name,
                task_id,
                status,
                available_workers.len()
            ),
            Some(task_id.clone()),
        );

        Ok(task_id)
    }

    /// Stop a task and enqueue STOP_TASK commands to its assigned workers
    pub fn stop_task_by_id(&self, task_id: &str) -> Result<(), String> {
        let now = Utc::now().timestamp_millis();
        let worker_ids = {
            let mut tasks = self.tasks.write();
            let Some(task) = tasks.get_mut(task_id) else {
                return Err(format!("Task '{}' not found", task_id));
            };
            if task.status != "completed" {
                task.status = "stopped".to_string();
                task.completed_at = Some(now);
            }
            task.logs.push(format!(
                "[{}] Task stopped by user",
                Utc::now().format("%H:%M:%S")
            ));
            task.worker_ids.clone()
        };

        for worker_id in &worker_ids {
            self.enqueue_worker_command(worker_id, format!("STOP_TASK:{}", task_id));
        }

        self.event_log.push(
            "info",
            "task_manager",
            format!("Task '{}' stopped", task_id),
            Some(task_id.to_string()),
        );

        Ok(())
    }

    /// Get filtered logs from EventLog for HTTP API
    pub fn get_logs_for_api(
        &self,
        level: Option<&str>,
        source: Option<&str>,
        task_id: Option<&str>,
        limit: usize,
    ) -> Vec<LogResponse> {
        self.event_log
            .filtered(level, source, task_id, limit)
            .into_iter()
            .map(|e| LogResponse {
                id: e.id,
                timestamp: e.timestamp,
                level: e.level,
                message: e.message,
                source: e.source,
                task_id: e.task_id,
                worker_id: None,
            })
            .collect()
    }

    /// Get logs for a specific task (from EventLog plus task summary if needed)
    pub fn get_task_logs_for_api(&self, task_id: &str) -> Vec<LogResponse> {
        let mut logs = self.get_logs_for_api(None, None, Some(task_id), 200);
        if logs.is_empty() {
            if let Some(task) = self.tasks.read().get(task_id) {
                for (i, line) in task.logs.iter().enumerate() {
                    logs.push(LogResponse {
                        id: format!("tasklog_{}_{}", task_id, i),
                        timestamp: task.started_at + (i as i64 * 1000),
                        level: "info".to_string(),
                        message: line.clone(),
                        source: "task_manager".to_string(),
                        task_id: Some(task_id.to_string()),
                        worker_id: None,
                    });
                }
            }
        }
        logs
    }
}

#[tonic::async_trait]
impl Coordinator for CoordinatorService {
    /// Register a new worker with the coordinator
    async fn register_worker(
        &self,
        request: Request<WorkerInfo>,
    ) -> Result<Response<WorkerConfig>, Status> {
        let start = Instant::now();
        let info = request.into_inner();
        let client_id = if info.worker_id.is_empty() {
            "anonymous".to_string()
        } else {
            info.worker_id.clone()
        };

        let result: Result<Response<WorkerConfig>, Status> = async {
            self.check_rate_limit(&client_id)?;
            self.input_validator.validate_worker_id(&info.worker_id)?;
            self.input_validator.validate_port(info.port)?;
            self.input_validator.validate_metadata(&info.metadata)?;

            info!(
                worker_id = %info.worker_id,
                hostname = %info.hostname,
                port = info.port,
                gpu_count = info.gpu_count,
                "Worker registration request"
            );

            // Create core worker info
            let mut core_info = CoreWorkerInfo::new(
                info.worker_id.clone(),
                info.hostname.clone(),
                info.port as u16,
                0, // rank assigned by registry
                0, // world_size updated after registration
            );
            core_info.metadata = info.metadata.clone();

            // Register with worker registry
            let registered = self.workers.register(core_info).map_err(|e| {
                Status::already_exists(format!("Worker registration failed: {}", e))
            })?;

            // Preserve hardware specs (gpu_count and memory_bytes)
            self.workers
                .set_worker_hardware(
                    &info.worker_id,
                    info.gpu_count.max(0) as u32,
                    info.memory_bytes.max(0) as u64,
                )
                .map_err(|e| Status::internal(format!("Failed to set worker hardware: {}", e)))?;

            // Also register with shard manager for data distribution
            self.shard_manager.register_worker(&info.worker_id);

            self.event_log.push(
                "info",
                "worker_registry",
                format!(
                    "Worker '{}' registered ({}:{}, gpus={}, rank={})",
                    registered.id, info.hostname, info.port, info.gpu_count, registered.rank
                ),
                None,
            );

            // Assign any pending tasks waiting for workers
            self.dispatch_pending_tasks_to_worker(&registered.id);

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
        .await;

        self.record_request_outcome("register_worker", &client_id, start, result.is_ok());
        result
    }

    /// Process worker heartbeat
    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let start = Instant::now();
        let hb = request.into_inner();
        let client_id = hb.worker_id.clone();

        let result: Result<Response<HeartbeatResponse>, Status> = async {
            self.check_rate_limit(&client_id)?;

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

            self.shard_manager.heartbeat(&hb.worker_id);

            // Update progress if provided
            if let Some(status) = &hb.status {
                let step = status.current_step.max(0) as u64;
                let epoch = status.current_epoch.max(0) as u64;
                let _ = self.workers.update_progress(
                    &hb.worker_id,
                    step,
                    epoch,
                    Some(status.current_task.clone()),
                );
                self.update_task_from_heartbeat(&hb.worker_id, step, epoch, &status.current_task);
            }

            let pending_commands = self.drain_worker_commands(&hb.worker_id);
            debug!(worker_id = %hb.worker_id, commands = pending_commands.len(), "Heartbeat processed");

            Ok(Response::new(HeartbeatResponse {
                acknowledged: true,
                server_timestamp_ms: Utc::now().timestamp_millis(),
                pending_commands,
            }))
        }
        .await;

        self.record_request_outcome("heartbeat", &client_id, start, result.is_ok());
        result
    }

    /// Deregister a worker
    async fn deregister_worker(
        &self,
        request: Request<WorkerInfo>,
    ) -> Result<Response<WorkerConfig>, Status> {
        let start = Instant::now();
        let info = request.into_inner();
        let client_id = info.worker_id.clone();

        let result: Result<Response<WorkerConfig>, Status> = async {
            self.check_rate_limit(&client_id)?;
            info!(worker_id = %info.worker_id, "Worker deregistration request");

            // Remove from registries
            let removed = self
                .workers
                .deregister(&info.worker_id)
                .map_err(|e| Status::not_found(format!("Worker not found: {}", e)))?;

            self.shard_manager.remove_worker(&info.worker_id);
            self.shard_manager.rebalance_shards();
            self.pending_commands.remove(&info.worker_id);

            self.event_log.push(
                "info",
                "worker_registry",
                format!("Worker '{}' deregistered", removed.id),
                None,
            );
            self.reevaluate_active_barriers();

            Ok(Response::new(WorkerConfig {
                assigned_id: removed.id,
                rank: removed.rank as i32,
                world_size: self.workers.world_size() as i32,
                heartbeat_interval_ms: self.heartbeat_interval_ms as i64,
                config: HashMap::new(),
            }))
        }
        .await;

        self.record_request_outcome("deregister_worker", &client_id, start, result.is_ok());
        result
    }

    /// Register a dataset for sharding
    async fn register_dataset(
        &self,
        request: Request<DatasetInfo>,
    ) -> Result<Response<DatasetAck>, Status> {
        let start = Instant::now();
        let info = request.into_inner();
        let client_id = if info.dataset_id.is_empty() {
            "anonymous".to_string()
        } else {
            info.dataset_id.clone()
        };

        let result: Result<Response<DatasetAck>, Status> = async {
            self.check_rate_limit(&client_id)?;
            self.input_validator.validate_dataset_id(&info.dataset_id)?;
            self.input_validator.validate_path(&info.path)?;
            self.input_validator.validate_metadata(&info.metadata)?;
            self.input_validator
                .validate_positive(info.total_samples, "total_samples")?;
            if info.shard_size <= 0 {
                return Err(Status::invalid_argument(
                    "shard_size must be greater than 0",
                ));
            }

            info!(
                dataset_id = %info.dataset_id,
                total_samples = info.total_samples,
                shard_size = info.shard_size,
                "Dataset registration request"
            );

            let display_name = info
                .metadata
                .get("name")
                .cloned()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| info.dataset_id.clone());

            let registered = self
                .register_dataset_entry(
                    info.dataset_id.clone(),
                    display_name,
                    info.path,
                    info.format,
                    info.total_samples as u64,
                    info.shard_size as u64,
                    info.shuffle,
                    info.seed.max(0) as u64,
                    info.metadata,
                )
                .map_err(Status::invalid_argument)?;

            Ok(Response::new(DatasetAck {
                success: true,
                dataset_id: registered.id,
                total_shards: registered.shard_count as i64,
                message: format!("Dataset registered with {} shards", registered.shard_count),
            }))
        }
        .await;

        self.record_request_outcome("register_dataset", &client_id, start, result.is_ok());
        result
    }

    /// Get shard assignment for a worker
    async fn get_data_shard(
        &self,
        request: Request<ShardRequest>,
    ) -> Result<Response<ShardAssignment>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let client_id = req.worker_id.clone();

        let result: Result<Response<ShardAssignment>, Status> = async {
            self.check_rate_limit(&client_id)?;
            debug!(
                worker_id = %req.worker_id,
                dataset_id = %req.dataset_id,
                epoch = req.epoch,
                "Shard request"
            );

            // Get dataset info
            let tracked = self.datasets.get(&req.dataset_id).ok_or_else(|| {
                Status::not_found(format!("Dataset not found: {}", req.dataset_id))
            })?;
            let dataset_info = &tracked.info;

            // Get shard assignments from manager
            let shards = self
                .shard_manager
                .get_shard_for_worker(&req.dataset_id, &req.worker_id, req.epoch.max(0) as u64)
                .ok_or_else(|| {
                    Status::internal(format!(
                        "Failed to get shards for worker {} on dataset {}",
                        req.worker_id, req.dataset_id
                    ))
                })?;

            // Return first shard (primary assignment)
            if let Some(shard) = shards.first() {
                let shard_size = dataset_info.shard_size.max(1) as u64;
                let total_samples = dataset_info.total_samples.max(0) as u64;
                let total_shards = total_samples.div_ceil(shard_size) as i64;

                let file_paths = if !shard.file_paths.is_empty() {
                    shard.file_paths.clone()
                } else if !dataset_info.path.is_empty() {
                    vec![dataset_info.path.clone()]
                } else {
                    Vec::new()
                };

                Ok(Response::new(ShardAssignment {
                    dataset_id: req.dataset_id,
                    shard_id: shard.shard_id as i64,
                    total_shards,
                    start_index: shard.start_index as i64,
                    end_index: shard.end_index as i64,
                    file_paths,
                    epoch: req.epoch,
                }))
            } else {
                Err(Status::not_found("No shards available for this worker"))
            }
        }
        .await;

        self.record_request_outcome("get_data_shard", &client_id, start, result.is_ok());
        result
    }

    /// Notify coordinator of a completed checkpoint
    async fn notify_checkpoint(
        &self,
        request: Request<CheckpointInfo>,
    ) -> Result<Response<CheckpointAck>, Status> {
        let start = Instant::now();
        let info = request.into_inner();
        let client_id = if info.worker_id.is_empty() {
            "anonymous".to_string()
        } else {
            info.worker_id.clone()
        };

        let result: Result<Response<CheckpointAck>, Status> = async {
            self.check_rate_limit(&client_id)?;
            self.input_validator.validate_worker_id(&info.worker_id)?;
            if info.checkpoint_id.is_empty() {
                return Err(Status::invalid_argument("checkpoint_id cannot be empty"));
            }
            self.input_validator.validate_path(&info.storage_path)?;
            self.input_validator.validate_metadata(&info.metadata)?;
            self.input_validator.validate_positive(info.step, "step")?;
            self.input_validator
                .validate_positive(info.size_bytes, "size_bytes")?;

            info!(
                worker_id = %info.worker_id,
                checkpoint_id = %info.checkpoint_id,
                step = info.step,
                epoch = info.epoch,
                size_bytes = info.size_bytes,
                "Checkpoint notification"
            );

            let mut metadata = info.metadata.clone();
            metadata.insert("worker_id".to_string(), info.worker_id.clone());

            self.checkpoint_manager.register_external_checkpoint(
                &info.checkpoint_id,
                info.step as u64,
                info.epoch.max(0) as u64,
                &info.storage_path,
                info.size_bytes as u64,
                metadata,
            );

            // Record recent checkpoint bytes for rolling throughput
            {
                let mut recent = self.recent_checkpoints.write();
                if recent.len() >= MAX_RECENT_CHECKPOINTS {
                    recent.pop_front();
                }
                recent.push_back((Instant::now(), info.size_bytes as u64));
            }

            self.event_log.push(
                "info",
                "checkpoint",
                format!(
                    "Checkpoint '{}' saved by '{}' (step={}, epoch={}, {} bytes)",
                    info.checkpoint_id, info.worker_id, info.step, info.epoch, info.size_bytes
                ),
                None,
            );

            Ok(Response::new(CheckpointAck {
                success: true,
                checkpoint_id: info.checkpoint_id,
                message: "Checkpoint acknowledged".to_string(),
                global_step: info.step,
            }))
        }
        .await;

        self.record_request_outcome("notify_checkpoint", &client_id, start, result.is_ok());
        result
    }

    /// Get latest checkpoint for recovery
    async fn get_latest_checkpoint(
        &self,
        request: Request<RecoveryRequest>,
    ) -> Result<Response<RecoveryResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let client_id = req.worker_id.clone();

        let result: Result<Response<RecoveryResponse>, Status> = async {
            self.check_rate_limit(&client_id)?;
            info!(
                worker_id = %req.worker_id,
                job_id = %req.job_id,
                "Recovery request"
            );

            let latest = self.checkpoint_manager.find_recovery_checkpoint();

            if let Some(ckpt) = latest {
                let mut shard_assignments = Vec::new();
                for entry in self.datasets.iter() {
                    let dataset_info = &entry.value().info;
                    if let Some(shards) = self.shard_manager.get_shard_for_worker(
                        &dataset_info.dataset_id,
                        &req.worker_id,
                        ckpt.epoch,
                    ) {
                        for shard in shards {
                            let shard_size = dataset_info.shard_size.max(1) as u64;
                            let total_samples = dataset_info.total_samples.max(0) as u64;
                            let total_shards = total_samples.div_ceil(shard_size) as i64;
                            let file_paths = if !shard.file_paths.is_empty() {
                                shard.file_paths
                            } else if !dataset_info.path.is_empty() {
                                vec![dataset_info.path.clone()]
                            } else {
                                Vec::new()
                            };

                            shard_assignments.push(ShardAssignment {
                                dataset_id: dataset_info.dataset_id.clone(),
                                shard_id: shard.shard_id as i64,
                                total_shards,
                                start_index: shard.start_index as i64,
                                end_index: shard.end_index as i64,
                                file_paths,
                                epoch: ckpt.epoch as i64,
                            });
                        }
                    }
                }

                Ok(Response::new(RecoveryResponse {
                    has_checkpoint: true,
                    latest_checkpoint: Some(proto::CheckpointInfo {
                        worker_id: String::new(),
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
        .await;

        self.record_request_outcome("get_latest_checkpoint", &client_id, start, result.is_ok());
        result
    }

    /// Barrier synchronization
    async fn wait_barrier(
        &self,
        request: Request<BarrierRequest>,
    ) -> Result<Response<BarrierResponse>, Status> {
        let start = Instant::now();
        let req = request.into_inner();
        let client_id = req.worker_id.clone();

        let result: Result<Response<BarrierResponse>, Status> = async {
            self.check_rate_limit(&client_id)?;
            let active_count = self.workers.active_workers().len() as u64;
            let world_size = self.workers.world_size() as u64;
            let expected = if active_count > 0 {
                active_count
            } else if world_size > 0 {
                world_size
            } else {
                // Default to 2 when unregistered test clients synchronize
                2
            };

            info!(
                worker_id = %req.worker_id,
                barrier_id = %req.barrier_id,
                step = req.step,
                expected = expected,
                "Barrier wait request"
            );

            // Get or create barrier state
            let barrier_ref = {
                if let Some(existing) = self.barriers.get(&req.barrier_id) {
                    existing.clone()
                } else {
                    let mut created_new = false;
                    let entry = self
                        .barriers
                        .entry(req.barrier_id.clone())
                        .or_insert_with(|| {
                            created_new = true;
                            Arc::new(BarrierState {
                                expected: AtomicU64::new(expected),
                                arrived_workers: parking_lot::Mutex::new(HashSet::new()),
                                waiters: parking_lot::Mutex::new(Vec::new()),
                                created_at_instant: Instant::now(),
                                created_at_ms: Utc::now().timestamp_millis(),
                            })
                        });
                    let cloned = entry.value().clone();
                    drop(entry);
                    if created_new {
                        self.event_log.push(
                            "info",
                            "barrier",
                            format!(
                                "Barrier '{}' created (expected={} workers, step={})",
                                req.barrier_id, expected, req.step
                            ),
                            None,
                        );
                    }
                    cloned
                }
            };

            // Deduplicate worker_id arrivals and optionally register waiter atomically
            enum ArrivalAction {
                Release(u64),
                Wait(u64, tokio::sync::oneshot::Receiver<u64>),
            }

            let action = {
                let mut arrived_guard = barrier_ref.arrived_workers.lock();
                arrived_guard.insert(req.worker_id.clone());
                let arrival_count = arrived_guard.len() as u64;
                let expected_count = barrier_ref.expected.load(Ordering::Acquire);

                info!(
                    barrier_id = %req.barrier_id,
                    worker_id = %req.worker_id,
                    arrival_order = arrival_count,
                    expected = expected_count,
                    "Worker arrived at barrier"
                );

                if arrival_count >= expected_count {
                    ArrivalAction::Release(arrival_count)
                } else {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    barrier_ref.waiters.lock().push(tx);
                    ArrivalAction::Wait(arrival_count, rx)
                }
            };

            match action {
                ArrivalAction::Release(arrival_order) => {
                    self.release_barrier(&req.barrier_id, &barrier_ref, arrival_order);
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
                }
                ArrivalAction::Wait(arrival_order, rx) => {
                    match tokio::time::timeout(Duration::from_secs(300), rx).await {
                        Ok(Ok(participants)) => Ok(Response::new(BarrierResponse {
                            released: true,
                            barrier_id: req.barrier_id,
                            participants: participants as i64,
                            arrival_order: arrival_order as i64,
                        })),
                        Ok(Err(_)) => Err(Status::internal("Barrier channel closed")),
                        Err(_) => {
                            warn!(barrier_id = %req.barrier_id, "Barrier wait timed out");
                            Err(Status::deadline_exceeded("Barrier timeout"))
                        }
                    }
                }
            }
        }
        .await;

        self.record_request_outcome("wait_barrier", &client_id, start, result.is_ok());
        result
    }

    /// Streaming heartbeats for efficient real-time updates
    type StreamHeartbeatsStream =
        Pin<Box<dyn Stream<Item = Result<HeartbeatResponse, Status>> + Send>>;

    async fn stream_heartbeats(
        &self,
        request: Request<Streaming<HeartbeatRequest>>,
    ) -> Result<Response<Self::StreamHeartbeatsStream>, Status> {
        let mut stream = request.into_inner();
        let svc = self.clone();

        // Create response channel
        let (tx, rx) = mpsc::channel(32);

        // Spawn task to process incoming heartbeats
        tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                let msg_start = Instant::now();
                match result {
                    Ok(hb) => {
                        let client_id = hb.worker_id.clone();
                        let state = hb
                            .status
                            .as_ref()
                            .map(|s| CoordinatorService::proto_to_core_state(s.state))
                            .unwrap_or(CoreWorkerState::Idle);

                        let resources = CoordinatorService::proto_to_core_resources(hb.resources);

                        let ok = match svc.workers.heartbeat(&hb.worker_id, state, resources) {
                            Ok(()) => {
                                svc.shard_manager.heartbeat(&hb.worker_id);
                                if let Some(status) = &hb.status {
                                    let step = status.current_step.max(0) as u64;
                                    let epoch = status.current_epoch.max(0) as u64;
                                    let _ = svc.workers.update_progress(
                                        &hb.worker_id,
                                        step,
                                        epoch,
                                        Some(status.current_task.clone()),
                                    );
                                    svc.update_task_from_heartbeat(
                                        &hb.worker_id,
                                        step,
                                        epoch,
                                        &status.current_task,
                                    );
                                }
                                true
                            }
                            Err(e) => {
                                error!(worker_id = %hb.worker_id, error = %e, "Failed to process heartbeat");
                                false
                            }
                        };

                        svc.record_request_outcome("stream_heartbeats", &client_id, msg_start, ok);

                        let pending_commands = svc.drain_worker_commands(&hb.worker_id);
                        let response = HeartbeatResponse {
                            acknowledged: ok,
                            server_timestamp_ms: Utc::now().timestamp_millis(),
                            pending_commands,
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

        assert_eq!(service.workers.world_size(), 0);
        assert_eq!(service.get_throughput_history_for_api().len(), 1);
    }

    #[tokio::test]
    async fn test_worker_registration_preserves_hardware() {
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
            gpu_count: 4,
            memory_bytes: 32 * 1024 * 1024 * 1024,
            metadata: HashMap::new(),
        });

        let response = service.register_worker(request).await.unwrap();
        let cfg = response.into_inner();

        assert_eq!(cfg.assigned_id, "worker-1");
        assert_eq!(cfg.rank, 0);
        assert_eq!(cfg.world_size, 1);

        let stored = service.workers.get("worker-1").unwrap();
        assert_eq!(stored.gpu_count, 4);
        assert_eq!(stored.memory_bytes, 32 * 1024 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_dataset_registration_and_shards() {
        let dir = tempdir().unwrap();
        let data_file = dir.path().join("train.jsonl");
        std::fs::write(&data_file, "{\"x\":1,\"label\":0}\n{\"x\":2,\"label\":1}\n").unwrap();

        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };

        let service = CoordinatorService::with_config(config, 100, Duration::from_secs(30))
            .await
            .unwrap();

        let worker_req = Request::new(WorkerInfo {
            worker_id: "worker-1".to_string(),
            hostname: "localhost".to_string(),
            port: 50052,
            gpu_count: 1,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            metadata: HashMap::new(),
        });
        service.register_worker(worker_req).await.unwrap();

        let mut meta = HashMap::new();
        meta.insert("name".to_string(), "ImageNet Subset".to_string());
        let dataset_req = Request::new(DatasetInfo {
            dataset_id: "imagenet".to_string(),
            path: data_file.to_string_lossy().to_string(),
            format: "jsonl".to_string(),
            total_samples: 100,
            shard_size: 25,
            shuffle: false,
            seed: 42,
            metadata: meta,
        });

        let response = service.register_dataset(dataset_req).await.unwrap();
        let ack = response.into_inner();
        assert!(ack.success);
        assert_eq!(ack.dataset_id, "imagenet");
        assert_eq!(ack.total_shards, 4);

        let shard_resp = service
            .get_data_shard(Request::new(ShardRequest {
                worker_id: "worker-1".to_string(),
                dataset_id: "imagenet".to_string(),
                epoch: 0,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            shard_resp.file_paths,
            vec![data_file.to_string_lossy().to_string()]
        );

        let api_workers = service.get_workers_for_api();
        assert_eq!(api_workers[0].assigned_shards, 4);
        assert_eq!(api_workers[0].status, "idle");
    }

    #[tokio::test]
    async fn test_barrier_dedup_and_completion_history() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let service = Arc::new(
            CoordinatorService::with_config(config, 100, Duration::from_secs(30))
                .await
                .unwrap(),
        );

        // Unregistered clients (world_size == 0) should default expected = 2 and deduplicate worker_id
        let svc1 = Arc::clone(&service);
        let h1 = tokio::spawn(async move {
            svc1.wait_barrier(Request::new(BarrierRequest {
                worker_id: "w1".to_string(),
                barrier_id: "b-dedup".to_string(),
                step: 1,
            }))
            .await
        });

        // Wait briefly so w1 arrives first, then send duplicate w1 (should NOT release barrier yet)
        tokio::time::sleep(Duration::from_millis(30)).await;
        let svc_dup = Arc::clone(&service);
        let h_dup = tokio::spawn(async move {
            svc_dup
                .wait_barrier(Request::new(BarrierRequest {
                    worker_id: "w1".to_string(),
                    barrier_id: "b-dedup".to_string(),
                    step: 1,
                }))
                .await
        });

        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(service.barriers.len(), 1);

        // Now w2 arrives -> barrier releases for all waiters
        let r2 = service
            .wait_barrier(Request::new(BarrierRequest {
                worker_id: "w2".to_string(),
                barrier_id: "b-dedup".to_string(),
                step: 1,
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(r2.released);
        assert_eq!(r2.participants, 2);

        assert!(h1.await.unwrap().unwrap().into_inner().released);
        assert!(h_dup.await.unwrap().unwrap().into_inner().released);

        let barriers = service.get_barriers_for_api();
        assert_eq!(barriers.len(), 1);
        assert_eq!(barriers[0].status, "complete");
        assert_eq!(barriers[0].arrived, 2);
    }

    #[tokio::test]
    async fn test_task_lifecycle_and_heartbeat_progress() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let service = CoordinatorService::with_config(config, 100, Duration::from_secs(30))
            .await
            .unwrap();

        service
            .register_worker(Request::new(WorkerInfo {
                worker_id: "w1".to_string(),
                hostname: "127.0.0.1".to_string(),
                port: 50052,
                gpu_count: 1,
                memory_bytes: 1024,
                metadata: HashMap::new(),
            }))
            .await
            .unwrap();

        service
            .register_dataset_entry(
                "ds1".to_string(),
                "DS 1".to_string(),
                "/tmp/ds1".to_string(),
                "jsonl".to_string(),
                100,
                50,
                false,
                42,
                HashMap::new(),
            )
            .unwrap();

        let mut task_cfg = HashMap::new();
        task_cfg.insert("epochs".to_string(), serde_json::json!(4));
        let task_id = service
            .create_task_entry(
                "train-job".to_string(),
                "custom".to_string(),
                "ds1".to_string(),
                1,
                task_cfg,
            )
            .unwrap();

        // Worker heartbeat delivers START_TASK command and reports epoch 2 -> 50%
        let hb_resp = service
            .heartbeat(Request::new(HeartbeatRequest {
                worker_id: "w1".to_string(),
                timestamp_ms: 0,
                status: Some(proto::WorkerStatus {
                    state: proto::worker_status::State::Training as i32,
                    current_step: 20,
                    current_epoch: 2,
                    current_task: task_id.clone(),
                }),
                resources: None,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            hb_resp.pending_commands,
            vec![format!("START_TASK:{}:ds1", task_id)]
        );

        let tasks = service.get_tasks_for_api();
        assert_eq!(tasks[0].status, "running");
        assert_eq!(tasks[0].progress, 50);

        // Stop task -> STOP_TASK queued on next heartbeat
        service.stop_task_by_id(&task_id).unwrap();
        let hb_resp2 = service
            .heartbeat(Request::new(HeartbeatRequest {
                worker_id: "w1".to_string(),
                timestamp_ms: 1,
                status: None,
                resources: None,
            }))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(
            hb_resp2.pending_commands,
            vec![format!("STOP_TASK:{}", task_id)]
        );
        assert_eq!(service.get_tasks_for_api()[0].status, "stopped");
    }

    #[tokio::test]
    async fn test_checkpoint_metrics_and_dead_worker_reaper() {
        let dir = tempdir().unwrap();
        let config = CheckpointManagerConfig {
            base_path: dir.path().to_path_buf(),
            ..Default::default()
        };
        let service = CoordinatorService::with_config(config, 100, Duration::from_millis(20))
            .await
            .unwrap();

        service
            .register_worker(Request::new(WorkerInfo {
                worker_id: "w-dead".to_string(),
                hostname: "127.0.0.1".to_string(),
                port: 50052,
                gpu_count: 1,
                memory_bytes: 1024,
                metadata: HashMap::new(),
            }))
            .await
            .unwrap();

        service
            .notify_checkpoint(Request::new(CheckpointInfo {
                worker_id: "w-dead".to_string(),
                checkpoint_id: "ckpt-1".to_string(),
                step: 10,
                epoch: 1,
                storage_path: "/tmp/ckpt-1".to_string(),
                size_bytes: 2 * 1024 * 1024,
                timestamp_ms: 0,
                r#type: proto::CheckpointType::Full as i32,
                metadata: HashMap::new(),
            }))
            .await
            .unwrap();

        let metrics = service.get_metrics_for_api();
        assert!(metrics.checkpoint_throughput > 0.0);
        assert!(metrics.coordinator_rps > 0);

        tokio::time::sleep(Duration::from_millis(40)).await;
        let dead = service.reap_dead_workers();
        assert_eq!(dead, vec!["w-dead".to_string()]);
        assert_eq!(service.get_workers_for_api()[0].status, "failed");
    }
}
