//! Checkpoint management for distributed training
//!
//! Provides async checkpoint writing, versioning, and recovery coordination.

pub mod manager;
pub mod writer;

pub use manager::{CheckpointManager, CheckpointManagerConfig, CheckpointManagerHandle};
pub use writer::AsyncCheckpointWriter;
