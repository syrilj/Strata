//! Storage - Pluggable storage backends for the distributed training runtime
//!
//! Provides async storage operations with support for:
//! - Local filesystem (default feature)
//! - Amazon S3 / S3-compatible storage (with `s3` feature)
//!
//! # Example
//!
//! ```no_run
//! use storage::{StorageBackend, LocalStorage};
//! use bytes::Bytes;
//!
//! # async fn example() -> runtime_core::Result<()> {
//! let storage = LocalStorage::new("/tmp/checkpoints");
//! storage.write("model/epoch-1.bin", Bytes::from(vec![1, 2, 3])).await?;
//! let data = storage.read("model/epoch-1.bin").await?;
//! # Ok(())
//! # }
//! ```

mod backend;
mod local;

#[cfg(feature = "s3")]
mod s3;

pub use backend::StorageBackend;
pub use local::LocalStorage;

#[cfg(feature = "s3")]
pub use s3::S3Storage;
