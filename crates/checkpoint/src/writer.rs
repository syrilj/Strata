//! Async checkpoint writer for non-blocking I/O

use bytes::Bytes;
use runtime_core::{CheckpointType, Epoch, Error, Result, Step};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use storage::{LocalStorage, StorageBackend};
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};

/// Request to write a checkpoint
#[derive(Debug)]
pub struct WriteRequest {
    /// Checkpoint identifier
    pub checkpoint_id: String,

    /// Checkpoint data
    pub data: Bytes,

    /// Target path (relative to storage base or filename)
    pub path: PathBuf,

    /// Training step
    pub step: Step,

    /// Training epoch
    pub epoch: Epoch,

    /// Checkpoint type
    pub checkpoint_type: CheckpointType,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Event reported by writer
#[derive(Debug)]
pub enum WriterEvent {
    /// Write completed successfully
    Completed {
        checkpoint_id: String,
        size_bytes: u64,
        checkpoint_type: CheckpointType,
        metadata: HashMap<String, String>,
    },
    /// Write failed
    Failed {
        checkpoint_id: String,
        error: String,
    },
}

/// Async checkpoint writer using Tokio and a pluggable `StorageBackend`
pub struct AsyncCheckpointWriter {
    /// Task handle
    _task: tokio::task::JoinHandle<()>,
}

impl AsyncCheckpointWriter {
    /// Create a new async writer backed by `LocalStorage` at `base_path`
    pub async fn new(
        base_path: PathBuf,
        buffer_size: usize,
        compression: bool,
        event_tx: mpsc::Sender<WriterEvent>,
    ) -> Result<(mpsc::Sender<WriteRequest>, Self)> {
        let storage: Arc<dyn StorageBackend> = Arc::new(LocalStorage::new(&base_path));
        Self::with_storage(base_path, storage, buffer_size, compression, event_tx).await
    }

    /// Create a new async writer with an explicit `StorageBackend`
    pub async fn with_storage(
        base_path: PathBuf,
        storage: Arc<dyn StorageBackend>,
        buffer_size: usize,
        compression: bool,
        event_tx: mpsc::Sender<WriterEvent>,
    ) -> Result<(mpsc::Sender<WriteRequest>, Self)> {
        // Ensure minimum channel capacity of 1 to prevent blocking
        let channel_capacity = (buffer_size / (1024 * 1024)).max(1);
        let (tx, rx) = mpsc::channel::<WriteRequest>(channel_capacity);

        let task = tokio::spawn(Self::writer_loop(
            rx,
            event_tx,
            compression,
            base_path,
            storage,
        ));

        Ok((tx, Self { _task: task }))
    }

    /// Convert a request path into a relative storage key
    fn storage_key(base_path: &Path, path: &Path) -> String {
        let rel = path.strip_prefix(base_path).unwrap_or(path);
        rel.to_string_lossy().replace('\\', "/")
    }

    /// Main writer loop
    async fn writer_loop(
        mut rx: mpsc::Receiver<WriteRequest>,
        event_tx: mpsc::Sender<WriterEvent>,
        compression: bool,
        base_path: PathBuf,
        storage: Arc<dyn StorageBackend>,
    ) {
        info!("Checkpoint writer started");

        while let Some(request) = rx.recv().await {
            let checkpoint_id = request.checkpoint_id.clone();
            let checkpoint_type = request.checkpoint_type;
            let metadata = request.metadata.clone();
            let key = Self::storage_key(&base_path, &request.path);
            let result =
                Self::write_checkpoint_to_storage(&request, compression, &*storage, &key).await;

            match result {
                Ok(size) => {
                    debug!(
                        checkpoint_id = %request.checkpoint_id,
                        size_bytes = size,
                        path = %request.path.display(),
                        "Checkpoint written successfully"
                    );

                    let _ = event_tx
                        .send(WriterEvent::Completed {
                            checkpoint_id,
                            size_bytes: size,
                            checkpoint_type,
                            metadata,
                        })
                        .await;
                }
                Err(e) => {
                    error!(
                        checkpoint_id = %request.checkpoint_id,
                        error = %e,
                        "Failed to write checkpoint"
                    );

                    let _ = event_tx
                        .send(WriterEvent::Failed {
                            checkpoint_id,
                            error: e.to_string(),
                        })
                        .await;
                }
            }
        }

        info!("Checkpoint writer stopped");
    }

    /// Write a single checkpoint directly to local filesystem (helper for tests and local callers)
    #[instrument(skip(request), fields(checkpoint_id = %request.checkpoint_id, step = request.step))]
    #[allow(dead_code)]
    async fn write_checkpoint(request: &WriteRequest, compression: bool) -> Result<u64> {
        let parent = request
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = request
            .path
            .file_name()
            .ok_or_else(|| Error::Storage {
                message: format!("Invalid checkpoint path: {}", request.path.display()),
            })?
            .to_string_lossy();
        let storage = LocalStorage::new(parent);
        Self::write_checkpoint_to_storage(request, compression, &storage, &file_name).await
    }

