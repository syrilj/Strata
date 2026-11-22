//! HTTP API for dashboard integration
//!
//! Provides REST endpoints for the dashboard to query and control coordinator state.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::path::Path as FsPath;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use crate::service::CoordinatorService;

/// Shared state for HTTP handlers (Arc for thread-safe sharing)
pub type AppState = Arc<CoordinatorService>;

/// GPU usage info for API response
#[derive(Serialize, Clone)]
pub struct GpuUsageResponse {
    pub gpu_id: u32,
    pub utilization_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub temperature_celsius: f64,
}

/// Worker resource metrics for API response
#[derive(Serialize, Clone)]
pub struct WorkerResourcesResponse {
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub gpu_usage: Vec<GpuUsageResponse>,
}

/// Worker info for API response
#[derive(Serialize, Clone)]
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
    pub resources: Option<WorkerResourcesResponse>,
}

/// Dataset info for API response
#[derive(Serialize, Clone)]
pub struct DatasetResponse {
    pub id: String,
    pub name: String,
    pub path: String,
    pub total_samples: u64,
    pub shard_size: u64,
    pub shard_count: u64,
    pub format: String,
    pub shuffle: bool,
    pub registered_at: i64,
}

/// Checkpoint info for API response
#[derive(Serialize, Clone)]
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
#[derive(Serialize, Clone)]
pub struct BarrierResponse {
    pub id: String,
    pub name: String,
    pub arrived: u64,
    pub total: u64,
    pub status: String,
    pub created_at: i64,
}

/// System metrics for API response
#[derive(Serialize, Clone)]
pub struct MetricsResponse {
    pub checkpoint_throughput: f64,
    pub coordinator_rps: u64,
    pub active_workers: u32,
    pub total_workers: u32,
    pub barrier_latency_p99: u64,
    pub shard_assignment_time: u64,
}

