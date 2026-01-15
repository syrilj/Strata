//! Async checkpoint writer for non-blocking I/O

use bytes::Bytes;
use runtime_core::{CheckpointType, Epoch, Error, Result, Step};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};

/// Request to write a checkpoint
#[derive(Debug)]
pub struct WriteRequest {
    /// Checkpoint identifier
    pub checkpoint_id: String,

    /// Checkpoint data
    pub data: Bytes,

    /// Target path
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
    },
    /// Write failed
    Failed {
        checkpoint_id: String,
        error: String,
    },
}

/// Async checkpoint writer using Tokio
pub struct AsyncCheckpointWriter {
    /// Task handle
    _task: tokio::task::JoinHandle<()>,
}

impl AsyncCheckpointWriter {
    /// Create a new async writer
    pub async fn new(
        _base_path: PathBuf,
        buffer_size: usize,
        compression: bool,
        event_tx: mpsc::Sender<WriterEvent>,
    ) -> Result<(mpsc::Sender<WriteRequest>, Self)> {
        let (tx, rx) = mpsc::channel::<WriteRequest>(buffer_size / (1024 * 1024).max(16));

        let task = tokio::spawn(Self::writer_loop(rx, event_tx, compression));

        Ok((tx, Self { _task: task }))
    }

    /// Main writer loop
    async fn writer_loop(
        mut rx: mpsc::Receiver<WriteRequest>,
        event_tx: mpsc::Sender<WriterEvent>,
        compression: bool,
    ) {
        info!("Checkpoint writer started");

        while let Some(request) = rx.recv().await {
            let checkpoint_id = request.checkpoint_id.clone();
            let result = Self::write_checkpoint(&request, compression).await;

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

    /// Write a single checkpoint
    #[instrument(skip(request), fields(checkpoint_id = %request.checkpoint_id, step = request.step))]
    async fn write_checkpoint(request: &WriteRequest, compression: bool) -> Result<u64> {
        let start = std::time::Instant::now();

        // Prepare data (optionally compress)
        let data = if compression {
            Self::compress_data(&request.data)?
        } else {
            request.data.clone()
        };

        // Write to temporary file first (atomic write pattern)
        let temp_path = request.path.with_extension("tmp");

        // Ensure parent directory exists
        if let Some(parent) = request.path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| Error::Io(e))?;
        }

        // Write data
        let mut file = File::create(&temp_path).await.map_err(|e| Error::Io(e))?;

        // Write header with metadata
        let header = Self::create_header(request, compression)?;
        file.write_all(&header).await.map_err(|e| Error::Io(e))?;

        // Write data
        file.write_all(&data).await.map_err(|e| Error::Io(e))?;

        // Sync to disk
        file.sync_all().await.map_err(|e| Error::Io(e))?;

        // Atomic rename
        tokio::fs::rename(&temp_path, &request.path)
            .await
            .map_err(|e| Error::Io(e))?;

        let size = header.len() as u64 + data.len() as u64;
        let elapsed = start.elapsed();

        info!(
            checkpoint_id = %request.checkpoint_id,
            size_bytes = size,
            elapsed_ms = elapsed.as_millis(),
            throughput_mbps = (size as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64(),
            "Checkpoint write complete"
        );

        Ok(size)
    }

    /// Create checkpoint header
    fn create_header(request: &WriteRequest, compressed: bool) -> Result<Vec<u8>> {
        let header = CheckpointHeader {
            magic: CHECKPOINT_MAGIC,
            version: CHECKPOINT_VERSION,
            step: request.step,
            epoch: request.epoch,
            checkpoint_type: request.checkpoint_type as u8,
            compressed,
            data_size: request.data.len() as u64,
            metadata_json: serde_json::to_string(&request.metadata)?,
        };

        let mut buf = Vec::with_capacity(256);

        // Write magic
        buf.extend_from_slice(&header.magic);

        // Write version
        buf.extend_from_slice(&header.version.to_le_bytes());

        // Write step
        buf.extend_from_slice(&header.step.to_le_bytes());

        // Write epoch
        buf.extend_from_slice(&header.epoch.to_le_bytes());

        // Write checkpoint type
        buf.push(header.checkpoint_type);

        // Write compressed flag
        buf.push(if header.compressed { 1 } else { 0 });

        // Write data size
        buf.extend_from_slice(&header.data_size.to_le_bytes());

        // Write metadata length and content
        let metadata_bytes = header.metadata_json.as_bytes();
        buf.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(metadata_bytes);

        Ok(buf)
    }

    /// Compress data (placeholder - would use lz4 or zstd in production)
    fn compress_data(data: &Bytes) -> Result<Bytes> {
        // In production, use lz4 or zstd for fast compression
        // For now, just return uncompressed
        // TODO: Add actual compression with lz4 or zstd crate
        Ok(data.clone())
    }

    /// Read checkpoint data from file
    pub async fn read_checkpoint_data(path: &PathBuf) -> Result<Bytes> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        
        let mut file = File::open(path).await.map_err(|e| Error::Io(e))?;
        
        // Read magic (4)
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).await.map_err(|e| Error::Io(e))?;
        
        if magic != CHECKPOINT_MAGIC {
            return Err(Error::Storage {
                message: "Invalid checkpoint magic".to_string(),
            });
        }
        
        // Read version (4)
        let version = file.read_u32_le().await.map_err(|e| Error::Io(e))?;
        if version != CHECKPOINT_VERSION {
             warn!("Checkpoint version mismatch: expected {}, got {}", CHECKPOINT_VERSION, version);
        }
        
        // Skip step (8), epoch (8), type (1), compressed (1)
        // 8+8+1+1 = 18 bytes
        let mut skipped = [0u8; 18];
        file.read_exact(&mut skipped).await.map_err(|e| Error::Io(e))?;
        
        // Read data size (8)
        let data_size = file.read_u64_le().await.map_err(|e| Error::Io(e))?;
        
        // Read metadata length (4)
        let meta_len = file.read_u32_le().await.map_err(|e| Error::Io(e))?;
        
        // Skip metadata
        file.seek(std::io::SeekFrom::Current(meta_len as i64)).await.map_err(|e| Error::Io(e))?;
        
        // Read data
        let mut data = vec![0u8; data_size as usize];
        file.read_exact(&mut data).await.map_err(|e| Error::Io(e))?;
        
        Ok(Bytes::from(data))
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
    }
}
