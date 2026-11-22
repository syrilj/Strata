//! Runtime configuration types

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// Main runtime configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Coordinator settings
    pub coordinator: CoordinatorConfig,

    /// Worker settings
    pub worker: WorkerConfig,

    /// Checkpoint settings
    pub checkpoint: CheckpointConfig,

    /// Storage settings
    pub storage: StorageConfig,

    /// Network settings
    pub network: NetworkConfig,

    /// Logging settings
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl RuntimeConfig {
    /// Load configuration from environment variables with sensible defaults.
    ///
    /// Recognized variables:
    /// - `GRPC_ADDR` (e.g., `"0.0.0.0:50051"` -> sets `coordinator.bind_address` and `coordinator.port`)
    /// - `HTTP_ADDR` (e.g., `"0.0.0.0:51051"` -> sets `coordinator.http_address`)
    /// - `MAX_WORKERS`
    /// - `HEARTBEAT_INTERVAL_SECS`
    /// - `HEARTBEAT_TIMEOUT_SECS`
    /// - `DEAD_WORKER_CHECK_INTERVAL_SECS`
    /// - `COORDINATOR_ADDR`
    /// - `WORKER_ID`
    /// - `DEMO_MODE` (`"true"` / `"1"` / `"yes"`)
    /// - `STORAGE_BACKEND` (`"local"` or `"s3"`)
    /// - `CHECKPOINT_DIR` (sets `storage.base_path`)
    /// - `CHECKPOINT_BUCKET` (S3 bucket)
    /// - `CHECKPOINT_PREFIX` (S3 prefix)
    /// - `AWS_REGION` (default `"us-east-1"`)
    /// - `MINIO_ENDPOINT` / `S3_ENDPOINT_URL`
    /// - `S3_FORCE_PATH_STYLE`
    /// - `MAX_CHECKPOINTS_KEPT`
    /// - `CHECKPOINT_COMPRESSION` (`"true"` / `"false"`)
    /// - `CHECKPOINT_COMPRESSION_LEVEL`
    /// - `WRITE_BUFFER_SIZE`
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(grpc_addr) = std::env::var("GRPC_ADDR") {
            let trimmed = grpc_addr.trim();
            if !trimmed.is_empty() {
                if let Some((host, port_str)) = trimmed.rsplit_once(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        config.coordinator.bind_address = host.to_string();
                        config.coordinator.port = port;
                    } else {
                        config.coordinator.bind_address = trimmed.to_string();
                    }
                } else {
                    config.coordinator.bind_address = trimmed.to_string();
                }
            }
        }

        if let Ok(http_addr) = std::env::var("HTTP_ADDR") {
            let trimmed = http_addr.trim();
            if !trimmed.is_empty() {
                config.coordinator.http_address = Some(trimmed.to_string());
            }
        }

        if let Ok(max_workers) = std::env::var("MAX_WORKERS") {
            if let Ok(v) = max_workers.parse::<usize>() {
                config.coordinator.max_workers = v;
            }
        }

        if let Ok(hb_interval) = std::env::var("HEARTBEAT_INTERVAL_SECS") {
            if let Ok(secs) = hb_interval.parse::<u64>() {
                config.worker.heartbeat_interval = Duration::from_secs(secs);
            }
        }

        if let Ok(hb_timeout) = std::env::var("HEARTBEAT_TIMEOUT_SECS") {
            if let Ok(secs) = hb_timeout.parse::<u64>() {
                config.coordinator.heartbeat_timeout = Duration::from_secs(secs);
            }
        }

        if let Ok(check_interval) = std::env::var("DEAD_WORKER_CHECK_INTERVAL_SECS") {
            if let Ok(secs) = check_interval.parse::<u64>() {
                config.coordinator.dead_worker_check_interval = Duration::from_secs(secs);
            }
        }

        if let Ok(coord_addr) = std::env::var("COORDINATOR_ADDR") {
            if !coord_addr.trim().is_empty() {
                config.worker.coordinator_address = coord_addr;
            }
        }

        if let Ok(worker_id) = std::env::var("WORKER_ID") {
            if !worker_id.trim().is_empty() {
                config.worker.worker_id = Some(worker_id);
            }
        }

        if let Ok(demo) = std::env::var("DEMO_MODE") {
            config.coordinator.demo_mode = parse_bool_env(&demo, config.coordinator.demo_mode);
        }

        if let Ok(dir) = std::env::var("CHECKPOINT_DIR") {
            if !dir.trim().is_empty() {
                config.storage.base_path = dir;
            }
        }

        if let Ok(prefix) = std::env::var("CHECKPOINT_PREFIX") {
            if !prefix.trim().is_empty() {
                config.storage.s3_prefix = Some(prefix);
            }
        }

        if let Ok(force_path) = std::env::var("S3_FORCE_PATH_STYLE") {
            config.storage.s3_force_path_style =
                parse_bool_env(&force_path, config.storage.s3_force_path_style);
        }

        if let Ok(backend) = std::env::var("STORAGE_BACKEND") {
            match backend.trim().to_ascii_lowercase().as_str() {
                "s3" => {
                    let bucket = std::env::var("CHECKPOINT_BUCKET").unwrap_or_default();
                    let region = std::env::var("AWS_REGION")
                        .ok()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| "us-east-1".to_string());
                    let endpoint = std::env::var("MINIO_ENDPOINT")
                        .or_else(|_| std::env::var("S3_ENDPOINT_URL"))
                        .ok()
                        .filter(|s| !s.trim().is_empty());
                    if endpoint.is_some() && std::env::var("S3_FORCE_PATH_STYLE").is_err() {
                        config.storage.s3_force_path_style = true;
                    }
                    config.storage.backend = StorageBackend::S3 {
                        endpoint,
                        region,
                        bucket,
                    };
                }
                "local" | "" => {
                    config.storage.backend = StorageBackend::Local;
                }
                _ => {}
            }
        } else if let Ok(bucket) = std::env::var("CHECKPOINT_BUCKET") {
            if !bucket.trim().is_empty() {
                let region = std::env::var("AWS_REGION")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| "us-east-1".to_string());
                let endpoint = std::env::var("MINIO_ENDPOINT")
                    .or_else(|_| std::env::var("S3_ENDPOINT_URL"))
                    .ok()
                    .filter(|s| !s.trim().is_empty());
                config.storage.backend = StorageBackend::S3 {
                    endpoint,
                    region,
                    bucket,
                };
            }
        }

        if let Ok(keep) = std::env::var("MAX_CHECKPOINTS_KEPT") {
            if let Ok(v) = keep.parse::<usize>() {
                config.checkpoint.keep_count = v;
            }
        }

        if let Ok(comp) = std::env::var("CHECKPOINT_COMPRESSION") {
            config.checkpoint.compression = parse_bool_env(&comp, config.checkpoint.compression);
        }

        if let Ok(level) = std::env::var("CHECKPOINT_COMPRESSION_LEVEL") {
            if let Ok(v) = level.parse::<u32>() {
                config.checkpoint.compression_level = v;
            }
        }

        if let Ok(buf_size) = std::env::var("WRITE_BUFFER_SIZE") {
            if let Ok(v) = buf_size.parse::<usize>() {
                config.checkpoint.write_buffer_size = v;
            }
        }

        config
    }

    /// Load configuration from a TOML file.
    ///
    /// Supports both direct `RuntimeConfig` TOML serialization and the
    /// `config/production.toml` schema (`[coordinator]`, `[storage]`,
    /// `[storage.local]`, `[storage.s3]`, `[checkpoint]`, `[logging]`).
    pub fn from_toml_file(path: impl AsRef<Path>) -> Result<Self> {
        let path_ref = path.as_ref();
        let contents = std::fs::read_to_string(path_ref).map_err(|e| Error::InvalidConfig {
            message: format!("Failed to read config file {}: {}", path_ref.display(), e),
        })?;

        Self::from_toml_str(&contents)
    }

    /// Parse configuration from a TOML string.
    pub fn from_toml_str(contents: &str) -> Result<Self> {
        // First try direct RuntimeConfig deserialization if all core sections are shaped as RuntimeConfig
        if let Ok(cfg) = toml::from_str::<RuntimeConfig>(contents) {
            return Ok(cfg);
        }

        let prod: ProductionToml = toml::from_str(contents).map_err(|e| Error::InvalidConfig {
            message: format!("Failed to parse TOML config: {}", e),
        })?;

        let mut config = Self::default();

        if let Some(coord) = prod.coordinator {
            if let Some(grpc_addr) = coord.grpc_address {
                if let Some((host, port_str)) = grpc_addr.rsplit_once(':') {
                    if let Ok(port) = port_str.parse::<u16>() {
                        config.coordinator.bind_address = host.to_string();
                        config.coordinator.port = port;
                    } else {
                        config.coordinator.bind_address = grpc_addr;
                    }
                } else {
                    config.coordinator.bind_address = grpc_addr;
                }
            }
            if let Some(bind) = coord.bind_address {
                config.coordinator.bind_address = bind;
            }
            if let Some(port) = coord.port {
                config.coordinator.port = port;
            }
            if let Some(http_addr) = coord.http_address {
                config.coordinator.http_address = Some(http_addr);
            }
            if let Some(max_workers) = coord.max_workers {
                config.coordinator.max_workers = max_workers;
            }
            if let Some(hb_timeout) = coord.heartbeat_timeout {
                config.coordinator.heartbeat_timeout = Duration::from_secs(hb_timeout);
            }
            if let Some(check_interval) = coord.dead_worker_check_interval {
                config.coordinator.dead_worker_check_interval = Duration::from_secs(check_interval);
            }
            if let Some(demo_mode) = coord.demo_mode {
                config.coordinator.demo_mode = demo_mode;
            }
        }

        if let Some(worker) = prod.worker {
            if let Some(addr) = worker.coordinator_address {
                config.worker.coordinator_address = addr;
            }
            if let Some(id) = worker.worker_id {
                config.worker.worker_id = Some(id);
            }
            if let Some(hb) = worker.heartbeat_interval {
                config.worker.heartbeat_interval = Duration::from_secs(hb);
            }
            if let Some(io_threads) = worker.io_threads {
                config.worker.io_threads = io_threads;
            }
            if let Some(prefetch) = worker.prefetch_buffer_size {
                config.worker.prefetch_buffer_size = prefetch;
            }
        }

        if let Some(storage) = prod.storage {
            if let Some(local) = &storage.local {
                if let Some(path) = &local.path {
                    config.storage.base_path = path.clone();
                }
            }
            if let Some(base_path) = storage.base_path {
                config.storage.base_path = base_path;
            }
            if let Some(max_ops) = storage.max_concurrent_ops {
                config.storage.max_concurrent_ops = max_ops;
            }

            let backend_kind = storage
                .backend
                .as_deref()
                .unwrap_or("local")
                .to_ascii_lowercase();

            if backend_kind == "s3" {
                let s3 = storage.s3.unwrap_or_default();
                config.storage.s3_prefix = s3.prefix;
                config.storage.s3_force_path_style = s3.force_path_style.unwrap_or(false);
                config.storage.backend = StorageBackend::S3 {
                    endpoint: s3.endpoint_url,
                    region: s3.region.unwrap_or_else(|| "us-east-1".to_string()),
                    bucket: s3.bucket.unwrap_or_default(),
                };
            } else {
                config.storage.backend = StorageBackend::Local;
            }
        }

        if let Some(ckpt) = prod.checkpoint {
            if let Some(keep_count) = ckpt.keep_count {
                config.checkpoint.keep_count = keep_count;
            }
            if let Some(write_buffer_size) = ckpt.write_buffer_size {
                config.checkpoint.write_buffer_size = write_buffer_size;
            }
            if let Some(compression) = ckpt.compression {
                config.checkpoint.compression = compression;
            }
            if let Some(compression_level) = ckpt.compression_level {
                config.checkpoint.compression_level = compression_level;
            }
            if let Some(write_timeout) = ckpt.write_timeout {
                config.checkpoint.write_timeout = Duration::from_secs(write_timeout);
            }
        }

        if let Some(logging) = prod.logging {
            if let Some(level) = logging.level {
                config.logging.level = level;
            }
            if let Some(json) = logging.json {
                config.logging.json = json;
            }
        }

        Ok(config)
    }
}