/// Rolling throughput history point
#[derive(Serialize, Clone, Debug)]
pub struct ThroughputPoint {
    pub timestamp: i64,
    pub time: String,
    pub checkpoint_throughput: f64,
    pub coordinator_rps: u64,
    pub active_workers: usize,
    pub barrier_latency_p99: u64,
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
#[derive(Serialize, Clone)]
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
#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub name: String,
    pub r#type: String,
    pub dataset_id: String,
    pub worker_count: u32,
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
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

/// Dataset registration request
#[derive(Deserialize)]
pub struct RegisterDatasetRequest {
    pub id: Option<String>,
    pub name: String,
    pub path: String,
    pub format: String,
    pub total_samples: Option<u64>,
    pub shard_size: u64,
    #[serde(default)]
    pub shuffle: bool,
    #[serde(default = "default_seed")]
    pub seed: u64,
}

fn default_seed() -> u64 {
    42
}

/// Dataset registration response (flattened DatasetResponse fields + success wrapper)
#[derive(Serialize)]
pub struct RegisterDatasetResponse {
    pub success: bool,
    pub dataset: DatasetResponse,
    #[serde(flatten)]
    pub flat: DatasetResponse,
}

/// Worker-grouped shard assignment entry
#[derive(Serialize)]
pub struct WorkerShardAssignment {
    pub worker_id: String,
    pub shard_ids: Vec<u64>,
    pub sample_range: [u64; 2],
}

/// Individual shard detail entry
#[derive(Serialize)]
pub struct DatasetShardDetail {
    pub shard_id: u64,
    pub worker_id: Option<String>,
    pub sample_count: u64,
    pub start_index: u64,
    pub end_index: u64,
    pub status: String,
}

/// Dataset shards endpoint response
#[derive(Serialize)]
pub struct DatasetShardsResponse {
    pub dataset_id: String,
    pub epoch: u64,
    pub total_shards: u64,
    pub shard_size: u64,
    pub assignments: Vec<WorkerShardAssignment>,
    pub shards: Vec<DatasetShardDetail>,
}

/// Dataset preview sample row
#[derive(Serialize)]
pub struct DataPreviewSample {
    pub id: String,
    pub features: serde_json::Map<String, serde_json::Value>,
    pub label: Option<serde_json::Value>,
    pub shard_id: u64,
    pub worker_id: Option<String>,
}

/// Dataset preview endpoint response
#[derive(Serialize)]
pub struct DatasetPreviewResponse {
    pub dataset_id: String,
    pub format: String,
    pub source: String,
    pub total_samples: u64,
    pub columns: Vec<String>,
    pub samples: Vec<DataPreviewSample>,
}

/// Coordinator status for API response
#[derive(Serialize)]
pub struct StatusResponse {
    pub connected: bool,
    pub address: String,
    pub http_address: String,
    pub uptime: u64,
    pub version: String,
    pub storage_backend: String,
    pub demo_mode: bool,
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
        .route("/api/workers/:id", get(get_worker).delete(delete_worker))
        .route("/api/datasets", get(get_datasets).post(register_dataset))
        .route("/api/datasets/:id", delete(delete_dataset))
        .route("/api/datasets/:id/shards", get(get_dataset_shards))
        .route("/api/datasets/:id/preview", get(get_dataset_preview))
        .route("/api/checkpoints", get(get_checkpoints))
        .route(
            "/api/checkpoints/:id",
            get(get_checkpoint).delete(delete_checkpoint),
        )
        .route("/api/barriers", get(get_barriers))
        .route("/api/metrics", get(get_metrics))
        .route("/api/metrics/history", get(get_metrics_history))
        .route("/api/dashboard", get(get_dashboard_state))
        .route("/api/tasks", get(get_tasks).post(create_task))
        .route("/api/tasks/:task_id/stop", post(stop_task))
        .route("/api/tasks/:task_id/logs", get(get_task_logs))
        .route("/api/logs", get(get_logs))
        .layer(cors)
        .with_state(service)
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// Health check endpoint
async fn health_check(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    service.record_request_outcome("http_health", "http", start, true);
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

/// Get coordinator status
async fn get_status(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    let status = StatusResponse {
        connected: true,
        address: service.grpc_address().to_string(),
        http_address: service.http_address().to_string(),
        uptime: service.uptime_secs(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        storage_backend: service.storage_backend().to_string(),
        demo_mode: service.is_demo_mode(),
    };
    service.record_request_outcome("http_status", "http", start, true);
    Json(status)
}

/// Get all workers
async fn get_workers(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        service.record_request_outcome("http_get_workers", "http", start, true);
        return Json(demo_state.workers);
    }
    let workers = service.get_workers_for_api();
    service.record_request_outcome("http_get_workers", "http", start, true);
    Json(workers)
}

/// Get a single worker by ID
async fn get_worker(State(service): State<AppState>, Path(id): Path<String>) -> Response {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        if let Some(worker) = demo_state.workers.into_iter().find(|w| w.id == id) {
            service.record_request_outcome("http_get_worker", "http", start, true);
            return Json(worker).into_response();
        }
    }

    match service.get_worker_for_api(&id) {
        Some(worker) => {
            service.record_request_outcome("http_get_worker", "http", start, true);
            Json(worker).into_response()
        }
        None => {
            service.record_request_outcome("http_get_worker", "http", start, false);
            json_error(StatusCode::NOT_FOUND, format!("Worker '{}' not found", id))
        }
    }
}

/// Deregister a worker by ID
async fn delete_worker(State(service): State<AppState>, Path(id): Path<String>) -> Response {
    let start = Instant::now();
    match service.deregister_worker_by_id(&id) {
        Ok(()) => {
            service.record_request_outcome("http_delete_worker", "http", start, true);
            (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response()
        }
        Err(msg) => {
            service.record_request_outcome("http_delete_worker", "http", start, false);
            json_error(StatusCode::NOT_FOUND, msg)
        }
    }
}

/// Get all datasets
async fn get_datasets(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        service.record_request_outcome("http_get_datasets", "http", start, true);
        return Json(demo_state.datasets);
    }
    let datasets = service.get_datasets_for_api();
    service.record_request_outcome("http_get_datasets", "http", start, true);
    Json(datasets)
}

/// Count lines/files for local dataset auto-detection when total_samples is omitted/0
fn infer_local_sample_count(path_str: &str, format: &str) -> u64 {
    let path = FsPath::new(path_str);
    if !path.exists() {
        return 0;
    }

    let is_line_format = matches!(
        format.trim().to_ascii_lowercase().as_str(),
        "csv" | "jsonl" | "ndjson" | "txt" | "text"
    );

    if path.is_file() {
        if is_line_format {
            if let Ok(file) = std::fs::File::open(path) {
                let reader = BufReader::new(file);
                let mut count = 0u64;
                for line in reader.lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        count += 1;
                    }
                }
                if format.eq_ignore_ascii_case("csv") && count > 0 {
                    return count - 1; // exclude CSV header row
                }
                return count;
            }
        }
        return 1;
    }

    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        if is_line_format {
            let mut total = 0u64;
            for file_path in &files {
                if let Ok(file) = std::fs::File::open(file_path) {
                    let mut file_lines = 0u64;
                    for line in BufReader::new(file).lines().map_while(Result::ok) {
                        if !line.trim().is_empty() {
                            file_lines += 1;
                        }
                    }
                    if format.eq_ignore_ascii_case("csv") && file_lines > 0 {
                        file_lines -= 1;
                    }
                    total += file_lines;
                }
            }
            return total;
        }
        return files.len() as u64;
    }

    0
}

/// Sanitize a dataset name into a valid dataset ID
fn slugify_dataset_id(name: &str) -> String {
    let slug: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        format!("ds_{}", &uuid::Uuid::new_v4().to_string()[..8])
    } else {
        trimmed.to_string()
    }
}

/// Register a new dataset via HTTP POST /api/datasets
async fn register_dataset(
    State(service): State<AppState>,
    Json(req): Json<RegisterDatasetRequest>,
) -> Response {
    let start = Instant::now();
    let name = req.name.trim().to_string();
    if name.is_empty() {
        service.record_request_outcome("http_register_dataset", "http", start, false);
        return json_error(StatusCode::BAD_REQUEST, "Dataset name cannot be empty");
    }
    if req.shard_size == 0 {
        service.record_request_outcome("http_register_dataset", "http", start, false);
        return json_error(StatusCode::BAD_REQUEST, "shard_size must be greater than 0");
    }

    let dataset_id = req
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| slugify_dataset_id(&name));

    let total_samples = match req.total_samples {
        Some(n) if n > 0 => n,
        _ => infer_local_sample_count(&req.path, &req.format),
    };

    let mut metadata = HashMap::new();
    metadata.insert("name".to_string(), name.clone());

    match service.register_dataset_entry(
        dataset_id,
        name,
        req.path,
        req.format,
        total_samples,
        req.shard_size,
        req.shuffle,
        req.seed,
        metadata,
    ) {
        Ok(dataset) => {
            service.record_request_outcome("http_register_dataset", "http", start, true);
            (
                StatusCode::CREATED,
                Json(RegisterDatasetResponse {
                    success: true,
                    flat: dataset.clone(),
                    dataset,
                }),
            )
                .into_response()
        }
        Err(msg) => {
            service.record_request_outcome("http_register_dataset", "http", start, false);
            json_error(StatusCode::BAD_REQUEST, msg)
        }
    }
}

