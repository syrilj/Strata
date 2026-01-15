//! Security middleware for the coordinator service
//!
//! Provides rate limiting, input validation, and request logging.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use parking_lot::RwLock;
use tonic::Status;
use tracing::debug;

/// Rate limiter using token bucket algorithm
pub struct RateLimiter {
    /// Requests per second limit
    rate: u64,
    /// Burst capacity
    burst: u64,
    /// Per-client buckets: client_id -> (tokens, last_update)
    buckets: DashMap<String, (AtomicU64, RwLock<Instant>)>,
    /// Cleanup interval
    cleanup_interval: Duration,
    /// Last cleanup time
    last_cleanup: RwLock<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `rate` - Requests per second
    /// * `burst` - Maximum burst capacity
    pub fn new(rate: u64, burst: u64) -> Self {
        Self {
            rate,
            burst,
            buckets: DashMap::new(),
            cleanup_interval: Duration::from_secs(60),
            last_cleanup: RwLock::new(Instant::now()),
        }
    }

    /// Check if a request should be allowed
    ///
    /// Returns Ok(()) if allowed, Err with retry-after duration if rate limited
    pub fn check(&self, client_id: &str) -> Result<(), Duration> {
        self.maybe_cleanup();

        let now = Instant::now();

        let entry = self.buckets.entry(client_id.to_string()).or_insert_with(|| {
            (AtomicU64::new(self.burst), RwLock::new(now))
        });

        let (tokens, last_update) = entry.value();

        // Calculate tokens to add based on time elapsed
        let elapsed = {
            let last = last_update.read();
            now.duration_since(*last)
        };

        let tokens_to_add = (elapsed.as_secs_f64() * self.rate as f64) as u64;

        if tokens_to_add > 0 {
            // Refill tokens
            let current = tokens.load(Ordering::Relaxed);
            let new_tokens = (current + tokens_to_add).min(self.burst);
            tokens.store(new_tokens, Ordering::Relaxed);
            *last_update.write() = now;
        }

        // Try to consume a token
        let current = tokens.load(Ordering::Relaxed);
        if current > 0 {
            tokens.fetch_sub(1, Ordering::Relaxed);
            Ok(())
        } else {
            // Calculate retry-after
            let retry_after = Duration::from_secs_f64(1.0 / self.rate as f64);
            Err(retry_after)
        }
    }

    /// Cleanup old entries
    fn maybe_cleanup(&self) {
        let now = Instant::now();
        let should_cleanup = {
            let last = self.last_cleanup.read();
            now.duration_since(*last) > self.cleanup_interval
        };

        if should_cleanup {
            *self.last_cleanup.write() = now;

            // Remove entries that haven't been used in a while
            let stale_threshold = Duration::from_secs(300);
            self.buckets.retain(|_, (_, last_update)| {
                let last = last_update.read();
                now.duration_since(*last) < stale_threshold
            });

            debug!(
                remaining_entries = self.buckets.len(),
                "Rate limiter cleanup completed"
            );
        }
    }
}

/// Input validator for coordinator requests
pub struct InputValidator {
    /// Maximum worker ID length
    max_worker_id_len: usize,
    /// Maximum dataset ID length
    max_dataset_id_len: usize,
    /// Maximum path length
    max_path_len: usize,
    /// Maximum metadata entries
    max_metadata_entries: usize,
    /// Maximum metadata value length
    max_metadata_value_len: usize,
    /// Allowed characters pattern for IDs
    id_pattern: regex::Regex,
}

impl Default for InputValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl InputValidator {
    /// Create a new input validator with default settings
    pub fn new() -> Self {
        Self {
            max_worker_id_len: 128,
            max_dataset_id_len: 256,
            max_path_len: 4096,
            max_metadata_entries: 64,
            max_metadata_value_len: 1024,
            // Allow alphanumeric, hyphens, underscores, and dots
            id_pattern: regex::Regex::new(r"^[a-zA-Z0-9_\-\.]+$").unwrap(),
        }
    }

    /// Validate a worker ID
    pub fn validate_worker_id(&self, id: &str) -> Result<(), Status> {
        if id.is_empty() {
            return Err(Status::invalid_argument("Worker ID cannot be empty"));
        }

        if id.len() > self.max_worker_id_len {
            return Err(Status::invalid_argument(format!(
                "Worker ID exceeds maximum length of {} characters",
                self.max_worker_id_len
            )));
        }

        if !self.id_pattern.is_match(id) {
            return Err(Status::invalid_argument(
                "Worker ID contains invalid characters. Only alphanumeric, hyphens, underscores, and dots are allowed"
            ));
        }

        Ok(())
    }

    /// Validate a dataset ID
    pub fn validate_dataset_id(&self, id: &str) -> Result<(), Status> {
        if id.is_empty() {
            return Err(Status::invalid_argument("Dataset ID cannot be empty"));
        }

        if id.len() > self.max_dataset_id_len {
            return Err(Status::invalid_argument(format!(
                "Dataset ID exceeds maximum length of {} characters",
                self.max_dataset_id_len
            )));
        }

        if !self.id_pattern.is_match(id) {
            return Err(Status::invalid_argument(
                "Dataset ID contains invalid characters"
            ));
        }

        Ok(())
    }

    /// Validate a file path
    pub fn validate_path(&self, path: &str) -> Result<(), Status> {
        if path.len() > self.max_path_len {
            return Err(Status::invalid_argument(format!(
                "Path exceeds maximum length of {} characters",
                self.max_path_len
            )));
        }

        // Check for path traversal attempts
        if path.contains("..") {
            return Err(Status::invalid_argument(
                "Path traversal sequences are not allowed"
            ));
        }

        // Check for null bytes
        if path.contains('\0') {
            return Err(Status::invalid_argument("Path contains null bytes"));
        }

        Ok(())
    }