    /// Write a single checkpoint through a `StorageBackend`
    #[instrument(skip(request, storage), fields(checkpoint_id = %request.checkpoint_id, step = request.step))]
    async fn write_checkpoint_to_storage(
        request: &WriteRequest,
        compression: bool,
        storage: &dyn StorageBackend,
        storage_key: &str,
    ) -> Result<u64> {
        let start = std::time::Instant::now();

        // Prepare data (optionally compress with LZ4)
        let payload = if compression {
            Self::compress_data(&request.data)?
        } else {
            request.data.clone()
        };

        // Write header with payload size and metadata
        let header = Self::create_header_with_size(request, compression, payload.len() as u64)?;
        let mut combined = Vec::with_capacity(header.len() + payload.len());
        combined.extend_from_slice(&header);
        combined.extend_from_slice(&payload);

        let size = storage.write(storage_key, Bytes::from(combined)).await?;
        let elapsed = start.elapsed();

        info!(
            checkpoint_id = %request.checkpoint_id,
            size_bytes = size,
            elapsed_ms = elapsed.as_millis(),
            throughput_mbps = (size as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64().max(1e-9),
            "Checkpoint write complete"
        );

        Ok(size)
    }

    /// Create checkpoint header using `request.data.len()` as data size
    #[allow(dead_code)]
    fn create_header(request: &WriteRequest, compressed: bool) -> Result<Vec<u8>> {
        Self::create_header_with_size(request, compressed, request.data.len() as u64)
    }

    /// Create checkpoint header with explicit payload size
    fn create_header_with_size(
        request: &WriteRequest,
        compressed: bool,
        payload_size: u64,
    ) -> Result<Vec<u8>> {
        let header = CheckpointHeader {
            magic: CHECKPOINT_MAGIC,
            version: CHECKPOINT_VERSION,
            step: request.step,
            epoch: request.epoch,
            checkpoint_type: checkpoint_type_to_u8(request.checkpoint_type),
            compressed,
            data_size: payload_size,
            metadata_json: serde_json::to_string(&request.metadata)?,
        };

        let mut buf = Vec::with_capacity(256);

        // Write magic (4)
        buf.extend_from_slice(&header.magic);

        // Write version (4)
        buf.extend_from_slice(&header.version.to_le_bytes());

        // Write step (8)
        buf.extend_from_slice(&header.step.to_le_bytes());

        // Write epoch (8)
        buf.extend_from_slice(&header.epoch.to_le_bytes());

        // Write checkpoint type (1)
        buf.push(header.checkpoint_type);

        // Write compressed flag (1)
        buf.push(if header.compressed { 1 } else { 0 });

        // Write data size (8)
        buf.extend_from_slice(&header.data_size.to_le_bytes());

        // Write metadata length (4) and content
        let metadata_bytes = header.metadata_json.as_bytes();
        buf.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(metadata_bytes);

        Ok(buf)
    }

    /// Compress data using LZ4 (with prepended uncompressed size)
    pub fn compress_data(data: &Bytes) -> Result<Bytes> {
        let compressed = lz4_flex::compress_prepend_size(data.as_ref());
        Ok(Bytes::from(compressed))
    }

    /// Decompress LZ4 data (with prepended uncompressed size)
    pub fn decompress_data(data: &[u8]) -> Result<Bytes> {
        let decompressed =
            lz4_flex::decompress_size_prepended(data).map_err(|e| Error::Storage {
                message: format!("Failed to decompress checkpoint data: {}", e),
            })?;
        Ok(Bytes::from(decompressed))
    }

    /// Parse checkpoint header and return `(header, payload_offset)` from raw checkpoint bytes
    pub fn parse_header_from_bytes(raw: &[u8]) -> Result<(CheckpointHeader, usize)> {
        // Fixed prefix size: 4 (magic) + 4 (version) + 8 (step) + 8 (epoch) + 1 (type) + 1 (compressed) + 8 (data_size) + 4 (meta_len) = 38
        if raw.len() < 38 {
            return Err(Error::Storage {
                message: format!("Checkpoint file too short: {} bytes", raw.len()),
            });
        }

        let magic: [u8; 4] = raw[0..4].try_into().unwrap();
        if magic != CHECKPOINT_MAGIC {
            return Err(Error::Storage {
                message: "Invalid checkpoint magic".to_string(),
            });
        }

        let version = u32::from_le_bytes(raw[4..8].try_into().unwrap());
        if version != CHECKPOINT_VERSION {
            warn!(
                "Checkpoint version mismatch: expected {}, got {}",
                CHECKPOINT_VERSION, version
            );
        }

        let step = u64::from_le_bytes(raw[8..16].try_into().unwrap());
        let epoch = u64::from_le_bytes(raw[16..24].try_into().unwrap());
        let checkpoint_type = raw[24];
        let compressed = raw[25] != 0;
        let data_size = u64::from_le_bytes(raw[26..34].try_into().unwrap());
        let meta_len = u32::from_le_bytes(raw[34..38].try_into().unwrap()) as usize;

        let meta_end = 38 + meta_len;
        if raw.len() < meta_end {
            return Err(Error::Storage {
                message: "Truncated checkpoint metadata header".to_string(),
            });
        }

        let metadata_json =
            String::from_utf8(raw[38..meta_end].to_vec()).map_err(|e| Error::Storage {
                message: format!("Invalid UTF-8 in checkpoint metadata: {}", e),
            })?;

        Ok((
            CheckpointHeader {
                magic,
                version,
                step,
                epoch,
                checkpoint_type,
                compressed,
                data_size,
                metadata_json,
            },
            meta_end,
        ))
    }

    /// Decode payload from raw checkpoint file bytes
    pub fn decode_checkpoint_bytes(raw: &[u8]) -> Result<Bytes> {
        let (header, payload_offset) = Self::parse_header_from_bytes(raw)?;
        let payload_end = payload_offset + header.data_size as usize;

        if raw.len() < payload_end {
            return Err(Error::Storage {
                message: format!(
                    "Truncated checkpoint payload: expected {} bytes, got {}",
                    payload_end,
                    raw.len()
                ),
            });
        }

        let payload = &raw[payload_offset..payload_end];
        if header.compressed {
            Self::decompress_data(payload)
        } else {
            Ok(Bytes::copy_from_slice(payload))
        }
    }

    /// Read checkpoint data from a local file path
    pub async fn read_checkpoint_data(path: &PathBuf) -> Result<Bytes> {
        let raw = tokio::fs::read(path).await.map_err(Error::Io)?;
        Self::decode_checkpoint_bytes(&raw)
    }

    /// Read checkpoint data using a `StorageBackend`
    pub async fn read_checkpoint_from_storage(
        storage: &dyn StorageBackend,
        key: &str,
    ) -> Result<Bytes> {
        let raw = storage.read(key).await?;
        Self::decode_checkpoint_bytes(&raw)
    }
}

pub(crate) fn checkpoint_type_to_u8(ty: CheckpointType) -> u8 {
    match ty {
        CheckpointType::Full => 0,
        CheckpointType::Incremental => 1,
        CheckpointType::OptimizerOnly => 2,
        CheckpointType::ModelOnly => 3,
    }
}

pub(crate) fn u8_to_checkpoint_type(v: u8) -> CheckpointType {
    match v {
        1 => CheckpointType::Incremental,
        2 => CheckpointType::OptimizerOnly,
        3 => CheckpointType::ModelOnly,
        _ => CheckpointType::Full,
    }
}

/// Checkpoint file header
#[derive(Debug)]
pub struct CheckpointHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub step: Step,
    pub epoch: Epoch,
    pub checkpoint_type: u8,
    pub compressed: bool,
    pub data_size: u64,
    pub metadata_json: String,
}

/// Magic bytes for checkpoint files
pub const CHECKPOINT_MAGIC: [u8; 4] = *b"CKPT";

/// Checkpoint format version
pub const CHECKPOINT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_write_checkpoint() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.ckpt");

