//! Local filesystem storage backend
//!
//! Provides async file I/O with atomic writes to prevent partial/corrupt files.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use bytes::Bytes;
use runtime_core::{Error, Result};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::StorageBackend;

/// Local filesystem storage backend
///
/// Stores data in a local directory with support for:
/// - Atomic writes (write to .tmp, then rename)
/// - Automatic directory creation
/// - Recursive file listing
#[derive(Debug, Clone)]
pub struct LocalStorage {
    /// Base path for all storage operations
    base_path: PathBuf,
}

impl LocalStorage {
    /// Create a new LocalStorage instance
    ///
    /// # Arguments
    /// * `base_path` - Directory to use as the storage root
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Get the base path
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// Resolve a relative path to an absolute path
    fn resolve_path(&self, path: &str) -> PathBuf {
        self.base_path.join(path)
    }

    /// Generate a unique temporary file path
    fn temp_path(&self, path: &str) -> PathBuf {
        let full_path = self.resolve_path(path);
        let temp_name = format!(
            ".{}.{}.tmp",
            full_path.file_name().unwrap_or_default().to_string_lossy(),
            Uuid::new_v4()
        );
        full_path.with_file_name(temp_name)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    #[instrument(skip(self), fields(backend = "local"))]
    async fn read(&self, path: &str) -> Result<Bytes> {
        let full_path = self.resolve_path(path);
        debug!(?full_path, "Reading file");

        match fs::read(&full_path).await {
            Ok(data) => Ok(Bytes::from(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::StoragePathNotFound {
                path: path.to_string(),
            }),
            Err(e) => Err(Error::Storage {
                message: format!("Failed to read {}: {}", path, e),
            }),
        }
    }

    #[instrument(skip(self, data), fields(backend = "local", size = data.len()))]
    async fn write(&self, path: &str, data: Bytes) -> Result<u64> {
        let full_path = self.resolve_path(path);
        let temp_path = self.temp_path(path);
        let size = data.len() as u64;

        debug!(?full_path, ?temp_path, size, "Writing file atomically");

        // Ensure parent directory exists
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Storage {
                    message: format!("Failed to create directory {:?}: {}", parent, e),
                })?;
        }

        // Write to temporary file
        let mut file = fs::File::create(&temp_path)
            .await
            .map_err(|e| Error::Storage {
                message: format!("Failed to create temp file {:?}: {}", temp_path, e),
            })?;

        file.write_all(&data).await.map_err(|e| Error::Storage {
            message: format!("Failed to write data: {}", e),
        })?;

        file.sync_all().await.map_err(|e| Error::Storage {
            message: format!("Failed to sync file: {}", e),
        })?;

        // Atomic rename
        fs::rename(&temp_path, &full_path)
            .await
            .map_err(|e| Error::Storage {
                message: format!("Failed to rename {:?} to {:?}: {}", temp_path, full_path, e),
            })?;

        debug!(?full_path, size, "File written successfully");
        Ok(size)
    }

    #[instrument(skip(self), fields(backend = "local"))]
    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.resolve_path(path);
        debug!(?full_path, "Deleting file");

        match fs::remove_file(&full_path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Error::StoragePathNotFound {
                path: path.to_string(),
            }),
            Err(e) => Err(Error::Storage {
                message: format!("Failed to delete {}: {}", path, e),
            }),
        }
    }

    #[instrument(skip(self), fields(backend = "local"))]
    async fn exists(&self, path: &str) -> Result<bool> {
        let full_path = self.resolve_path(path);
        Ok(fs::metadata(&full_path).await.is_ok())
    }

    #[instrument(skip(self), fields(backend = "local"))]
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let search_path = self.resolve_path(prefix);
        let mut results = Vec::new();

        debug!(?search_path, "Listing files with prefix");

        // Determine the directory to scan
        let dir_to_scan = if search_path.is_dir() {
            search_path.clone()
        } else if let Some(parent) = search_path.parent() {
            if parent.is_dir() {
                parent.to_path_buf()
            } else {
                return Ok(results);
            }
        } else {
            return Ok(results);
        };

        // Recursively walk the directory
        let mut stack = vec![dir_to_scan];
        while let Some(dir) = stack.pop() {
            let mut entries = match fs::read_dir(&dir).await {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let entry_path = entry.path();
                let metadata = match entry.metadata().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if metadata.is_dir() {
                    stack.push(entry_path);
                } else if metadata.is_file() {
                    // Convert to relative path
                    if let Ok(relative) = entry_path.strip_prefix(&self.base_path) {
                        let relative_str = relative.to_string_lossy().to_string();
                        // Only include if it matches the prefix
                        if relative_str.starts_with(prefix) {
                            results.push(relative_str);
                        }
                    }
                }
            }
        }

        results.sort();
        debug!(count = results.len(), "Found files");
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> (TempDir, LocalStorage) {
        let temp_dir = TempDir::new().unwrap();
        let storage = LocalStorage::new(temp_dir.path());
        (temp_dir, storage)
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let (_temp_dir, storage) = setup().await;
        let data = Bytes::from("hello world");

        let written = storage.write("test.txt", data.clone()).await.unwrap();
        assert_eq!(written, 11);

        let read_data = storage.read("test.txt").await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_write_creates_directories() {
        let (_temp_dir, storage) = setup().await;
        let data = Bytes::from("nested content");

        storage.write("a/b/c/deep.txt", data.clone()).await.unwrap();

        let read_data = storage.read("a/b/c/deep.txt").await.unwrap();
        assert_eq!(read_data, data);
    }

    #[tokio::test]
    async fn test_exists() {
        let (_temp_dir, storage) = setup().await;

        assert!(!storage.exists("missing.txt").await.unwrap());

        storage
            .write("exists.txt", Bytes::from("data"))
            .await
            .unwrap();
        assert!(storage.exists("exists.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete() {
        let (_temp_dir, storage) = setup().await;

        storage
            .write("to_delete.txt", Bytes::from("data"))
            .await
            .unwrap();
        assert!(storage.exists("to_delete.txt").await.unwrap());

        storage.delete("to_delete.txt").await.unwrap();
        assert!(!storage.exists("to_delete.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let (_temp_dir, storage) = setup().await;

        let result = storage.delete("missing.txt").await;
        assert!(matches!(result, Err(Error::StoragePathNotFound { .. })));
    }

    #[tokio::test]
    async fn test_read_not_found() {
        let (_temp_dir, storage) = setup().await;

        let result = storage.read("missing.txt").await;
        assert!(matches!(result, Err(Error::StoragePathNotFound { .. })));
    }

    #[tokio::test]
    async fn test_list() {
        let (_temp_dir, storage) = setup().await;

        storage
            .write("checkpoints/epoch-1.bin", Bytes::from("1"))
            .await
            .unwrap();
        storage
            .write("checkpoints/epoch-2.bin", Bytes::from("2"))
            .await
            .unwrap();
        storage
            .write("other/file.txt", Bytes::from("other"))
            .await
            .unwrap();

        let checkpoints = storage.list("checkpoints/").await.unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints.contains(&"checkpoints/epoch-1.bin".to_string()));
        assert!(checkpoints.contains(&"checkpoints/epoch-2.bin".to_string()));

        let all = storage.list("").await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_atomic_write_prevents_partial() {
        let (temp_dir, storage) = setup().await;
        let data = Bytes::from("complete data");

        // Write should be atomic - no partial files
        storage.write("atomic.txt", data.clone()).await.unwrap();

        // Check no temp files remain
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(entries.is_empty(), "Temp files should be cleaned up");
    }
}
