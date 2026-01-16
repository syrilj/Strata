//! Worker state and registry management

use crate::{Error, ResourceMetrics, Result, WorkerId};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// Worker state enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerState {
    /// Worker is initializing
    Initializing,

    /// Worker is idle, ready for work
    Idle,

    /// Worker is loading data
    LoadingData,

    /// Worker is training
    Training,

    /// Worker is writing a checkpoint
    Checkpointing,

    /// Worker is recovering from a failure
    Recovering,

    /// Worker encountered an error
    Error,

    /// Worker is disconnecting gracefully
    Disconnecting,

    /// Worker is dead (missed heartbeats)
    Dead,
}

impl WorkerState {
    /// Returns true if the worker is in an active state
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            WorkerState::Idle
                | WorkerState::LoadingData
                | WorkerState::Training
                | WorkerState::Checkpointing
                | WorkerState::Recovering
        )
    }

    /// Returns true if the worker can accept new work
    pub fn can_accept_work(&self) -> bool {
        matches!(self, WorkerState::Idle)
    }
}

/// Worker information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    /// Unique worker identifier
    pub id: WorkerId,

    /// Worker hostname
    pub hostname: String,

    /// Worker port
    pub port: u16,

    /// Worker rank in the distributed group
    pub rank: u32,

    /// Total number of workers (world size)
    pub world_size: u32,

    /// Number of GPUs available
    pub gpu_count: u32,

    /// Total memory in bytes
    pub memory_bytes: u64,

    /// Current state
    pub state: WorkerState,

    /// Last heartbeat timestamp
    pub last_heartbeat: DateTime<Utc>,

    /// Registration timestamp
    pub registered_at: DateTime<Utc>,

    /// Current training step
    pub current_step: u64,

    /// Current epoch
    pub current_epoch: u64,

    /// Current task description
    pub current_task: String,

    /// Resource metrics
    pub resources: ResourceMetrics,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl WorkerInfo {
    /// Create a new worker info
    pub fn new(id: WorkerId, hostname: String, port: u16, rank: u32, world_size: u32) -> Self {
        let now = Utc::now();
        Self {
            id,
            hostname,
            port,
            rank,
            world_size,
            gpu_count: 0,
            memory_bytes: 0,
            state: WorkerState::Initializing,
            last_heartbeat: now,
            registered_at: now,
            current_step: 0,
            current_epoch: 0,
            current_task: String::new(),
            resources: ResourceMetrics::default(),
            metadata: HashMap::new(),
        }
    }

    /// Update heartbeat timestamp and resources
    pub fn heartbeat(&mut self, resources: ResourceMetrics) {
        self.last_heartbeat = Utc::now();
        self.resources = resources;
    }

    /// Check if worker is considered dead based on timeout
    pub fn is_dead(&self, timeout: Duration) -> bool {
        let elapsed = Utc::now()
            .signed_duration_since(self.last_heartbeat)
            .to_std()
            .unwrap_or(Duration::MAX);
        elapsed > timeout
    }

    /// Get time since last heartbeat
    pub fn time_since_heartbeat(&self) -> Duration {
        Utc::now()
            .signed_duration_since(self.last_heartbeat)
            .to_std()
            .unwrap_or(Duration::ZERO)
    }
}

/// Thread-safe worker registry
pub struct WorkerRegistry {
    /// Map of worker ID to worker info
    workers: DashMap<WorkerId, WorkerInfo>,

    /// Counter for assigning ranks
    rank_counter: AtomicU64,

    /// Maximum workers allowed
    max_workers: usize,

    /// Heartbeat timeout duration
    heartbeat_timeout: Duration,
}

impl WorkerRegistry {
    /// Create a new worker registry
    pub fn new(max_workers: usize, heartbeat_timeout: Duration) -> Self {
        Self {
            workers: DashMap::new(),
            rank_counter: AtomicU64::new(0),
            max_workers,
            heartbeat_timeout,
        }
    }

    /// Register a new worker
    pub fn register(&self, mut worker: WorkerInfo) -> Result<WorkerInfo> {
        if self.workers.len() >= self.max_workers {
            return Err(Error::InvalidConfig {
                message: format!("Maximum workers ({}) reached", self.max_workers),
            });
        }

        if self.workers.contains_key(&worker.id) {
            return Err(Error::WorkerAlreadyRegistered {
                worker_id: worker.id.clone(),
            });
        }

        // Assign rank
        let rank = self.rank_counter.fetch_add(1, Ordering::SeqCst) as u32;
        worker.rank = rank;
        worker.state = WorkerState::Idle;

        info!(
            worker_id = %worker.id,
            rank = rank,
            hostname = %worker.hostname,
            "Worker registered"
        );

        let result = worker.clone();
        self.workers.insert(worker.id.clone(), worker);
        Ok(result)
    }

    /// Deregister a worker
    pub fn deregister(&self, worker_id: &str) -> Result<WorkerInfo> {
        self.workers
            .remove(worker_id)
            .map(|(_, w)| {
                info!(worker_id = %worker_id, "Worker deregistered");
                w
            })
            .ok_or_else(|| Error::WorkerNotFound {
                worker_id: worker_id.to_string(),
            })
    }

