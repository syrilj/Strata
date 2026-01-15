//! HTTP API for dashboard integration
//!
//! Provides REST endpoints for the dashboard to query coordinator state.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tower_http::cors::{Any, CorsLayer};
use dashmap::DashMap;
use std::sync::OnceLock;

use crate::service::CoordinatorService;

// Global state for stopped tasks and user-created tasks
static STOPPED_TASKS: OnceLock<DashMap<String, i64>> = OnceLock::new();
static TASK_STORE: OnceLock<DashMap<String, TaskResponse>> = OnceLock::new();

fn get_stopped_tasks() -> &'static DashMap<String, i64> {
    STOPPED_TASKS.get_or_init(DashMap::new)
}

fn get_task_store() -> &'static DashMap<String, TaskResponse> {
    TASK_STORE.get_or_init(DashMap::new)
}

/// Shared state for HTTP handlers (Arc for thread-safe sharing)
pub type AppState = Arc<CoordinatorService>;

/// Worker info for API response
#[derive(Serialize)]
pub struct WorkerResponse {
    pub id: String,
    pub ip: String,
    pub port: u16,
    pub status: String,
    pub gpu_count: u32,
    pub last_heartbeat: i64,
    pub assigned_shards: u32,
    pub current_epoch: u64,
    pub current_step: u64,
    pub current_task: String,
}

/// Dataset info for API response
#[derive(Serialize)]
pub struct DatasetResponse {
    pub id: String,
    pub name: String,
    pub total_samples: u64,
    pub shard_size: u64,
    pub shard_count: u64,
    pub format: String,
    pub shuffle: bool,
    pub registered_at: i64,
}

/// Checkpoint info for API response
#[derive(Serialize)]
pub struct CheckpointResponse {
    pub id: String,
    pub step: u64,
    pub epoch: u64,
    pub size: u64,
    pub path: String,
    pub created_at: i64,
    pub worker_id: String,
    pub status: String,
}

/// Barrier status for API response
#[derive(Serialize)]
pub struct BarrierResponse {
    pub id: String,
    pub name: String,
    pub arrived: u64,
    pub total: u64,
    pub status: String,
    pub created_at: i64,
}

/// System metrics for API response
#[derive(Serialize)]
pub struct MetricsResponse {
    pub checkpoint_throughput: u64,
    pub coordinator_rps: u64,
    pub active_workers: u32,
    pub total_workers: u32,
    pub barrier_latency_p99: u64,
    pub shard_assignment_time: u64,
}

/// Task info for API response
#[derive(Serialize, Clone)]
pub struct TaskResponse {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub status: String,
    pub worker_ids: Vec<String>,
    pub dataset_id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub progress: u32,
    pub logs: Vec<String>,
}

/// Log entry for API response
#[derive(Serialize)]
pub struct LogResponse {
    pub id: String,
    pub timestamp: i64,
    pub level: String,
    pub message: String,
    pub source: String,
    pub task_id: Option<String>,
    pub worker_id: Option<String>,
}

/// Task creation request
#[derive(serde::Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub r#type: String,
    pub dataset_id: String,
    pub worker_count: u32,
    pub config: std::collections::HashMap<String, serde_json::Value>,
}

/// Task creation response
#[derive(Serialize)]
pub struct CreateTaskResponse {
    pub task_id: String,
}

/// Task stop response
#[derive(Serialize)]
pub struct StopTaskResponse {
    pub success: bool,
}

/// Coordinator status for API response
#[derive(Serialize)]
pub struct StatusResponse {
    pub connected: bool,
    pub address: String,
    pub uptime: u64,
    pub version: String,
}

/// Full dashboard state response
#[derive(Serialize)]
pub struct DashboardState {
    pub coordinator: StatusResponse,
    pub workers: Vec<WorkerResponse>,
    pub datasets: Vec<DatasetResponse>,
    pub checkpoints: Vec<CheckpointResponse>,
    pub barriers: Vec<BarrierResponse>,
    pub metrics: MetricsResponse,
    pub tasks: Vec<TaskResponse>,
    pub logs: Vec<LogResponse>,
}