        let request = WriteRequest {
            checkpoint_id: "test-1".to_string(),
            data: Bytes::from(vec![1u8; 1000]),
            path: path.clone(),
            step: 100,
            epoch: 1,
            checkpoint_type: CheckpointType::Full,
            metadata: HashMap::new(),
        };

        let size = AsyncCheckpointWriter::write_checkpoint(&request, false)
            .await
            .unwrap();

        assert!(size > 1000);
        assert!(path.exists());

        let read_back = AsyncCheckpointWriter::read_checkpoint_data(&path)
            .await
            .unwrap();
        assert_eq!(read_back, Bytes::from(vec![1u8; 1000]));
    }

    #[tokio::test]
    async fn test_lz4_compression_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("compressed.ckpt");
        let original = Bytes::from(vec![42u8; 8192]);

        let mut meta = HashMap::new();
        meta.insert("worker".to_string(), "w-0".to_string());

        let request = WriteRequest {
            checkpoint_id: "test-comp".to_string(),
            data: original.clone(),
            path: path.clone(),
            step: 200,
            epoch: 2,
            checkpoint_type: CheckpointType::OptimizerOnly,
            metadata: meta,
        };

        let size = AsyncCheckpointWriter::write_checkpoint(&request, true)
            .await
            .unwrap();

        assert!(size < 8192);
        let read_back = AsyncCheckpointWriter::read_checkpoint_data(&path)
            .await
            .unwrap();
        assert_eq!(read_back, original);
    }
}
