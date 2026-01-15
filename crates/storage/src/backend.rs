//! Storage backend trait definition
//!
//! Defines the async interface that all storage backends must implement.

use async_trait::async_trait;
use bytes::Bytes;
use runtime_core::Result;

/// Async trait for storage backends
///
/// Implementors provide basic CRUD operations for binary data,
/// supporting both local filesystem and remote storage (S3, etc.).
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Read data from the given path
    ///
    /// # Arguments
    /// * `path` - Relative path within the storage backend
    ///
    /// # Returns
    /// The file contents as `Bytes`
    ///
    /// # Errors
    /// Returns error if path doesn't exist or read fails
    async fn read(&self, path: &str) -> Result<Bytes>;

    /// Write data to the given path
    ///
    /// Creates parent directories if they don't exist.
    /// Uses atomic writes where possible (write to temp, then rename).
    ///
    /// # Arguments
    /// * `path` - Relative path within the storage backend
    /// * `data` - Binary data to write
    ///
    /// # Returns
    /// Number of bytes written
    ///
    /// # Errors
    /// Returns error if write fails
    async fn write(&self, path: &str, data: Bytes) -> Result<u64>;

    /// Delete data at the given path
    ///
    /// # Arguments
    /// * `path` - Relative path within the storage backend
    ///
    /// # Errors
    /// Returns error if path doesn't exist or deletion fails
    async fn delete(&self, path: &str) -> Result<()>;

    /// Check if a path exists
    ///
    /// # Arguments
    /// * `path` - Relative path within the storage backend
    ///
    /// # Returns
    /// `true` if the path exists, `false` otherwise
    async fn exists(&self, path: &str) -> Result<bool>;

    /// List all paths under a given prefix
    ///
    /// # Arguments
    /// * `prefix` - Path prefix to filter by (e.g., "checkpoints/")
    ///
    /// # Returns
    /// Vector of paths matching the prefix
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
}