/// Delete a dataset by ID
async fn delete_dataset(State(service): State<AppState>, Path(id): Path<String>) -> Response {
    let start = Instant::now();
    if service.delete_dataset_by_id(&id) {
        service.record_request_outcome("http_delete_dataset", "http", start, true);
        (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response()
    } else {
        service.record_request_outcome("http_delete_dataset", "http", start, false);
        json_error(StatusCode::NOT_FOUND, format!("Dataset '{}' not found", id))
    }
}

/// Get shard assignments for a dataset
async fn get_dataset_shards(
    State(service): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let start = Instant::now();
    let Some(tracked) = service.get_tracked_dataset(&id) else {
        service.record_request_outcome("http_dataset_shards", "http", start, false);
        return json_error(StatusCode::NOT_FOUND, format!("Dataset '{}' not found", id));
    };

    let epoch = params
        .get("epoch")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let shard_list = service.shard_manager().list_dataset_shards(&id, epoch);
    let total_shards = shard_list.len() as u64;
    let shard_size = tracked.info.shard_size.max(1) as u64;

    // Group shards by worker_id
    let mut by_worker: BTreeMap<String, Vec<(u64, u64, u64)>> = BTreeMap::new();
    let mut shards = Vec::with_capacity(shard_list.len());

    for (assignment, owner) in shard_list {
        let sample_count = assignment.end_index.saturating_sub(assignment.start_index);
        let status = if owner.is_some() {
            "assigned".to_string()
        } else {
            "unassigned".to_string()
        };
        if let Some(ref w_id) = owner {
            by_worker.entry(w_id.clone()).or_default().push((
                assignment.shard_id,
                assignment.start_index,
                assignment.end_index,
            ));
        }
        shards.push(DatasetShardDetail {
            shard_id: assignment.shard_id,
            worker_id: owner,
            sample_count,
            start_index: assignment.start_index,
            end_index: assignment.end_index,
            status,
        });
    }

    let assignments = by_worker
        .into_iter()
        .map(|(worker_id, entries)| {
            let shard_ids = entries.iter().map(|(sid, _, _)| *sid).collect();
            let min_start = entries.iter().map(|(_, s, _)| *s).min().unwrap_or(0);
            let max_end = entries.iter().map(|(_, _, e)| *e).max().unwrap_or(0);
            WorkerShardAssignment {
                worker_id,
                shard_ids,
                sample_range: [min_start, max_end],
            }
        })
        .collect();

    service.record_request_outcome("http_dataset_shards", "http", start, true);
    Json(DatasetShardsResponse {
        dataset_id: id,
        epoch,
        total_shards,
        shard_size,
        assignments,
        shards,
    })
    .into_response()
}

/// Read up to `limit` real sample rows from a local dataset file or directory
fn read_local_dataset_samples(
    path_str: &str,
    format: &str,
    shards: &[(runtime_core::ShardAssignment, Option<String>)],
    limit: usize,
) -> Option<(Vec<String>, Vec<DataPreviewSample>)> {
    let path = FsPath::new(path_str);
    if !path.exists() {
        return None;
    }

    let candidate_files: Vec<std::path::PathBuf> = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return None;
        };
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        files
    } else {
        return None;
    };

    if candidate_files.is_empty() {
        return None;
    }

    let ext = candidate_files[0]
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let fmt = format.trim().to_ascii_lowercase();
    let effective_fmt = if fmt.is_empty() || fmt == "unknown" {
        ext.as_str()
    } else {
        fmt.as_str()
    };

    let mut columns: Vec<String> = Vec::new();
    let mut samples: Vec<DataPreviewSample> = Vec::new();

    let shard_for_idx = |row_idx: u64| -> (u64, Option<String>) {
        for (assignment, owner) in shards {
            if row_idx >= assignment.start_index && row_idx < assignment.end_index {
                return (assignment.shard_id, owner.clone());
            }
        }
        shards
            .first()
            .map(|(a, o)| (a.shard_id, o.clone()))
            .unwrap_or((0, None))
    };

    let push_col = |cols: &mut Vec<String>, key: &str| {
        if !cols.iter().any(|c| c == key) {
            cols.push(key.to_string());
        }
    };

    match effective_fmt {
        "csv" => {
            let mut header: Option<Vec<String>> = None;
            let mut row_idx = 0u64;
            for file_path in &candidate_files {
                let Ok(file) = std::fs::File::open(file_path) else {
                    continue;
                };
                for (line_num, line) in BufReader::new(file)
                    .lines()
                    .map_while(Result::ok)
                    .enumerate()
                {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let cells: Vec<String> =
                        trimmed.split(',').map(|s| s.trim().to_string()).collect();
                    if line_num == 0 && header.is_none() {
                        header = Some(cells.clone());
                        for h in &cells {
                            push_col(&mut columns, h);
                        }
                        continue;
                    }
                    if line_num == 0 {
                        // Skip repeated headers in multi-file CSV datasets
                        if header.as_ref() == Some(&cells) {
                            continue;
                        }
                    }

                    let mut features = serde_json::Map::new();
                    let mut label = None;
                    if let Some(cols) = &header {
                        for (idx, val) in cells.iter().enumerate() {
                            let col_name = cols
                                .get(idx)
                                .cloned()
                                .unwrap_or_else(|| format!("col_{}", idx));
                            push_col(&mut columns, &col_name);
                            let json_val = parse_scalar_value(val);
                            if col_name.eq_ignore_ascii_case("label")
                                || col_name.eq_ignore_ascii_case("target")
                            {
                                label = Some(json_val.clone());
                            }
                            features.insert(col_name, json_val);
                        }
                    }

                    let (shard_id, worker_id) = shard_for_idx(row_idx);
                    samples.push(DataPreviewSample {
                        id: format!("sample_{}", row_idx),
                        features,
                        label,
                        shard_id,
                        worker_id,
                    });
                    row_idx += 1;
                    if samples.len() >= limit {
                        return Some((columns, samples));
                    }
                }
            }
            if !samples.is_empty() {
                return Some((columns, samples));
            }
        }
        "jsonl" | "ndjson" => {
            let mut row_idx = 0u64;
            for file_path in &candidate_files {
                let Ok(file) = std::fs::File::open(file_path) else {
                    continue;
                };
                for line in BufReader::new(file).lines().map_while(Result::ok) {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                        continue;
                    };
                    let mut features = serde_json::Map::new();
                    let mut label = None;
                    match val {
                        serde_json::Value::Object(map) => {
                            for (k, v) in map {
                                push_col(&mut columns, &k);
                                if k.eq_ignore_ascii_case("label")
                                    || k.eq_ignore_ascii_case("target")
                                {
                                    label = Some(v.clone());
                                }
                                features.insert(k, v);
                            }
                        }
                        other => {
                            push_col(&mut columns, "value");
                            features.insert("value".to_string(), other);
                        }
                    }
                    let (shard_id, worker_id) = shard_for_idx(row_idx);
                    samples.push(DataPreviewSample {
                        id: format!("sample_{}", row_idx),
                        features,
                        label,
                        shard_id,
                        worker_id,
                    });
                    row_idx += 1;
                    if samples.len() >= limit {
                        return Some((columns, samples));
                    }
                }
            }
            if !samples.is_empty() {
                return Some((columns, samples));
            }
        }
        "json" => {
            let mut row_idx = 0u64;
            for file_path in &candidate_files {
                let Ok(raw) = std::fs::read_to_string(file_path) else {
                    continue;
                };
                let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
                    continue;
                };
                let items = match parsed {
                    serde_json::Value::Array(arr) => arr,
                    other => vec![other],
                };
                for item in items {
                    let mut features = serde_json::Map::new();
                    let mut label = None;
                    match item {
                        serde_json::Value::Object(map) => {
                            for (k, v) in map {
                                push_col(&mut columns, &k);
                                if k.eq_ignore_ascii_case("label")
                                    || k.eq_ignore_ascii_case("target")
                                {
                                    label = Some(v.clone());
                                }
                                features.insert(k, v);
                            }
                        }
                        other => {
                            push_col(&mut columns, "value");
                            features.insert("value".to_string(), other);
                        }
                    }
                    let (shard_id, worker_id) = shard_for_idx(row_idx);
                    samples.push(DataPreviewSample {
                        id: format!("sample_{}", row_idx),
                        features,
                        label,
                        shard_id,
                        worker_id,
                    });
                    row_idx += 1;
                    if samples.len() >= limit {
                        return Some((columns, samples));
                    }
                }
            }
            if !samples.is_empty() {
                return Some((columns, samples));
            }
        }
        "txt" | "text" => {
            push_col(&mut columns, "text");
            let mut row_idx = 0u64;
            for file_path in &candidate_files {
                let Ok(file) = std::fs::File::open(file_path) else {
                    continue;
                };
                for line in BufReader::new(file).lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let mut features = serde_json::Map::new();
                    features.insert("text".to_string(), serde_json::Value::String(line));
                    let (shard_id, worker_id) = shard_for_idx(row_idx);
                    samples.push(DataPreviewSample {
                        id: format!("sample_{}", row_idx),
                        features,
                        label: None,
                        shard_id,
                        worker_id,
                    });
                    row_idx += 1;
                    if samples.len() >= limit {
                        return Some((columns, samples));
                    }
                }
            }
            if !samples.is_empty() {
                return Some((columns, samples));
            }
        }
        _ => {}
    }

    None
}