fn parse_bool_env(val: &str, default: bool) -> bool {
    match val.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        _ => default,
    }
}

#[derive(Debug, Deserialize)]
struct ProductionToml {
    coordinator: Option<ProductionCoordinatorToml>,
    worker: Option<ProductionWorkerToml>,
    storage: Option<ProductionStorageToml>,
    checkpoint: Option<ProductionCheckpointToml>,
    logging: Option<ProductionLoggingToml>,
}

#[derive(Debug, Deserialize)]
struct ProductionCoordinatorToml {
    grpc_address: Option<String>,
    bind_address: Option<String>,
    port: Option<u16>,
    http_address: Option<String>,
    max_workers: Option<usize>,
    heartbeat_timeout: Option<u64>,
    dead_worker_check_interval: Option<u64>,
    demo_mode: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProductionWorkerToml {
    coordinator_address: Option<String>,
    worker_id: Option<String>,
    heartbeat_interval: Option<u64>,
    io_threads: Option<usize>,
    prefetch_buffer_size: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ProductionStorageToml {
    backend: Option<String>,
    base_path: Option<String>,
    max_concurrent_ops: Option<usize>,
    local: Option<ProductionLocalStorageToml>,
    s3: Option<ProductionS3StorageToml>,
}

#[derive(Debug, Deserialize)]
struct ProductionLocalStorageToml {
    path: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProductionS3StorageToml {
    bucket: Option<String>,
    prefix: Option<String>,
    region: Option<String>,
    endpoint_url: Option<String>,
    force_path_style: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProductionCheckpointToml {
    keep_count: Option<usize>,
    write_buffer_size: Option<usize>,
    compression: Option<bool>,
    compression_level: Option<u32>,
    write_timeout: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ProductionLoggingToml {
    level: Option<String>,
    json: Option<bool>,
}

/// Coordinator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    /// Address to bind the coordinator server
    pub bind_address: String,

    /// Port for gRPC server
    pub port: u16,

    /// HTTP API address for dashboard
    #[serde(default)]
    pub http_address: Option<String>,

    /// Maximum number of workers
    pub max_workers: usize,

    /// Worker heartbeat timeout
    #[serde(with = "humantime_serde")]
    pub heartbeat_timeout: Duration,

    /// How often to check for dead workers
    #[serde(with = "humantime_serde")]
    pub dead_worker_check_interval: Duration,

    /// Whether demo mode is enabled
    #[serde(default)]
    pub demo_mode: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 50051,
            http_address: Some("0.0.0.0:51051".to_string()),
            max_workers: 10000,
            heartbeat_timeout: Duration::from_secs(30),
            dead_worker_check_interval: Duration::from_secs(5),
            demo_mode: false,
        }
    }
}

/// Worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Coordinator address to connect to
    pub coordinator_address: String,

    /// Worker identifier (auto-generated if not set)
    pub worker_id: Option<String>,

    /// Heartbeat interval
    #[serde(with = "humantime_serde")]
    pub heartbeat_interval: Duration,

    /// Number of async I/O threads
    pub io_threads: usize,

    /// Size of data prefetch buffer
    pub prefetch_buffer_size: usize,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            coordinator_address: "localhost:50051".to_string(),
            worker_id: None,
            heartbeat_interval: Duration::from_secs(5),
            io_threads: 4,
            prefetch_buffer_size: 16,
        }
    }
}

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Checkpoint strategy
    pub strategy: CheckpointStrategy,

    /// Number of checkpoints to keep
    pub keep_count: usize,

    /// Async write buffer size in bytes
    pub write_buffer_size: usize,

    /// Enable compression
    pub compression: bool,

    /// Compression level (1-9)
    pub compression_level: u32,

    /// Write timeout
    #[serde(with = "humantime_serde")]
    pub write_timeout: Duration,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            strategy: CheckpointStrategy::default(),
            keep_count: 5,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            compression: true,
            compression_level: 3,
            write_timeout: Duration::from_secs(300),
        }
    }
}

