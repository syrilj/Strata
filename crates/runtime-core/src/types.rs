//! Core type definitions for the distributed training runtime

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier types
pub type WorkerId = String;
pub type DatasetId = String;
pub type ShardId = u64;
pub type CheckpointId = String;
pub type BarrierId = String;
pub type JobId = String;

/// Training step and epoch counters
pub type Step = u64;
pub type Epoch = u64;

/// Checkpoint metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointMetadata {
    /// Unique checkpoint identifier
    pub id: CheckpointId,

    /// Training step at checkpoint
    pub step: Step,

    /// Training epoch at checkpoint
    pub epoch: Epoch,

    /// Storage path
    pub path: String,

    /// Checkpoint size in bytes
    pub size_bytes: u64,

    /// Timestamp when checkpoint was created
    pub created_at: DateTime<Utc>,

    /// Checkpoint type
    pub checkpoint_type: CheckpointType,

    /// Model hash for verification
    pub model_hash: Option<String>,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Checkpoint type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckpointType {
    /// Full checkpoint with all state
    Full,

    /// Incremental checkpoint (delta from previous)
    Incremental,

    /// Optimizer state only
    OptimizerOnly,

    /// Model weights only
    ModelOnly,
}

/// Dataset metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    /// Unique dataset identifier
    pub id: DatasetId,

    /// Storage path
    pub path: String,

    /// Data format (parquet, tfrecord, webdataset, etc.)
    pub format: String,

    /// Total number of samples
    pub total_samples: u64,

    /// Number of shards
    pub total_shards: u64,

    /// Samples per shard
    pub shard_size: u64,

    /// Whether to shuffle
    pub shuffle: bool,

    /// Random seed for shuffling
    pub seed: u64,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Shard assignment for a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardAssignment {
    /// Dataset identifier
    pub dataset_id: DatasetId,

    /// Assigned shard ID
    pub shard_id: ShardId,

    /// Total shards in dataset
    pub total_shards: u64,

    /// Start sample index (inclusive)
    pub start_index: u64,

    /// End sample index (exclusive)
    pub end_index: u64,

    /// File paths for this shard
    pub file_paths: Vec<String>,

    /// Current epoch
    pub epoch: Epoch,
}

/// Barrier state for synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarrierState {
    /// Barrier identifier
    pub id: BarrierId,

    /// Training step for this barrier
    pub step: Step,

    /// Expected number of participants
    pub expected_participants: usize,

    /// Workers that have arrived
    pub arrived_workers: Vec<WorkerId>,

    /// Whether barrier has been released
    pub released: bool,

    /// Timestamp when barrier was created
    pub created_at: DateTime<Utc>,

    /// Timestamp when barrier was released
    pub released_at: Option<DateTime<Utc>>,
}

impl BarrierState {
    /// Create a new barrier
    pub fn new(id: BarrierId, step: Step, expected_participants: usize) -> Self {
        Self {
            id,
            step,
            expected_participants,
            arrived_workers: Vec::new(),
            released: false,
            created_at: Utc::now(),
            released_at: None,
        }
    }

    /// Record a worker arrival, returns true if barrier should be released
    pub fn arrive(&mut self, worker_id: WorkerId) -> bool {
        if !self.arrived_workers.contains(&worker_id) {
            self.arrived_workers.push(worker_id);
        }

        if self.arrived_workers.len() >= self.expected_participants && !self.released {
            self.released = true;
            self.released_at = Some(Utc::now());
            return true;
        }

        false
    }

    /// Get arrival order for a worker (1-indexed)
    pub fn arrival_order(&self, worker_id: &str) -> Option<usize> {
        self.arrived_workers
            .iter()
            .position(|w| w == worker_id)
            .map(|pos| pos + 1)
    }
}

/// Resource usage metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// CPU utilization percentage
    pub cpu_percent: f64,

    /// Memory used in bytes
    pub memory_used_bytes: u64,

    /// GPU metrics
    pub gpu_metrics: Vec<GpuMetrics>,

    /// Disk read bytes since last report
    pub disk_read_bytes: u64,

    /// Disk write bytes since last report
    pub disk_write_bytes: u64,

    /// Network bytes received since last report
    pub network_rx_bytes: u64,

    /// Network bytes transmitted since last report
    pub network_tx_bytes: u64,
}

/// GPU metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuMetrics {
    /// GPU device ID
    pub gpu_id: u32,

    /// GPU utilization percentage
    pub utilization_percent: f64,

    /// GPU memory used in bytes
    pub memory_used_bytes: u64,

    /// GPU memory total in bytes
    pub memory_total_bytes: u64,

    /// GPU temperature in Celsius
    pub temperature_celsius: f64,
}

