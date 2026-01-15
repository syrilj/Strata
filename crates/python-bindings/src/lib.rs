//! Python bindings for the distributed training runtime
//!
//! This crate provides PyO3 bindings exposing the Rust runtime's core
//! functionality to Python, including:
//!
//! - `DatasetRegistry`: Register datasets and get shard assignments
//! - `CheckpointManager`: Save and load training checkpoints
//! - `TrainingOrchestrator`: High-level training coordination
//!
//! # Example
//!
//! ```python
//! from dtruntime import DatasetRegistry, CheckpointManager, TrainingOrchestrator
//!
//! # Create a checkpoint manager
//! ckpt = CheckpointManager("/tmp/checkpoints", keep_count=5)
//!
//! # Save a checkpoint
//! ckpt.save(model_bytes, step=1000, epoch=5)
//! ```

use pyo3::prelude::*;

mod checkpoint;
mod dataset;
mod orchestrator;

/// Python module for the distributed training runtime
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Register classes
    m.add_class::<dataset::DatasetRegistry>()?;
    m.add_class::<dataset::ShardInfo>()?;
    m.add_class::<checkpoint::CheckpointManager>()?;
    m.add_class::<checkpoint::CheckpointInfo>()?;
    m.add_class::<orchestrator::TrainingOrchestrator>()?;
    m.add_class::<orchestrator::WorkerConfig>()?;
    
    // Add version info
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    
    Ok(())
}