fn parse_scalar_value(raw: &str) -> serde_json::Value {
    if let Ok(i) = raw.parse::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = raw.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(num);
        }
    }
    if raw.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }
    serde_json::Value::String(raw.to_string())
}

/// Preview real samples (from local files) or real shard metadata rows
async fn get_dataset_preview(State(service): State<AppState>, Path(id): Path<String>) -> Response {
    let start = Instant::now();
    let Some(tracked) = service.get_tracked_dataset(&id) else {
        service.record_request_outcome("http_dataset_preview", "http", start, false);
        return json_error(StatusCode::NOT_FOUND, format!("Dataset '{}' not found", id));
    };

    let shards = service.shard_manager().list_dataset_shards(&id, 0);
    let total_samples = tracked.info.total_samples.max(0) as u64;
    let format = tracked.info.format.clone();

    if let Some((columns, samples)) =
        read_local_dataset_samples(&tracked.info.path, &format, &shards, 20)
    {
        service.record_request_outcome("http_dataset_preview", "http", start, true);
        return Json(DatasetPreviewResponse {
            dataset_id: id,
            format,
            source: "local_files".to_string(),
            total_samples,
            columns,
            samples,
        })
        .into_response();
    }

    // Fallback to real shard index metadata rows (never fake hardcoded stock/ImageNet mocks)
    let columns = vec![
        "shard_id".to_string(),
        "start_index".to_string(),
        "end_index".to_string(),
        "sample_count".to_string(),
        "file_paths".to_string(),
    ];
    let samples = shards
        .into_iter()
        .take(20)
        .map(|(assignment, owner)| {
            let mut features = serde_json::Map::new();
            features.insert(
                "shard_id".to_string(),
                serde_json::json!(assignment.shard_id),
            );
            features.insert(
                "start_index".to_string(),
                serde_json::json!(assignment.start_index),
            );
            features.insert(
                "end_index".to_string(),
                serde_json::json!(assignment.end_index),
            );
            features.insert(
                "sample_count".to_string(),
                serde_json::json!(assignment.end_index.saturating_sub(assignment.start_index)),
            );
            features.insert(
                "file_paths".to_string(),
                serde_json::json!(assignment.file_paths),
            );
            DataPreviewSample {
                id: format!("shard_{}", assignment.shard_id),
                features,
                label: None,
                shard_id: assignment.shard_id,
                worker_id: owner,
            }
        })
        .collect();

    service.record_request_outcome("http_dataset_preview", "http", start, true);
    Json(DatasetPreviewResponse {
        dataset_id: id,
        format,
        source: "shard_metadata".to_string(),
        total_samples,
        columns,
        samples,
    })
    .into_response()
}

