//! Async runtime manager

use crate::{Error, Result, RuntimeConfig, WorkerRegistry, WorkerRegistryHandle};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::broadcast;
use tracing::{error, info};

/// Shutdown signal sender
pub type ShutdownSender = broadcast::Sender<()>;

/// Shutdown signal receiver
pub type ShutdownReceiver = broadcast::Receiver<()>;

/// Runtime manager for coordinating async operations
pub struct RuntimeManager {
    /// Tokio runtime
    runtime: Option<Runtime>,

    /// Configuration
    config: RuntimeConfig,

    /// Worker registry
    worker_registry: WorkerRegistryHandle,

    /// Shutdown signal sender
    shutdown_tx: ShutdownSender,
}

impl RuntimeManager {
    /// Create a new runtime manager
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(config.worker.io_threads)
            .enable_all()
            .thread_name("dtruntime-worker")
            .build()
            .map_err(|e| Error::Internal {
                message: format!("Failed to build Tokio runtime: {}", e),
            })?;

        let worker_registry = Arc::new(WorkerRegistry::new(
            config.coordinator.max_workers,
            config.coordinator.heartbeat_timeout,
        ));

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            runtime: Some(runtime),
            config,
            worker_registry,
            shutdown_tx,
        })
    }

    /// Get a reference to the Tokio runtime
    pub fn runtime(&self) -> &Runtime {
        self.runtime.as_ref().expect("Runtime should exist")
    }

    /// Get the runtime handle for spawning tasks
    pub fn handle(&self) -> tokio::runtime::Handle {
        self.runtime().handle().clone()
    }

    /// Get worker registry handle
    pub fn worker_registry(&self) -> WorkerRegistryHandle {
        Arc::clone(&self.worker_registry)
    }

    /// Get configuration
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Get a shutdown receiver
    pub fn shutdown_receiver(&self) -> ShutdownReceiver {
        self.shutdown_tx.subscribe()
    }

    /// Signal shutdown to all components
    pub fn shutdown(&self) {
        info!("Initiating runtime shutdown");
        let _ = self.shutdown_tx.send(());
    }

    /// Block on a future until completion
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime().block_on(future)
    }

    /// Spawn a task on the runtime
    pub fn spawn<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.runtime().spawn(future)
    }

    /// Run the dead worker check loop
    pub async fn run_dead_worker_check(&self) {
        let registry = self.worker_registry();
        let interval = self.config.coordinator.dead_worker_check_interval;
        let mut shutdown_rx = self.shutdown_receiver();

        info!(
            interval_secs = interval.as_secs(),
            "Starting dead worker check loop"
        );

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    let dead = registry.check_dead_workers();
                    if !dead.is_empty() {
                        info!(count = dead.len(), "Detected dead workers");
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Dead worker check loop shutting down");
                    break;
                }
            }
        }
    }
}

impl Drop for RuntimeManager {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            // Signal shutdown
            let _ = self.shutdown_tx.send(());

            // Give tasks time to clean up
            runtime.shutdown_timeout(Duration::from_secs(5));
            info!("Runtime manager shut down");
        }
    }
}

/// Builder for RuntimeManager
pub struct RuntimeManagerBuilder {
    config: RuntimeConfig,
}

impl RuntimeManagerBuilder {
    /// Create a new builder with default config
    pub fn new() -> Self {
        Self {
            config: RuntimeConfig::default(),
        }
    }

    /// Set the configuration
    pub fn config(mut self, config: RuntimeConfig) -> Self {
        self.config = config;
        self
    }

    /// Set I/O thread count
    pub fn io_threads(mut self, threads: usize) -> Self {
        self.config.worker.io_threads = threads;
        self
    }

    /// Set max workers
    pub fn max_workers(mut self, max: usize) -> Self {
        self.config.coordinator.max_workers = max;
        self
    }

    /// Set heartbeat timeout
    pub fn heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.config.coordinator.heartbeat_timeout = timeout;
        self
    }

    /// Build the runtime manager
    pub fn build(self) -> Result<RuntimeManager> {
        RuntimeManager::new(self.config)
    }
}

impl Default for RuntimeManagerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let manager = RuntimeManagerBuilder::new()
            .io_threads(2)
            .max_workers(100)
            .build()
            .unwrap();

        assert_eq!(manager.worker_registry().world_size(), 0);
    }

    #[test]
    fn test_spawn_task() {
        let manager = RuntimeManagerBuilder::new().build().unwrap();

        let result = manager.block_on(async {
            let handle = manager.spawn(async { 42 });
            handle.await.unwrap()
        });

        assert_eq!(result, 42);
    }
}