/// Create the HTTP API router
pub fn create_router(service: Arc<CoordinatorService>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/health", get(health_check))
        .route("/api/status", get(get_status))
        .route("/api/workers", get(get_workers))
        .route("/api/datasets", get(get_datasets))
        .route("/api/checkpoints", get(get_checkpoints))
        .route("/api/barriers", get(get_barriers))
        .route("/api/metrics", get(get_metrics))
        .route("/api/dashboard", get(get_dashboard_state))
        .route("/api/tasks", get(get_tasks))
        .route("/api/tasks", post(create_task))
        .route("/api/tasks/:task_id/stop", post(stop_task))
        .route("/api/tasks/:task_id/logs", get(get_task_logs))
        .route("/api/logs", get(get_logs))
        .layer(cors)
        .with_state(service)
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

/// Get coordinator status
async fn get_status(State(service): State<AppState>) -> impl IntoResponse {
    let status = StatusResponse {
        connected: true,
        address: "localhost:50051".to_string(),
        uptime: service.uptime_secs(),
        version: "0.1.0".to_string(),
    };
    Json(status)
}

/// Get all workers
async fn get_workers(State(service): State<AppState>) -> impl IntoResponse {
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let demo_state = get_demo_dashboard_state(service.uptime_secs());
        return Json(demo_state.workers);
    }
    let workers = service.get_workers_for_api();
    Json(workers)
}

/// Get all datasets
async fn get_datasets(State(service): State<AppState>) -> impl IntoResponse {
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let demo_state = get_demo_dashboard_state(service.uptime_secs());
        return Json(demo_state.datasets);
    }
    let datasets = service.get_datasets_for_api();
    Json(datasets)
}

/// Get recent checkpoints
async fn get_checkpoints(State(service): State<AppState>) -> impl IntoResponse {
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let demo_state = get_demo_dashboard_state(service.uptime_secs());
        return Json(demo_state.checkpoints);
    }
    let checkpoints = service.get_checkpoints_for_api();
    Json(checkpoints)
}

/// Get barrier status
async fn get_barriers(State(service): State<AppState>) -> impl IntoResponse {
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let demo_state = get_demo_dashboard_state(service.uptime_secs());
        return Json(demo_state.barriers);
    }
    let barriers = service.get_barriers_for_api();
    Json(barriers)
}

/// Get system metrics
async fn get_metrics(State(service): State<AppState>) -> impl IntoResponse {
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let demo_state = get_demo_dashboard_state(service.uptime_secs());
        return Json(demo_state.metrics);
    }
    let metrics = service.get_metrics_for_api();
    Json(metrics)
}

/// Get full dashboard state in one request
async fn get_dashboard_state(State(service): State<AppState>) -> impl IntoResponse {
    // Check if we should use demo data
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        return Json(get_demo_dashboard_state(service.uptime_secs()));
    }
    
    let uptime = service.uptime_secs();
    let state = DashboardState {
        coordinator: StatusResponse {
            connected: true,
            address: "localhost:50051".to_string(),
            uptime,
            version: "0.1.0".to_string(),
        },
        workers: service.get_workers_for_api(),
        datasets: service.get_datasets_for_api(),
        checkpoints: service.get_checkpoints_for_api(),
        barriers: service.get_barriers_for_api(),
        metrics: service.get_metrics_for_api(),
        tasks: get_demo_tasks(uptime),
        logs: get_demo_logs(uptime),
    };
    Json(state)
}

/// Get all tasks
async fn get_tasks(State(service): State<AppState>) -> impl IntoResponse {
    let mut all_tasks = Vec::new();
    
    // Add user-created tasks
    for entry in get_task_store().iter() {
        all_tasks.push(entry.value().clone());
    }
    
    // In demo mode, also add demo tasks
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let demo_tasks = get_demo_tasks(service.uptime_secs());
        for demo_task in demo_tasks {
            // Skip if already exists (user-created task with same ID)
            if !all_tasks.iter().any(|t| t.id == demo_task.id) {
                all_tasks.push(demo_task);
            }
        }
    }
    
    Json(all_tasks)
}