/// Get recent checkpoints
async fn get_checkpoints(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        service.record_request_outcome("http_get_checkpoints", "http", start, true);
        return Json(demo_state.checkpoints);
    }
    let checkpoints = service.get_checkpoints_for_api();
    service.record_request_outcome("http_get_checkpoints", "http", start, true);
    Json(checkpoints)
}

/// Get a single checkpoint by ID
async fn get_checkpoint(State(service): State<AppState>, Path(id): Path<String>) -> Response {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        if let Some(ckpt) = demo_state.checkpoints.into_iter().find(|c| c.id == id) {
            service.record_request_outcome("http_get_checkpoint", "http", start, true);
            return Json(ckpt).into_response();
        }
    }

    match service.get_checkpoint_for_api(&id) {
        Some(ckpt) => {
            service.record_request_outcome("http_get_checkpoint", "http", start, true);
            Json(ckpt).into_response()
        }
        None => {
            service.record_request_outcome("http_get_checkpoint", "http", start, false);
            json_error(
                StatusCode::NOT_FOUND,
                format!("Checkpoint '{}' not found", id),
            )
        }
    }
}

/// Delete a checkpoint by ID
async fn delete_checkpoint(State(service): State<AppState>, Path(id): Path<String>) -> Response {
    let start = Instant::now();
    match service.delete_checkpoint_by_id(&id).await {
        Ok(true) => {
            service.record_request_outcome("http_delete_checkpoint", "http", start, true);
            (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response()
        }
        Ok(false) => {
            service.record_request_outcome("http_delete_checkpoint", "http", start, false);
            json_error(
                StatusCode::NOT_FOUND,
                format!("Checkpoint '{}' not found", id),
            )
        }
        Err(msg) => {
            service.record_request_outcome("http_delete_checkpoint", "http", start, false);
            json_error(StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    }
}

/// Get barrier status
async fn get_barriers(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        service.record_request_outcome("http_get_barriers", "http", start, true);
        return Json(demo_state.barriers);
    }
    let barriers = service.get_barriers_for_api();
    service.record_request_outcome("http_get_barriers", "http", start, true);
    Json(barriers)
}

/// Get system metrics
async fn get_metrics(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        service.record_request_outcome("http_get_metrics", "http", start, true);
        return Json(demo_state.metrics);
    }
    let metrics = service.get_metrics_for_api();
    service.record_request_outcome("http_get_metrics", "http", start, true);
    Json(metrics)
}

/// Get rolling throughput history
async fn get_metrics_history(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    let history = service.get_throughput_history_for_api();
    service.record_request_outcome("http_metrics_history", "http", start, true);
    Json(history)
}

/// Get full dashboard state in one request
async fn get_dashboard_state(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    if service.is_demo_mode() {
        let demo_state = get_demo_dashboard_state(&service);
        service.record_request_outcome("http_dashboard", "http", start, true);
        return Json(demo_state);
    }

    let uptime = service.uptime_secs();
    let state = DashboardState {
        coordinator: StatusResponse {
            connected: true,
            address: service.grpc_address().to_string(),
            http_address: service.http_address().to_string(),
            uptime,
            version: env!("CARGO_PKG_VERSION").to_string(),
            storage_backend: service.storage_backend().to_string(),
            demo_mode: false,
        },
        workers: service.get_workers_for_api(),
        datasets: service.get_datasets_for_api(),
        checkpoints: service.get_checkpoints_for_api(),
        barriers: service.get_barriers_for_api(),
        metrics: service.get_metrics_for_api(),
        tasks: service.get_tasks_for_api(),
        logs: service.get_logs_for_api(None, None, None, 100),
    };
    service.record_request_outcome("http_dashboard", "http", start, true);
    Json(state)
}

/// Get all tasks
async fn get_tasks(State(service): State<AppState>) -> impl IntoResponse {
    let start = Instant::now();
    let mut all_tasks = service.get_tasks_for_api();

    if service.is_demo_mode() {
        let demo_tasks = get_demo_tasks(service.uptime_secs());
        for demo_task in demo_tasks {
            if !all_tasks.iter().any(|t| t.id == demo_task.id) {
                all_tasks.push(demo_task);
            }
        }
    }

    service.record_request_outcome("http_get_tasks", "http", start, true);
    Json(all_tasks)
}

/// Create a new task
async fn create_task(
    State(service): State<AppState>,
    Json(request): Json<CreateTaskRequest>,
) -> Response {
    let start = Instant::now();
    match service.create_task_entry(
        request.name,
        request.r#type,
        request.dataset_id,
        request.worker_count,
        request.config,
    ) {
        Ok(task_id) => {
            service.record_request_outcome("http_create_task", "http", start, true);
            (StatusCode::CREATED, Json(CreateTaskResponse { task_id })).into_response()
        }
        Err(msg) => {
            service.record_request_outcome("http_create_task", "http", start, false);
            json_error(StatusCode::BAD_REQUEST, msg)
        }
    }
}

/// Stop a task
async fn stop_task(State(service): State<AppState>, Path(task_id): Path<String>) -> Response {
    let start = Instant::now();
    match service.stop_task_by_id(&task_id) {
        Ok(()) => {
            service.record_request_outcome("http_stop_task", "http", start, true);
            Json(StopTaskResponse { success: true }).into_response()
        }
        Err(msg) => {
            service.record_request_outcome("http_stop_task", "http", start, false);
            json_error(StatusCode::NOT_FOUND, msg)
        }
    }
}

/// Get logs for a specific task
async fn get_task_logs(
    State(service): State<AppState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    let start = Instant::now();
    let logs = service.get_task_logs_for_api(&task_id);
    if !logs.is_empty() || !service.is_demo_mode() {
        service.record_request_outcome("http_get_task_logs", "http", start, true);
        return Json(logs);
    }

    let demo_logs = get_demo_logs(service.uptime_secs())
        .into_iter()
        .filter(|log| log.task_id.as_ref() == Some(&task_id))
        .collect::<Vec<_>>();
    service.record_request_outcome("http_get_task_logs", "http", start, true);
    Json(demo_logs)
}

/// Get system logs with optional `?limit=100&level=...&source=...&task_id=...`
async fn get_logs(
    State(service): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let start = Instant::now();
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);
    let level = params
        .get("level")
        .map(String::as_str)
        .filter(|s| !s.is_empty());
    let source = params
        .get("source")
        .map(String::as_str)
        .filter(|s| !s.is_empty());
    let task_id = params
        .get("task_id")
        .map(String::as_str)
        .filter(|s| !s.is_empty());

    if service.is_demo_mode() {
        let mut logs = get_demo_logs(service.uptime_secs());
        logs.retain(|l| {
            level.is_none_or(|lvl| l.level.eq_ignore_ascii_case(lvl))
                && source.is_none_or(|src| l.source == src)
                && task_id.is_none_or(|tid| l.task_id.as_deref() == Some(tid))
        });
        logs.truncate(limit);
        service.record_request_outcome("http_get_logs", "http", start, true);
        return Json(logs);
    }

    let logs = service.get_logs_for_api(level, source, task_id, limit);
    service.record_request_outcome("http_get_logs", "http", start, true);
    Json(logs)
}

