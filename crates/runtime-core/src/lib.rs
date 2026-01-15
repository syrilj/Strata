//! Runtime Core - Foundation for the distributed training runtime
//!
//! Provides core types, error handling, and async runtime utilities
//! for the distributed training data and checkpoint system.

pub mod config;
pub mod error;
pub mod runtime;
pub mod types;
pub mod worker;

pub use config::RuntimeConfig;
pub use error::{Error, Result};
pub use runtime::RuntimeManager;
pub use types::*;
pub use worker::{WorkerInfo, WorkerRegistry, WorkerRegistryHandle, WorkerState};