/// Create a new task
async fn create_task(
    State(_service): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    
    let task_id = format!("task_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    
    tracing::info!(
        task_id = %task_id,
        name = %request.name,
        task_type = %request.r#type,
        dataset_id = %request.dataset_id,
        worker_count = request.worker_count,
        "Creating new task"
    );
    
    // Store the task
    let task = TaskResponse {
        id: task_id.clone(),
        name: request.name.clone(),
        r#type: request.r#type,
        status: "running".to_string(),
        worker_ids: (0..request.worker_count)
            .map(|i| format!("worker-{}", i))
            .collect(),
        dataset_id: request.dataset_id,
        started_at: now,
        completed_at: None,
        progress: 0,
        logs: vec![
            format!("[{}] Task '{}' started", chrono::Utc::now().format("%H:%M:%S"), request.name),
            format!("[{}] Allocated {} workers", chrono::Utc::now().format("%H:%M:%S"), request.worker_count),
            format!("[{}] Loading dataset...", chrono::Utc::now().format("%H:%M:%S")),
        ],
    };
    
    get_task_store().insert(task_id.clone(), task);
    
    Json(CreateTaskResponse { task_id })
}

/// Stop a task
async fn stop_task(
    State(_service): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    tracing::info!(task_id = %task_id, "Stopping task");
    
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    
    // Mark as stopped
    get_stopped_tasks().insert(task_id.clone(), now);
    
    // Update task in store if it exists
    if let Some(mut task) = get_task_store().get_mut(&task_id) {
        task.status = "completed".to_string();
        task.completed_at = Some(now);
        task.logs.push(format!("[{}] Task stopped by user", chrono::Utc::now().format("%H:%M:%S")));
        tracing::info!(task_id = %task_id, "Task stopped successfully");
    } else {
        tracing::info!(task_id = %task_id, "Task marked as stopped");
    }
    
    Json(StopTaskResponse { success: true })
}

/// Get logs for a specific task
async fn get_task_logs(
    State(service): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    // Check user-created tasks first
    if let Some(task) = get_task_store().get(&task_id) {
        let logs = task.logs.iter().enumerate().map(|(i, log)| {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
            LogResponse {
                id: format!("log_{}_{}", task_id, i),
                timestamp: now - ((task.logs.len() - i) * 5000) as i64,
                level: "info".to_string(),
                message: log.clone(),
                source: "task_manager".to_string(),
                task_id: Some(task_id.clone()),
                worker_id: None,
            }
        }).collect();
        return Json(logs);
    }
    
    // Fall back to demo logs
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let logs = get_demo_logs(service.uptime_secs())
            .into_iter()
            .filter(|log| log.task_id.as_ref() == Some(&task_id))
            .collect::<Vec<_>>();
        return Json(logs);
    }
    Json(Vec::<LogResponse>::new())
}

/// Get system logs
async fn get_logs(
    State(service): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);
        
    if std::env::var("DEMO_MODE").unwrap_or_default() == "true" {
        let mut logs = get_demo_logs(service.uptime_secs());
        logs.truncate(limit);
        return Json(logs);
    }
    Json(Vec::<LogResponse>::new())
}