    /// Get worker info by ID
    pub fn get(&self, worker_id: &str) -> Option<WorkerInfo> {
        self.workers.get(worker_id).map(|w| w.clone())
    }

    /// Update worker heartbeat
    pub fn heartbeat(
        &self,
        worker_id: &str,
        state: WorkerState,
        resources: ResourceMetrics,
    ) -> Result<()> {
        let mut worker = self
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| Error::WorkerNotFound {
                worker_id: worker_id.to_string(),
            })?;

        worker.heartbeat(resources);
        worker.state = state;
        Ok(())
    }

    /// Update worker training progress
    pub fn update_progress(
        &self,
        worker_id: &str,
        step: u64,
        epoch: u64,
        task: Option<String>,
    ) -> Result<()> {
        let mut worker = self
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| Error::WorkerNotFound {
                worker_id: worker_id.to_string(),
            })?;

        worker.current_step = step;
        worker.current_epoch = epoch;
        if let Some(t) = task {
            worker.current_task = t;
        }
        Ok(())
    }

    /// Get all active workers
    pub fn active_workers(&self) -> Vec<WorkerInfo> {
        self.workers
            .iter()
            .filter(|entry| entry.value().state.is_active())
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get all workers
    pub fn all_workers(&self) -> Vec<WorkerInfo> {
        self.workers
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get current world size (number of registered workers)
    pub fn world_size(&self) -> usize {
        self.workers.len()
    }

    /// Check for dead workers and mark them
    pub fn check_dead_workers(&self) -> Vec<WorkerId> {
        let mut dead_workers = Vec::new();

        for mut entry in self.workers.iter_mut() {
            if entry.value().is_dead(self.heartbeat_timeout)
                && entry.value().state != WorkerState::Dead
            {
                warn!(
                    worker_id = %entry.key(),
                    last_heartbeat = ?entry.value().last_heartbeat,
                    "Worker marked as dead"
                );
                entry.value_mut().state = WorkerState::Dead;
                dead_workers.push(entry.key().clone());
            }
        }

        dead_workers
    }

    /// Remove all dead workers from registry
    pub fn remove_dead_workers(&self) -> Vec<WorkerInfo> {
        let dead_ids: Vec<_> = self
            .workers
            .iter()
            .filter(|entry| entry.value().state == WorkerState::Dead)
            .map(|entry| entry.key().clone())
            .collect();

        dead_ids
            .into_iter()
            .filter_map(|id| self.workers.remove(&id).map(|(_, w)| w))
            .collect()
    }

    /// Get aggregate resource metrics across all active workers
    pub fn aggregate_resources(&self) -> ResourceMetrics {
        let mut aggregate = ResourceMetrics::default();

        for entry in self.workers.iter() {
            if entry.value().state.is_active() {
                let resources = &entry.value().resources;
                aggregate.cpu_percent += resources.cpu_percent;
                aggregate.memory_used_bytes += resources.memory_used_bytes;
                aggregate.disk_read_bytes += resources.disk_read_bytes;
                aggregate.disk_write_bytes += resources.disk_write_bytes;
                aggregate.network_rx_bytes += resources.network_rx_bytes;
                aggregate.network_tx_bytes += resources.network_tx_bytes;

                for gpu in &resources.gpu_metrics {
                    aggregate.gpu_metrics.push(gpu.clone());
                }
            }
        }

        aggregate
    }
}

impl Default for WorkerRegistry {
    fn default() -> Self {
        Self::new(10000, Duration::from_secs(30))
    }
}

/// Thread-safe handle to worker registry
pub type WorkerRegistryHandle = Arc<WorkerRegistry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_registration() {
        let registry = WorkerRegistry::new(10, Duration::from_secs(30));

        let worker = WorkerInfo::new("worker-1".to_string(), "host1".to_string(), 50052, 0, 1);

        let registered = registry.register(worker).unwrap();
        assert_eq!(registered.rank, 0);
        assert_eq!(registry.world_size(), 1);
    }

    #[test]
    fn test_worker_heartbeat() {
        let registry = WorkerRegistry::new(10, Duration::from_secs(30));

        let worker = WorkerInfo::new("worker-1".to_string(), "host1".to_string(), 50052, 0, 1);
        registry.register(worker).unwrap();

        registry
            .heartbeat(
                "worker-1",
                WorkerState::Training,
                ResourceMetrics::default(),
            )
            .unwrap();

        let updated = registry.get("worker-1").unwrap();
        assert_eq!(updated.state, WorkerState::Training);
    }

    #[test]
    fn test_duplicate_registration() {
        let registry = WorkerRegistry::new(10, Duration::from_secs(30));

        let worker = WorkerInfo::new("worker-1".to_string(), "host1".to_string(), 50052, 0, 1);

        registry.register(worker.clone()).unwrap();
        let result = registry.register(worker);
        assert!(matches!(result, Err(Error::WorkerAlreadyRegistered { .. })));
    }
}