/// Training progress summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingProgress {
    /// Current step
    pub step: Step,

    /// Current epoch
    pub epoch: Epoch,

    /// Total steps
    pub total_steps: Option<Step>,

    /// Total epochs
    pub total_epochs: Option<Epoch>,

    /// Training loss (if available)
    pub loss: Option<f64>,

    /// Learning rate
    pub learning_rate: Option<f64>,

    /// Samples processed per second
    pub samples_per_second: Option<f64>,

    /// Steps per second
    pub steps_per_second: Option<f64>,

    /// Estimated time remaining in seconds
    pub eta_seconds: Option<f64>,
}

/// Structured log event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEvent {
    /// Unique event identifier
    pub id: String,

    /// Unix timestamp in milliseconds
    pub timestamp: i64,

    /// Log level (e.g., "info", "warn", "error", "debug")
    pub level: String,

    /// Source component (e.g., "coordinator", "checkpoint", "shard_manager")
    pub source: String,

    /// Human-readable log message
    pub message: String,

    /// Optional associated task ID
    pub task_id: Option<String>,
}

/// Thread-safe bounded event log
#[derive(Debug)]
pub struct EventLog {
    events: parking_lot::RwLock<std::collections::VecDeque<LogEvent>>,
    capacity: usize,
    next_seq: std::sync::atomic::AtomicU64,
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl EventLog {
    /// Create a new bounded event log with the given capacity (minimum 1)
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            events: parking_lot::RwLock::new(std::collections::VecDeque::with_capacity(cap)),
            capacity: cap,
            next_seq: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Push a new event into the log, evicting the oldest if at capacity
    pub fn push(
        &self,
        level: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
        task_id: Option<String>,
    ) -> LogEvent {
        let seq = self
            .next_seq
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let event = LogEvent {
            id: format!("evt_{}", seq),
            timestamp: Utc::now().timestamp_millis(),
            level: level.into(),
            source: source.into(),
            message: message.into(),
            task_id,
        };

        let mut events = self.events.write();
        if events.len() >= self.capacity {
            events.pop_front();
        }
        events.push_back(event.clone());
        event
    }

    /// Return up to `limit` most recent events (newest first)
    pub fn recent(&self, limit: usize) -> Vec<LogEvent> {
        if limit == 0 {
            return Vec::new();
        }
        let events = self.events.read();
        events.iter().rev().take(limit).cloned().collect()
    }

    /// Return up to `limit` most recent events matching optional filters (newest first)
    pub fn filtered(
        &self,
        level: Option<&str>,
        source: Option<&str>,
        task_id: Option<&str>,
        limit: usize,
    ) -> Vec<LogEvent> {
        if limit == 0 {
            return Vec::new();
        }
        let events = self.events.read();
        events
            .iter()
            .rev()
            .filter(|e| {
                level.is_none_or(|l| e.level.eq_ignore_ascii_case(l))
                    && source.is_none_or(|s| e.source == s)
                    && task_id.is_none_or(|t| e.task_id.as_deref() == Some(t))
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Remove all events from the log
    pub fn clear(&self) {
        self.events.write().clear();
    }

    /// Current number of stored events
    pub fn len(&self) -> usize {
        self.events.read().len()
    }

    /// Returns true if the event log has no entries
    pub fn is_empty(&self) -> bool {
        self.events.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_barrier_state() {
        let mut barrier = BarrierState::new("barrier-1".to_string(), 100, 3);

        assert!(!barrier.arrive("worker-1".to_string()));
        assert!(!barrier.arrive("worker-2".to_string()));
        assert!(barrier.arrive("worker-3".to_string()));
        assert!(barrier.released);
        assert_eq!(barrier.arrival_order("worker-1"), Some(1));
        assert_eq!(barrier.arrival_order("worker-2"), Some(2));
        assert_eq!(barrier.arrival_order("worker-3"), Some(3));
    }

    #[test]
    fn test_event_log_bounded_and_filtered() {
        let log = EventLog::new(3);
        assert!(log.is_empty());

        log.push("info", "coordinator", "msg 1", None);
        log.push("warn", "checkpoint", "msg 2", Some("task-1".to_string()));
        log.push("error", "coordinator", "msg 3", Some("task-1".to_string()));
        log.push("info", "shard_manager", "msg 4", Some("task-2".to_string()));

        assert_eq!(log.len(), 3);
        let recent = log.recent(10);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].message, "msg 4");
        assert_eq!(recent[2].message, "msg 2");

        let by_task = log.filtered(None, None, Some("task-1"), 10);
        assert_eq!(by_task.len(), 2);
        assert_eq!(by_task[0].message, "msg 3");

        let by_level_source = log.filtered(Some("ERROR"), Some("coordinator"), None, 10);
        assert_eq!(by_level_source.len(), 1);
        assert_eq!(by_level_source[0].message, "msg 3");

        log.clear();
        assert!(log.is_empty());
    }
}
