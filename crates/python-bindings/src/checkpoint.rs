//! Checkpoint manager Python bindings
//!
//! Exposes async checkpoint operations with both synchronous and async Python methods.

use bytes::Bytes;
use checkpoint::{CheckpointManager as RustCheckpointManager, CheckpointManagerConfig};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes};
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
#[pyclass]
pub struct CheckpointManager {
    inner: Arc<RustCheckpointManager>,
    runtime: Arc<Runtime>,
}

impl CheckpointManager {
    fn resolve_checkpoint_id(&self, checkpoint_id_or_step: &Bound<'_, PyAny>) -> PyResult<String> {
        if let Ok(step) = checkpoint_id_or_step.extract::<u64>() {
            let info = self.inner.get_by_step(step).ok_or_else(|| {
                pyo3::exceptions::PyFileNotFoundError::new_err(format!(
                    "No checkpoint found for step {}",
                    step
                ))
            })?;
            Ok(info.id)
        } else if let Ok(ckpt_id) = checkpoint_id_or_step.extract::<String>() {
            Ok(ckpt_id)
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "checkpoint_id_or_step must be a checkpoint_id (str) or step (int)",
            ))
        }
    }
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

    /// Save a checkpoint (queues async write and returns checkpoint ID)
    #[pyo3(signature = (data, step, epoch=0, metadata=None))]
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

        py.detach(|| {
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

    /// Async save a checkpoint, awaiting completion of the write
    #[pyo3(signature = (data, step, epoch=0, metadata=None))]
    fn save_async<'py>(
        &self,
        py: Python<'py>,
        data: &Bound<'py, PyBytes>,
        step: u64,
        epoch: u64,
        metadata: Option<HashMap<String, String>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let bytes_data = Bytes::copy_from_slice(data.as_bytes());
        let meta = metadata.unwrap_or_default();
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ckpt_id = inner
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
                })?;

            inner.wait_pending().await.map_err(|e| {
                pyo3::exceptions::PyIOError::new_err(format!(
                    "Pending checkpoint write failed: {}",
                    e
                ))
            })?;

            Ok(ckpt_id)
        })
    }

    /// Load checkpoint data by checkpoint ID (str) or step (int)
    #[pyo3(signature = (checkpoint_id_or_step))]
    fn load(
        &self,
        py: Python<'_>,
        checkpoint_id_or_step: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let ckpt_id = self.resolve_checkpoint_id(checkpoint_id_or_step)?;
        let inner = self.inner.clone();

        let data = py.detach(|| {
            self.runtime.block_on(async move {
                inner.load(&ckpt_id).await.map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!(
                        "Failed to load checkpoint: {}",
                        e
                    ))
                })
            })
        })?;

        Ok(PyBytes::new(py, &data).into())
    }

    /// Async load checkpoint data by checkpoint ID (str) or step (int)
    #[pyo3(signature = (checkpoint_id_or_step))]
    fn load_async<'py>(
        &self,
        py: Python<'py>,
        checkpoint_id_or_step: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let ckpt_id = self.resolve_checkpoint_id(checkpoint_id_or_step)?;
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let data = inner.load(&ckpt_id).await.map_err(|e| {
                pyo3::exceptions::PyIOError::new_err(format!("Failed to load checkpoint: {}", e))
            })?;

            Ok(Python::attach(|py| -> Py<PyAny> {
                PyBytes::new(py, &data).into()
            }))
        })
    }

    /// Delete a checkpoint by ID
    fn delete(&self, py: Python<'_>, checkpoint_id: &str) -> PyResult<bool> {
        let inner = self.inner.clone();
        let ckpt_id = checkpoint_id.to_string();

        py.detach(|| {
            self.runtime.block_on(async move {
                inner.delete(&ckpt_id).await.map_err(|e| {
                    pyo3::exceptions::PyIOError::new_err(format!(
                        "Failed to delete checkpoint: {}",
                        e
                    ))
                })
            })
        })
    }

    /// Return sorted list of checkpoint steps
    fn list_checkpoints(&self) -> Vec<u64> {
        let mut steps: Vec<u64> = self
            .inner
            .all_checkpoints()
            .into_iter()
            .map(|m| m.step)
            .collect();
        steps.sort_unstable();
        steps
    }

    /// Get the latest checkpoint info
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

        py.detach(|| {
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