    /// Validate metadata map
    pub fn validate_metadata(&self, metadata: &HashMap<String, String>) -> Result<(), Status> {
        if metadata.len() > self.max_metadata_entries {
            return Err(Status::invalid_argument(format!(
                "Metadata exceeds maximum of {} entries",
                self.max_metadata_entries
            )));
        }

        for (key, value) in metadata {
            if key.len() > 128 {
                return Err(Status::invalid_argument("Metadata key too long"));
            }

            if value.len() > self.max_metadata_value_len {
                return Err(Status::invalid_argument(format!(
                    "Metadata value for key '{}' exceeds maximum length",
                    key
                )));
            }
        }

        Ok(())
    }

    /// Validate numeric ranges
    pub fn validate_positive(&self, value: i64, field_name: &str) -> Result<(), Status> {
        if value < 0 {
            return Err(Status::invalid_argument(format!(
                "{} must be non-negative",
                field_name
            )));
        }
        Ok(())
    }

    /// Validate port number
    pub fn validate_port(&self, port: i32) -> Result<(), Status> {
        if port < 1 || port > 65535 {
            return Err(Status::invalid_argument(
                "Port must be between 1 and 65535"
            ));
        }
        Ok(())
    }
}

/// Request metrics collector
pub struct RequestMetrics {
    /// Total requests by method
    requests: DashMap<String, AtomicU64>,
    /// Errors by method
    errors: DashMap<String, AtomicU64>,
    /// Latency samples (method -> recent latencies in microseconds)
    latencies: DashMap<String, Vec<u64>>,
    /// Max latency samples to keep
    max_samples: usize,
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
            errors: DashMap::new(),
            latencies: DashMap::new(),
            max_samples: 1000,
        }
    }

    /// Record a request
    pub fn record_request(&self, method: &str) {
        self.requests
            .entry(method.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record an error
    pub fn record_error(&self, method: &str) {
        self.errors
            .entry(method.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record latency
    pub fn record_latency(&self, method: &str, latency_us: u64) {
        let mut entry = self.latencies.entry(method.to_string()).or_insert_with(Vec::new);
        if entry.len() >= self.max_samples {
            entry.remove(0);
        }
        entry.push(latency_us);
    }

    /// Get request count for a method
    pub fn get_request_count(&self, method: &str) -> u64 {
        self.requests
            .get(method)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get error count for a method
    pub fn get_error_count(&self, method: &str) -> u64 {
        self.errors
            .get(method)
            .map(|v| v.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Get p99 latency for a method in microseconds
    pub fn get_p99_latency(&self, method: &str) -> Option<u64> {
        self.latencies.get(method).and_then(|samples| {
            if samples.is_empty() {
                return None;
            }
            let mut sorted: Vec<_> = samples.iter().copied().collect();
            sorted.sort_unstable();
            let idx = (sorted.len() as f64 * 0.99) as usize;
            sorted.get(idx.min(sorted.len() - 1)).copied()
        })
    }

    /// Get summary of all metrics
    pub fn summary(&self) -> HashMap<String, (u64, u64, Option<u64>)> {
        let mut result = HashMap::new();

        for entry in self.requests.iter() {
            let method = entry.key().clone();
            let requests = entry.value().load(Ordering::Relaxed);
            let errors = self.get_error_count(&method);
            let p99 = self.get_p99_latency(&method);
            result.insert(method, (requests, errors, p99));
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_burst() {
        let limiter = RateLimiter::new(10, 5);

        // Should allow burst of 5
        for _ in 0..5 {
            assert!(limiter.check("client-1").is_ok());
        }

        // 6th request should be rate limited
        assert!(limiter.check("client-1").is_err());
    }

    #[test]
    fn test_rate_limiter_different_clients() {
        let limiter = RateLimiter::new(10, 2);

        // Each client has their own bucket
        assert!(limiter.check("client-1").is_ok());
        assert!(limiter.check("client-1").is_ok());
        assert!(limiter.check("client-1").is_err());

        // Different client should still have tokens
        assert!(limiter.check("client-2").is_ok());
    }

    #[test]
    fn test_input_validator_worker_id() {
        let validator = InputValidator::new();

        assert!(validator.validate_worker_id("worker-1").is_ok());
        assert!(validator.validate_worker_id("gpu_node_0").is_ok());
        assert!(validator.validate_worker_id("node.cluster.local").is_ok());

        assert!(validator.validate_worker_id("").is_err());
        assert!(validator.validate_worker_id("worker/1").is_err());
        assert!(validator.validate_worker_id("worker<script>").is_err());
    }

    #[test]
    fn test_input_validator_path() {
        let validator = InputValidator::new();

        assert!(validator.validate_path("/data/training").is_ok());
        assert!(validator.validate_path("s3://bucket/key").is_ok());

        assert!(validator.validate_path("/data/../etc/passwd").is_err());
        assert!(validator.validate_path("/data/file\0.txt").is_err());
    }

    #[test]
    fn test_request_metrics() {
        let metrics = RequestMetrics::new();

        metrics.record_request("register_worker");
        metrics.record_request("register_worker");
        metrics.record_error("register_worker");
        metrics.record_latency("register_worker", 1000);
        metrics.record_latency("register_worker", 2000);

        assert_eq!(metrics.get_request_count("register_worker"), 2);
        assert_eq!(metrics.get_error_count("register_worker"), 1);
        assert!(metrics.get_p99_latency("register_worker").is_some());
    }
}