/// Checkpoint strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckpointStrategy {
    /// Checkpoint every N steps
    Steps { interval: u64 },

    /// Checkpoint every N seconds
    Time {
        #[serde(with = "humantime_serde")]
        interval: Duration,
    },

    /// Adaptive: checkpoint based on loss improvement
    Adaptive {
        min_steps: u64,
        max_steps: u64,
        loss_threshold: f64,
    },

    /// No automatic checkpointing (manual only)
    Manual,
}

impl Default for CheckpointStrategy {
    fn default() -> Self {
        CheckpointStrategy::Steps { interval: 1000 }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type
    pub backend: StorageBackend,

    /// Base path for storage
    pub base_path: String,

    /// Optional S3 prefix
    #[serde(default)]
    pub s3_prefix: Option<String>,

    /// Force path-style S3 addressing (e.g., for MinIO)
    #[serde(default)]
    pub s3_force_path_style: bool,

    /// Maximum concurrent I/O operations
    pub max_concurrent_ops: usize,

    /// Retry configuration
    pub retry: RetryConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            base_path: "./data".to_string(),
            s3_prefix: None,
            s3_force_path_style: false,
            max_concurrent_ops: 64,
            retry: RetryConfig::default(),
        }
    }
}

/// Storage backend type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageBackend {
    /// Local filesystem
    Local,

    /// S3-compatible storage
    S3 {
        endpoint: Option<String>,
        region: String,
        bucket: String,
    },
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retries
    pub max_retries: u32,

    /// Initial delay before first retry
    #[serde(with = "humantime_serde")]
    pub initial_delay: Duration,

    /// Maximum delay between retries
    #[serde(with = "humantime_serde")]
    pub max_delay: Duration,

    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,

    /// Add jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Connection timeout
    #[serde(with = "humantime_serde")]
    pub connect_timeout: Duration,

    /// Request timeout
    #[serde(with = "humantime_serde")]
    pub request_timeout: Duration,

    /// Keep-alive interval
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable TLS
    pub tls_enabled: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            keepalive_interval: Duration::from_secs(10),
            max_message_size: 256 * 1024 * 1024, // 256MB
            tls_enabled: false,
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level filter
    pub level: String,
    /// Enable JSON log format
    pub json: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            json: false,
        }
    }
}

