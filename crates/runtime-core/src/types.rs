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
}