/// Generate demo dashboard state with active training simulation
fn get_demo_dashboard_state(uptime: u64) -> DashboardState {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    
    // Simulate training progress based on uptime
    let base_step = uptime / 3; // Progress every 3 seconds
    let current_epoch = (base_step / 500) + 1; // New epoch every 500 steps
    let current_step = base_step % 500;
    
    // Simulate realistic loss decay
    let _loss = 0.8 * (-0.001 * base_step as f64).exp() + 0.15;
    let _accuracy = 0.95 * (1.0 - (-0.002 * base_step as f64).exp());
    
    let workers = vec![
        WorkerResponse {
            id: "gpu-worker-01".to_string(),
            ip: "gpu-node-01".to_string(),
            port: 50052,
            status: "active".to_string(),
            gpu_count: 8,
            last_heartbeat: now,
            assigned_shards: 12,
            current_epoch,
            current_step,
            current_task: match base_step % 4 {
                0 => "forward_pass".to_string(),
                1 => "backward_pass".to_string(),
                2 => "gradient_sync".to_string(),
                _ => "parameter_update".to_string(),
            },
        },
        WorkerResponse {
            id: "gpu-worker-02".to_string(),
            ip: "gpu-node-02".to_string(),
            port: 50052,
            status: "active".to_string(),
            gpu_count: 8,
            last_heartbeat: now,
            assigned_shards: 12,
            current_epoch,
            current_step: current_step.saturating_sub(2),
            current_task: "backward_pass".to_string(),
        },
        WorkerResponse {
            id: "cpu-worker-01".to_string(),
            ip: "cpu-node-01".to_string(),
            port: 50052,
            status: if uptime % 30 < 5 { "idle".to_string() } else { "active".to_string() },
            gpu_count: 0,
            last_heartbeat: now,
            assigned_shards: 8,
            current_epoch,
            current_step: 0,
            current_task: "data_preprocessing".to_string(),
        },
    ];
    
    let datasets = vec![
        DatasetResponse {
            id: "imagenet-train".to_string(),
            name: "ImageNet Training Set".to_string(),
            total_samples: 1281167,
            shard_size: 10000,
            shard_count: 128,
            format: "tfrecord".to_string(),
            shuffle: true,
            registered_at: now - 3600000, // 1 hour ago
        },
        DatasetResponse {
            id: "custom-vision".to_string(),
            name: "Custom Vision Model".to_string(),
            total_samples: 500000,
            shard_size: 8000,
            shard_count: 64,
            format: "parquet".to_string(),
            shuffle: true,
            registered_at: now - 1800000, // 30 minutes ago
        },
    ];
    
    // Generate checkpoints based on completed epochs
    let mut checkpoints = Vec::new();
    for epoch in 1..current_epoch {
        checkpoints.push(CheckpointResponse {
            id: format!("checkpoint_epoch_{}", epoch),
            step: 500,
            epoch,
            size: 650 * 1024 * 1024, // 650MB
            path: format!("/checkpoints/epoch_{}.pt", epoch),
            created_at: now - ((current_epoch - epoch) * 1500000) as i64, // Spaced out
            worker_id: "gpu-worker-01".to_string(),
            status: "completed".to_string(),
        });
    }
    
    // Add current checkpoint if we're far enough in the epoch
    if current_step > 400 {
        checkpoints.push(CheckpointResponse {
            id: format!("checkpoint_epoch_{}_step_{}", current_epoch, current_step),
            step: current_step,
            epoch: current_epoch,
            size: 650 * 1024 * 1024,
            path: format!("/checkpoints/epoch_{}_step_{}.pt", current_epoch, current_step),
            created_at: now - 30000, // 30 seconds ago
            worker_id: "gpu-worker-01".to_string(),
            status: "completed".to_string(),
        });
    }
    
    // Generate barriers occasionally
    let barriers = if uptime % 60 < 10 {
        vec![BarrierResponse {
            id: format!("epoch_{}_sync", current_epoch),
            name: format!("Epoch {} Synchronization", current_epoch),
            arrived: 2,
            total: 3,
            status: "waiting".to_string(),
            created_at: now - 5000,
        }]
    } else {
        vec![]
    };
    
    let metrics = MetricsResponse {
        checkpoint_throughput: 45 + (uptime % 20),
        coordinator_rps: 120 + (uptime % 30),
        active_workers: workers.iter().filter(|w| w.status == "active").count() as u32,
        total_workers: workers.len() as u32,
        barrier_latency_p99: 20 + (uptime % 15),
        shard_assignment_time: 8 + (uptime % 5),
    };
    
    DashboardState {
        coordinator: StatusResponse {
            connected: true,
            address: "localhost:50052".to_string(),
            uptime,
            version: "0.1.0".to_string(),
        },
        workers,
        datasets,
        checkpoints,
        barriers,
        metrics,
        tasks: get_demo_tasks(uptime),
        logs: get_demo_logs(uptime),
    }
}