/// Duration serialization helper for human-readable formats
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert_eq!(config.coordinator.port, 50051);
        assert_eq!(config.checkpoint.keep_count, 5);
    }

    #[test]
    fn test_config_serialization() {
        let config = RuntimeConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RuntimeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.coordinator.port, config.coordinator.port);
    }

    #[test]
    fn test_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let keys = [
            "GRPC_ADDR",
            "HTTP_ADDR",
            "STORAGE_BACKEND",
            "CHECKPOINT_DIR",
            "CHECKPOINT_BUCKET",
            "AWS_REGION",
            "MINIO_ENDPOINT",
            "DEMO_MODE",
            "HEARTBEAT_INTERVAL_SECS",
            "HEARTBEAT_TIMEOUT_SECS",
            "MAX_CHECKPOINTS_KEPT",
            "CHECKPOINT_COMPRESSION",
        ];

        for k in keys {
            std::env::remove_var(k);
        }

        std::env::set_var("GRPC_ADDR", "127.0.0.1:60051");
        std::env::set_var("HTTP_ADDR", "127.0.0.1:61051");
        std::env::set_var("STORAGE_BACKEND", "s3");
        std::env::set_var("CHECKPOINT_DIR", "/tmp/ckpts");
        std::env::set_var("CHECKPOINT_BUCKET", "my-bucket");
        std::env::set_var("AWS_REGION", "us-west-2");
        std::env::set_var("MINIO_ENDPOINT", "http://localhost:9000");
        std::env::set_var("DEMO_MODE", "true");
        std::env::set_var("HEARTBEAT_INTERVAL_SECS", "12");
        std::env::set_var("HEARTBEAT_TIMEOUT_SECS", "45");
        std::env::set_var("MAX_CHECKPOINTS_KEPT", "10");
        std::env::set_var("CHECKPOINT_COMPRESSION", "false");

        let cfg = RuntimeConfig::from_env();
        assert_eq!(cfg.coordinator.bind_address, "127.0.0.1");
        assert_eq!(cfg.coordinator.port, 60051);
        assert_eq!(
            cfg.coordinator.http_address.as_deref(),
            Some("127.0.0.1:61051")
        );
        assert!(cfg.coordinator.demo_mode);
        assert_eq!(cfg.worker.heartbeat_interval, Duration::from_secs(12));
        assert_eq!(cfg.coordinator.heartbeat_timeout, Duration::from_secs(45));
        assert_eq!(cfg.storage.base_path, "/tmp/ckpts");
        assert!(cfg.storage.s3_force_path_style);
        assert_eq!(
            cfg.storage.backend,
            StorageBackend::S3 {
                endpoint: Some("http://localhost:9000".to_string()),
                region: "us-west-2".to_string(),
                bucket: "my-bucket".to_string(),
            }
        );
        assert_eq!(cfg.checkpoint.keep_count, 10);
        assert!(!cfg.checkpoint.compression);

        for k in keys {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn test_from_toml_production_format() {
        let toml_str = r#"
[coordinator]
grpc_address = "127.0.0.1:50055"
http_address = "127.0.0.1:51055"
max_workers = 500
heartbeat_timeout = 45

[storage]
backend = "s3"

[storage.local]
path = "./custom_ckpts"

[storage.s3]
bucket = "prod-bucket"
prefix = "runs/"
region = "eu-west-1"
endpoint_url = "http://minio:9000"
force_path_style = true

[checkpoint]
keep_count = 8
write_buffer_size = 33554432
compression = false
compression_level = 5

[logging]
level = "debug"
json = true
"#;

        let cfg = RuntimeConfig::from_toml_str(toml_str).unwrap();
        assert_eq!(cfg.coordinator.bind_address, "127.0.0.1");
        assert_eq!(cfg.coordinator.port, 50055);
        assert_eq!(
            cfg.coordinator.http_address.as_deref(),
            Some("127.0.0.1:51055")
        );
        assert_eq!(cfg.coordinator.max_workers, 500);
        assert_eq!(cfg.coordinator.heartbeat_timeout, Duration::from_secs(45));
        assert_eq!(cfg.storage.base_path, "./custom_ckpts");
        assert_eq!(cfg.storage.s3_prefix.as_deref(), Some("runs/"));
        assert!(cfg.storage.s3_force_path_style);
        assert_eq!(
            cfg.storage.backend,
            StorageBackend::S3 {
                endpoint: Some("http://minio:9000".to_string()),
                region: "eu-west-1".to_string(),
                bucket: "prod-bucket".to_string(),
            }
        );
        assert_eq!(cfg.checkpoint.keep_count, 8);
        assert_eq!(cfg.checkpoint.write_buffer_size, 33554432);
        assert!(!cfg.checkpoint.compression);
        assert_eq!(cfg.checkpoint.compression_level, 5);
    }

    #[test]
    fn test_from_toml_file_repo_production() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let prod_path = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("config/production.toml");
        let cfg = RuntimeConfig::from_toml_file(prod_path).unwrap();
        assert_eq!(cfg.coordinator.port, 50051);
        assert_eq!(cfg.coordinator.max_workers, 1000);
        assert_eq!(cfg.storage.base_path, "./checkpoints");
        assert!(matches!(
            cfg.storage.backend,
            StorageBackend::S3 { ref bucket, .. } if bucket == "your-checkpoint-bucket"
        ));
    }
}