/// Generate demo dashboard state with active training simulation (only when demo_mode is true)
fn get_demo_dashboard_state(service: &CoordinatorService) -> DashboardState {
    let uptime = service.uptime_secs();
    let now = chrono::Utc::now().timestamp_millis();

    let base_step = uptime / 3;
    let current_epoch = (base_step / 500) + 1;
    let current_step = base_step % 500;

    let workers = vec![
        WorkerResponse {
            id: "gpu-worker-01".to_string(),
            ip: "gpu-node-01".to_string(),
            port: 50052,
            status: "training".to_string(),
            gpu_count: 8,
            last_heartbeat: now,
            assigned_shards: 12,
            current_epoch,
            current_step,
            current_task: "forward_pass".to_string(),
            resources: None,
        },
        WorkerResponse {
            id: "gpu-worker-02".to_string(),
            ip: "gpu-node-02".to_string(),
            port: 50052,
            status: "training".to_string(),
            gpu_count: 8,
            last_heartbeat: now,
            assigned_shards: 12,
            current_epoch,
            current_step: current_step.saturating_sub(2),
            current_task: "backward_pass".to_string(),
            resources: None,
        },
        WorkerResponse {
            id: "cpu-worker-01".to_string(),
            ip: "cpu-node-01".to_string(),
            port: 50052,
            status: if uptime % 30 < 5 {
                "idle".to_string()
            } else {
                "loading_data".to_string()
            },
            gpu_count: 0,
            last_heartbeat: now,
            assigned_shards: 8,
            current_epoch,
            current_step: 0,
            current_task: "data_preprocessing".to_string(),
            resources: None,
        },
    ];

    let datasets = vec![
        DatasetResponse {
            id: "imagenet-train".to_string(),
            name: "ImageNet Training Set".to_string(),
            path: "/data/imagenet/train".to_string(),
            total_samples: 1281167,
            shard_size: 10000,
            shard_count: 129,
            format: "tfrecord".to_string(),
            shuffle: true,
            registered_at: now - 3600000,
        },
        DatasetResponse {
            id: "custom-vision".to_string(),
            name: "Custom Vision Model".to_string(),
            path: "/data/vision".to_string(),
            total_samples: 500000,
            shard_size: 8000,
            shard_count: 63,
            format: "parquet".to_string(),
            shuffle: true,
            registered_at: now - 1800000,
        },
    ];

    let mut checkpoints = Vec::new();
    for epoch in 1..current_epoch {
        checkpoints.push(CheckpointResponse {
            id: format!("checkpoint_epoch_{}", epoch),
            step: 500,
            epoch,
            size: 650 * 1024 * 1024,
            path: format!("/checkpoints/epoch_{}.pt", epoch),
            created_at: now - ((current_epoch - epoch) * 1500000) as i64,
            worker_id: "gpu-worker-01".to_string(),
            status: "completed".to_string(),
        });
    }

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
        checkpoint_throughput: (45 + (uptime % 20)) as f64,
        coordinator_rps: 120 + (uptime % 30),
        active_workers: workers.iter().filter(|w| w.status != "idle").count() as u32,
        total_workers: workers.len() as u32,
        barrier_latency_p99: 20 + (uptime % 15),
        shard_assignment_time: 8 + (uptime % 5),
    };

    DashboardState {
        coordinator: StatusResponse {
            connected: true,
            address: service.grpc_address().to_string(),
            http_address: service.http_address().to_string(),
            uptime,
            version: env!("CARGO_PKG_VERSION").to_string(),
            storage_backend: service.storage_backend().to_string(),
            demo_mode: true,
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

/// Generate demo tasks (only used when DEMO_MODE is true)
fn get_demo_tasks(uptime: u64) -> Vec<TaskResponse> {
    let now = chrono::Utc::now().timestamp_millis();
    let mut tasks = Vec::new();
    if uptime > 0 {
        let progress = ((uptime % 300) * 100 / 300) as u32;
        tasks.push(TaskResponse {
            id: "task_vision_training".to_string(),
            name: "Vision Model Training".to_string(),
            r#type: "image_classification".to_string(),
            status: if progress < 95 {
                "running".to_string()
            } else {
                "completed".to_string()
            },
            worker_ids: vec!["gpu-worker-01".to_string(), "gpu-worker-02".to_string()],
            dataset_id: "imagenet-train".to_string(),
            started_at: now - (uptime * 1000) as i64,
            completed_at: None,
            progress,
            logs: vec!["Demo training task active".to_string()],
        });
    }
    tasks
}

/// Generate demo logs (only used when DEMO_MODE is true)
fn get_demo_logs(_uptime: u64) -> Vec<LogResponse> {
    let now = chrono::Utc::now().timestamp_millis();
    vec![LogResponse {
        id: "demo_log_1".to_string(),
        timestamp: now,
        level: "info".to_string(),
        message: "Demo mode active".to_string(),
        source: "coordinator".to_string(),
        task_id: None,
        worker_id: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use checkpoint::CheckpointManagerConfig;
    use http_body_util::BodyExt;
    use std::time::Duration;
    use tempfile::tempdir;
    use tower::ServiceExt;

    async fn setup_app() -> (tempfile::TempDir, Arc<CoordinatorService>, Router) {
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
        let router = create_router(Arc::clone(&service));
        (dir, service, router)
    }

    async fn send_json(
        router: &Router,
        method: Method,
        uri: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let req_builder = Request::builder().method(method).uri(uri);
        let req = match body {
            Some(json) => req_builder
                .header("content-type", "application/json")
                .body(Body::from(json.to_string()))
                .unwrap(),
            None => req_builder.body(Body::empty()).unwrap(),
        };
        let resp = router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let parsed = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, parsed)
    }

    #[tokio::test]
    async fn test_status_dashboard_and_metrics_history() {
        let (_dir, _svc, router) = setup_app().await;

        let (status, body) = send_json(&router, Method::GET, "/api/status", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["connected"], true);
        assert_eq!(body["storage_backend"], "local");
        assert_eq!(body["demo_mode"], false);

        // Live-mode dashboard returns real empty tasks (never demo tasks) and real logs
        let (dash_status, dash) = send_json(&router, Method::GET, "/api/dashboard", None).await;
        assert_eq!(dash_status, StatusCode::OK);
        assert_eq!(dash["tasks"].as_array().unwrap().len(), 0);
        assert!(!dash["logs"].as_array().unwrap().is_empty());

        let (hist_status, hist) =
            send_json(&router, Method::GET, "/api/metrics/history", None).await;
        assert_eq!(hist_status, StatusCode::OK);
        assert_eq!(hist.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_workers_get_and_delete() {
        let (_dir, svc, router) = setup_app().await;
        use crate::proto::{coordinator_server::Coordinator, WorkerInfo};

        svc.register_worker(tonic::Request::new(WorkerInfo {
            worker_id: "w-api-1".to_string(),
            hostname: "10.0.0.5".to_string(),
            port: 50052,
            gpu_count: 2,
            memory_bytes: 16_000_000_000,
            metadata: HashMap::new(),
        }))
        .await
        .unwrap();

        let (status, worker) = send_json(&router, Method::GET, "/api/workers/w-api-1", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(worker["gpu_count"], 2);
        assert!(worker["resources"].is_object());

        let (del_status, del_body) =
            send_json(&router, Method::DELETE, "/api/workers/w-api-1", None).await;
        assert_eq!(del_status, StatusCode::OK);
        assert_eq!(del_body["success"], true);

        let (missing_status, _) =
            send_json(&router, Method::GET, "/api/workers/w-api-1", None).await;
        assert_eq!(missing_status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_datasets_register_preview_shards_and_delete() {
        let (dir, svc, router) = setup_app().await;
        let csv_path = dir.path().join("samples.csv");
        std::fs::write(
            &csv_path,
            "f1,f2,label\n1.0,2.0,cat\n3.0,4.0,dog\n5.0,6.0,bird\n",
        )
        .unwrap();

        // Register a worker so shards get assigned
        use crate::proto::{coordinator_server::Coordinator, WorkerInfo};
        svc.register_worker(tonic::Request::new(WorkerInfo {
            worker_id: "w-ds-1".to_string(),
            hostname: "127.0.0.1".to_string(),
            port: 50052,
            gpu_count: 1,
            memory_bytes: 1024,
            metadata: HashMap::new(),
        }))
        .await
        .unwrap();

        // Validation failure: empty name
        let (bad_status, _) = send_json(
            &router,
            Method::POST,
            "/api/datasets",
            Some(serde_json::json!({
                "name": "   ",
                "path": "/tmp/foo",
                "format": "csv",
                "shard_size": 2
            })),
        )
        .await;
        assert_eq!(bad_status, StatusCode::BAD_REQUEST);

        // Register CSV dataset with omitted total_samples -> auto-counts 3 rows
        let (reg_status, reg_body) = send_json(
            &router,
            Method::POST,
            "/api/datasets",
            Some(serde_json::json!({
                "id": "animals-csv",
                "name": "Animals CSV",
                "path": csv_path.to_string_lossy(),
                "format": "csv",
                "shard_size": 2,
                "shuffle": false
            })),
        )
        .await;
        assert_eq!(reg_status, StatusCode::CREATED);
        assert_eq!(reg_body["success"], true);
        assert_eq!(reg_body["dataset"]["total_samples"], 3);
        assert_eq!(reg_body["dataset"]["shard_count"], 2);

        // Preview reads real CSV rows
        let (prev_status, prev_body) = send_json(
            &router,
            Method::GET,
            "/api/datasets/animals-csv/preview",
            None,
        )
        .await;
        assert_eq!(prev_status, StatusCode::OK);
        assert_eq!(prev_body["source"], "local_files");
        assert_eq!(
            prev_body["columns"],
            serde_json::json!(["f1", "f2", "label"])
        );
        assert_eq!(prev_body["samples"].as_array().unwrap().len(), 3);
        assert_eq!(prev_body["samples"][0]["label"], "cat");

        // Shards endpoint
        let (shards_status, shards_body) = send_json(
            &router,
            Method::GET,
            "/api/datasets/animals-csv/shards",
            None,
        )
        .await;
        assert_eq!(shards_status, StatusCode::OK);
        assert_eq!(shards_body["total_shards"], 2);
        assert_eq!(
            shards_body["assignments"][0]["worker_id"],
            serde_json::json!("w-ds-1")
        );

        // Delete dataset
        let (del_status, _) =
            send_json(&router, Method::DELETE, "/api/datasets/animals-csv", None).await;
        assert_eq!(del_status, StatusCode::OK);

        let (gone_status, _) =
            send_json(&router, Method::DELETE, "/api/datasets/animals-csv", None).await;
        assert_eq!(gone_status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_tasks_and_checkpoints_and_logs_endpoints() {
        let (_dir, svc, router) = setup_app().await;

        // Task creation fails if dataset doesn't exist
        let (err_status, _) = send_json(
            &router,
            Method::POST,
            "/api/tasks",
            Some(serde_json::json!({
                "name": "bad-task",
                "type": "custom",
                "dataset_id": "missing-ds",
                "worker_count": 1,
                "config": {}
            })),
        )
        .await;
        assert_eq!(err_status, StatusCode::BAD_REQUEST);

        // Register dataset and create pending task (0 workers)
        let (ds_status, _) = send_json(
            &router,
            Method::POST,
            "/api/datasets",
            Some(serde_json::json!({
                "id": "ds-task",
                "name": "Task DS",
                "path": "/nonexistent/ds",
                "format": "parquet",
                "total_samples": 100,
                "shard_size": 25
            })),
        )
        .await;
        assert_eq!(ds_status, StatusCode::CREATED);

        // Preview falls back to shard_metadata when local file is absent
        let (prev_status, prev_body) =
            send_json(&router, Method::GET, "/api/datasets/ds-task/preview", None).await;
        assert_eq!(prev_status, StatusCode::OK);
        assert_eq!(prev_body["source"], "shard_metadata");
        assert_eq!(prev_body["samples"].as_array().unwrap().len(), 4);

        let (task_status, task_body) = send_json(
            &router,
            Method::POST,
            "/api/tasks",
            Some(serde_json::json!({
                "name": "resnet-job",
                "type": "image_classification",
                "dataset_id": "ds-task",
                "worker_count": 2,
                "config": { "epochs": 5 }
            })),
        )
        .await;
        assert_eq!(task_status, StatusCode::CREATED);
        let task_id = task_body["task_id"].as_str().unwrap().to_string();

        let (logs_status, logs_body) = send_json(
            &router,
            Method::GET,
            &format!("/api/tasks/{}/logs", task_id),
            None,
        )
        .await;
        assert_eq!(logs_status, StatusCode::OK);
        assert!(!logs_body.as_array().unwrap().is_empty());

        let (stop_status, stop_body) = send_json(
            &router,
            Method::POST,
            &format!("/api/tasks/{}/stop", task_id),
            None,
        )
        .await;
        assert_eq!(stop_status, StatusCode::OK);
        assert_eq!(stop_body["success"], true);

        // Register external checkpoint, get, delete
        svc.checkpoint_manager().register_external_checkpoint(
            "ckpt-api-1",
            50,
            1,
            "/tmp/ckpt-api-1.pt",
            4096,
            HashMap::new(),
        );

        let (ckpt_status, ckpt_body) =
            send_json(&router, Method::GET, "/api/checkpoints/ckpt-api-1", None).await;
        assert_eq!(ckpt_status, StatusCode::OK);
        assert_eq!(ckpt_body["step"], 50);

        let (del_ckpt_status, _) =
            send_json(&router, Method::DELETE, "/api/checkpoints/ckpt-api-1", None).await;
        assert_eq!(del_ckpt_status, StatusCode::OK);

        let (missing_ckpt, _) =
            send_json(&router, Method::GET, "/api/checkpoints/ckpt-api-1", None).await;
        assert_eq!(missing_ckpt, StatusCode::NOT_FOUND);

        // Filtered system logs endpoint
        let (sys_logs_status, sys_logs) = send_json(
            &router,
            Method::GET,
            "/api/logs?limit=10&source=task_manager",
            None,
        )
        .await;
        assert_eq!(sys_logs_status, StatusCode::OK);
        assert!(!sys_logs.as_array().unwrap().is_empty());
    }
}
