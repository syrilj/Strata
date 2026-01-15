//! Checkpoint manager Python bindings
//!
//! Exposes async checkpoint operations with synchronous Python wrappers.

use bytes::Bytes;
use checkpoint::{CheckpointManager as RustCheckpointManager, CheckpointManagerConfig};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Metadata about a saved checkpoint
#[pyclass]
#[derive(Clone)]
pub struct CheckpointInfo {
    /// Unique checkpoint identifier
    #[pyo3(get)]
    pub checkpoint_id: String,

    /// Training step at checkpoint
    #[pyo3(get)]
    pub step: u64,

    /// Training epoch at checkpoint
    #[pyo3(get)]
    pub epoch: u64,

    /// Storage path
    #[pyo3(get)]
    pub path: String,

    /// Size in bytes
    #[pyo3(get)]
    pub size_bytes: u64,

    /// Creation timestamp (ISO 8601 string)
    #[pyo3(get)]
    pub created_at: String,
}

#[pymethods]
impl CheckpointInfo {
    fn __repr__(&self) -> String {
        format!(
            "CheckpointInfo(id='{}', step={}, epoch={}, size={})",
            self.checkpoint_id, self.step, self.epoch, self.size_bytes
        )
    }
}

/// Checkpoint manager for saving and loading training checkpoints
///
/// Provides async checkpoint writing with configurable retention.
///
/// Example:
///     ckpt = CheckpointManager("/tmp/checkpoints", keep_count=5)
///     
///     # Save a checkpoint
///     checkpoint_id = ckpt.save(model_bytes, step=1000, epoch=5)
///     
///     # Load the latest checkpoint
///     info = ckpt.latest()
///     data = ckpt.load(info.checkpoint_id)
#[pyclass]
pub struct CheckpointManager {
    inner: Arc<RustCheckpointManager>,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl CheckpointManager {
    /// Create a new checkpoint manager
    ///
    /// Args:
    ///     base_path: Directory to store checkpoints
    ///     keep_count: Number of checkpoints to retain (default: 5)
    ///     compression: Enable compression (default: True)
    #[new]
    #[pyo3(signature = (base_path, keep_count=5, compression=true))]
    fn new(base_path: &str, keep_count: usize, compression: bool) -> PyResult<Self> {
        let config = CheckpointManagerConfig {
            base_path: PathBuf::from(base_path),
            keep_count,
            compression,
            ..Default::default()
        };

        // Create tokio runtime for async operations
        let runtime = Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to create async runtime: {}",
                e
            ))
        })?;

        let inner = runtime.block_on(async { RustCheckpointManager::new(config).await });

        match inner {
            Ok(manager) => Ok(Self {
                inner: Arc::new(manager),
                runtime: Arc::new(runtime),
            }),
            Err(e) => Err(pyo3::exceptions::PyIOError::new_err(format!(
                "Failed to create checkpoint manager: {}",
                e
            ))),
        }
    }

    /// Save a checkpoint asynchronously
    ///
    /// Args:
    ///     data: Checkpoint data as bytes
    ///     step: Current training step
    ///     epoch: Current training epoch
    ///     metadata: Optional metadata dictionary
    ///
    /// Returns:
    ///     Checkpoint ID string
    #[pyo3(signature = (data, step, epoch, metadata=None))]
    fn save(
        &self,
        py: Python<'_>,
        data: &Bound<'_, PyBytes>,
        step: u64,
        epoch: u64,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<String> {
        let bytes_data = Bytes::copy_from_slice(data.as_bytes());
        let meta = metadata.unwrap_or_default();
        let inner = self.inner.clone();

        // Release GIL during async operation
        py.allow_threads(|| {
            self.runtime.block_on(async move {
                inner
                    .save_async(
                        bytes_data,
                        step,
                        epoch,
                        runtime_core::CheckpointType::Full,
                        meta,
                    )
                    .await
                    .map_err(|e| {
                        pyo3::exceptions::PyIOError::new_err(format!(
                            "Failed to save checkpoint: {}",
                            e
                        ))
                    })
            })
        })
    }

    /// Load checkpoint data by ID
    ///
    /// Args:
    ///     checkpoint_id: The checkpoint ID to load
    ///
    /// Returns:
    ///     Checkpoint data as bytes
    fn load(&self, py: Python<'_>, checkpoint_id: &str) -> PyResult<PyObject> {
        let inner = self.inner.clone();
        let ckpt_id = checkpoint_id.to_string();

        let data = py.allow_threads(|| {
            self.runtime.block_on(async move {
                inner.load(&ckpt_id).await.map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!(
                        "Failed to load checkpoint: {}",
                        e
                    ))
                })
            })
        })?;

        Ok(PyBytes::new_bound(py, &data).into())
    }

    /// Get the latest checkpoint info
    ///
    /// Returns:
    ///     CheckpointInfo or None if no checkpoints exist
    fn latest(&self) -> Option<CheckpointInfo> {
        self.inner.latest().map(|m| CheckpointInfo {
            checkpoint_id: m.id,
            step: m.step,
            epoch: m.epoch,
            path: m.path,
            size_bytes: m.size_bytes,
            created_at: m.created_at.to_rfc3339(),
        })
    }

    /// Get checkpoint info by step
    ///
    /// Args:
    ///     step: Training step to look up
    ///
    /// Returns:
    ///     CheckpointInfo or None if not found
    fn get_by_step(&self, step: u64) -> Option<CheckpointInfo> {
        self.inner.get_by_step(step).map(|m| CheckpointInfo {
            checkpoint_id: m.id,
            step: m.step,
            epoch: m.epoch,
            path: m.path,
            size_bytes: m.size_bytes,
            created_at: m.created_at.to_rfc3339(),
        })
    }

    /// Get all checkpoint infos
    ///
    /// Returns:
    ///     List of CheckpointInfo objects
    fn all_checkpoints(&self) -> Vec<CheckpointInfo> {
        self.inner
            .all_checkpoints()
            .into_iter()
            .map(|m| CheckpointInfo {
                checkpoint_id: m.id,
                step: m.step,
                epoch: m.epoch,
                path: m.path,
                size_bytes: m.size_bytes,
                created_at: m.created_at.to_rfc3339(),
            })
            .collect()
    }

    /// Wait for all pending checkpoint writes to complete
    fn wait_pending(&self, py: Python<'_>) -> PyResult<()> {
        let inner = self.inner.clone();

        py.allow_threads(|| {
            self.runtime.block_on(async move {
                inner.wait_pending().await.map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!(
                        "Pending checkpoint writes failed: {}",
                        e
                    ))
                })
            })
        })
    }

    fn __repr__(&self) -> String {
        let count = self.inner.all_checkpoints().len();
        format!("CheckpointManager(checkpoints={})", count)
    }
}