/// Generate demo tasks
fn get_demo_tasks(uptime: u64) -> Vec<TaskResponse> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    
    let mut tasks = vec![];
    
    // Always show at least one active training task (unless stopped by user)
    if uptime > 10 {
        let task_id = "task_vision_training".to_string();
        
        // Check if this task was stopped by user
        if let Some(stopped_at) = get_stopped_tasks().get(&task_id) {
            // Show as completed
            tasks.push(TaskResponse {
                id: task_id,
                name: "Vision Model Training".to_string(),
                r#type: "image_classification".to_string(),
                status: "completed".to_string(),
                worker_ids: vec!["gpu-worker-01".to_string(), "gpu-worker-02".to_string()],
                dataset_id: "imagenet-train".to_string(),
                started_at: now - (uptime * 1000) as i64,
                completed_at: Some(*stopped_at),
                progress: 100,
                logs: vec![
                    format!("[{}] Task started with 2 workers", chrono::Utc::now().format("%H:%M:%S")),
                    format!("[{}] Dataset loaded: 1,281,167 samples", chrono::Utc::now().format("%H:%M:%S")),
                    format!("[{}] Task stopped by user", chrono::Utc::now().format("%H:%M:%S")),
                ],
            });
        } else {
            // Show as running
            let progress = ((uptime % 300) * 100 / 300) as u32; // 5 minute cycles
            tasks.push(TaskResponse {
                id: task_id,
                name: "Vision Model Training".to_string(),
                r#type: "image_classification".to_string(),
                status: if progress < 95 { "running".to_string() } else { "completed".to_string() },
                worker_ids: vec!["gpu-worker-01".to_string(), "gpu-worker-02".to_string()],
                dataset_id: "imagenet-train".to_string(),
                started_at: now - (uptime * 1000) as i64,
                completed_at: if progress >= 95 { Some(now - 30000) } else { None },
                progress,
                logs: vec![
                    format!("[{}] Task started with 2 workers", chrono::Utc::now().format("%H:%M:%S")),
                    format!("[{}] Dataset loaded: 1,281,167 samples", chrono::Utc::now().format("%H:%M:%S")),
                    format!("[{}] Training progress: {}%", chrono::Utc::now().format("%H:%M:%S"), progress),
                ],
            });
        }
    }
    
    // Show completed task occasionally (unless stopped)
    if uptime > 60 && uptime % 120 < 60 {
        let task_id = "task_nlp_completed".to_string();
        
        if !get_stopped_tasks().contains_key(&task_id) {
            tasks.push(TaskResponse {
                id: task_id,
                name: "NLP Model Fine-tuning".to_string(),
                r#type: "nlp_training".to_string(),
                status: "completed".to_string(),
                worker_ids: vec!["cpu-worker-01".to_string()],
                dataset_id: "custom-vision".to_string(),
                started_at: now - 3600000,
                completed_at: Some(now - 1800000),
                progress: 100,
                logs: vec![
                    "[14:30:15] Task started with 1 worker".to_string(),
                    "[14:45:22] Epoch 1/5 completed - Loss: 0.234".to_string(),
                    "[15:00:18] Epoch 5/5 completed - Final accuracy: 94.2%".to_string(),
                    "[15:00:45] Task completed successfully".to_string(),
                ],
            });
        }
    }
    
    tasks
}

/// Generate demo logs
fn get_demo_logs(uptime: u64) -> Vec<LogResponse> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
    
    let mut logs = vec![];
    
    // Generate realistic system logs
    let log_entries = vec![
        ("info", "coordinator", "Coordinator service started", None, None),
        ("info", "worker_registry", "Worker gpu-worker-01 registered successfully", None, Some("gpu-worker-01")),
        ("info", "worker_registry", "Worker gpu-worker-02 registered successfully", None, Some("gpu-worker-02")),
        ("info", "task_manager", "Training task started: Vision Model Training", Some("task_vision_training"), None),
        ("info", "data_loader", "Dataset imagenet-train loaded: 1,281,167 samples", Some("task_vision_training"), None),
        ("debug", "shard_manager", "Assigned 12 shards to gpu-worker-01", Some("task_vision_training"), Some("gpu-worker-01")),
        ("debug", "shard_manager", "Assigned 12 shards to gpu-worker-02", Some("task_vision_training"), Some("gpu-worker-02")),
        ("info", "training", "Epoch 1 started - 500 steps", Some("task_vision_training"), None),
        ("info", "checkpoint", "Checkpoint saved: epoch_1.pt (650MB)", Some("task_vision_training"), Some("gpu-worker-01")),
        ("warn", "heartbeat", "Worker cpu-worker-01 heartbeat delayed", None, Some("cpu-worker-01")),
        ("info", "barrier", "Barrier sync completed: 3/3 workers", Some("task_vision_training"), None),
        ("info", "training", "Training progress: 45% complete", Some("task_vision_training"), None),
    ];
    
    for (i, (level, source, message, task_id, worker_id)) in log_entries.iter().enumerate() {
        let timestamp = now - ((log_entries.len() - i) * 30000) as i64; // 30 seconds apart
        logs.push(LogResponse {
            id: format!("log_{}", &uuid::Uuid::new_v4().to_string()[..8]),
            timestamp,
            level: level.to_string(),
            message: message.to_string(),
            source: source.to_string(),
            task_id: task_id.map(|s| s.to_string()),
            worker_id: worker_id.map(|s| s.to_string()),
        });
    }
    
    // Add some recent logs based on uptime
    if uptime > 0 {
        logs.push(LogResponse {
            id: format!("log_{}", &uuid::Uuid::new_v4().to_string()[..8]),
            timestamp: now - 5000,
            level: "info".to_string(),
            message: format!("System uptime: {}s, {} active workers", uptime, 2),
            source: "coordinator".to_string(),
            task_id: None,
            worker_id: None,
        });
    }
    
    logs.reverse(); // Most recent first
    logs
}
