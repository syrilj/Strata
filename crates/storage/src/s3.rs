//! S3 storage backend
//!
//! Provides async S3-compatible storage with:
//! - Multipart uploads for large files
//! - Exponential backoff retry logic
//! - Custom endpoint support (for MinIO, LocalStack, etc.)

use std::time::Duration;

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    config::Builder as S3ConfigBuilder,
    primitives::ByteStream,
    types::{CompletedMultipartUpload, CompletedPart},
    Client,
};
use bytes::Bytes;
use runtime_core::{Error, Result};
use tracing::{debug, instrument, warn};

use crate::StorageBackend;

/// Threshold for switching to multipart upload (5 MB)
const MULTIPART_THRESHOLD: usize = 5 * 1024 * 1024;

/// Part size for multipart uploads (5 MB minimum required by S3)
const MULTIPART_PART_SIZE: usize = 5 * 1024 * 1024;

/// Maximum retry attempts for transient failures
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (milliseconds)
const BASE_RETRY_DELAY_MS: u64 = 100;

/// S3-compatible storage backend
///
/// Supports Amazon S3 and S3-compatible services like MinIO.
#[derive(Debug, Clone)]
pub struct S3Storage {
    client: Client,
    bucket: String,
    prefix: String,
}

/// Configuration for S3Storage
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 bucket name
    pub bucket: String,
    /// Optional prefix for all paths (e.g., "training-data/")
    pub prefix: Option<String>,
    /// Optional custom endpoint URL (for MinIO, LocalStack, etc.)
    pub endpoint_url: Option<String>,
    /// AWS region (default: "us-east-1")
    pub region: Option<String>,
    /// Force path-style addressing (required for MinIO)
    pub force_path_style: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            prefix: None,
            endpoint_url: None,
            region: Some("us-east-1".to_string()),
            force_path_style: false,
        }
    }
}

impl S3Storage {
    /// Create a new S3Storage with default AWS configuration
    ///
    /// Uses environment variables or instance profile for credentials.
    pub async fn new(bucket: impl Into<String>) -> Self {
        Self::with_config(S3Config {
            bucket: bucket.into(),
            ..Default::default()
        })
        .await
    }

    /// Create a new S3Storage with custom configuration
    pub async fn with_config(config: S3Config) -> Self {
        let aws_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(
                config.region.unwrap_or_else(|| "us-east-1".to_string()),
            ))
            .load()
            .await;

        let mut s3_config_builder = S3ConfigBuilder::from(&aws_config);

        if let Some(endpoint) = &config.endpoint_url {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
        }

        if config.force_path_style {
            s3_config_builder = s3_config_builder.force_path_style(true);
        }

        let client = Client::from_conf(s3_config_builder.build());

        Self {
            client,
            bucket: config.bucket,
            prefix: config.prefix.unwrap_or_default(),
        }
    }

    /// Create S3Storage for MinIO (convenience constructor)
    pub async fn minio(endpoint: &str, bucket: &str) -> Self {
        Self::with_config(S3Config {
            bucket: bucket.to_string(),
            endpoint_url: Some(endpoint.to_string()),
            force_path_style: true,
            ..Default::default()
        })
        .await
    }

    /// Get the full S3 key for a path
    fn s3_key(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", self.prefix.trim_end_matches('/'), path)
        }
    }

    /// Execute an async operation with exponential backoff retry
    async fn with_retry<T, F, Fut>(&self, operation: &str, f: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !e.is_retryable() {
                        return Err(e);
                    }

                    let delay = Duration::from_millis(BASE_RETRY_DELAY_MS * (1 << attempt));
                    warn!(
                        %operation,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "Retrying after transient error"
                    );

                    tokio::time::sleep(delay).await;
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Storage {
            message: format!("{} failed after {} retries", operation, MAX_RETRIES),
        }))
    }

    /// Perform multipart upload for large files
    async fn multipart_upload(&self, key: &str, data: Bytes) -> Result<u64> {
        let size = data.len() as u64;

        // Initiate multipart upload
        let create_result = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| Error::Storage {
                message: format!("Failed to initiate multipart upload: {}", e),
            })?;

        let upload_id = create_result.upload_id().ok_or_else(|| Error::Storage {
            message: "No upload_id returned".to_string(),
        })?;

        debug!(key, upload_id, size, "Started multipart upload");

        let mut completed_parts = Vec::new();
        let mut offset = 0;
        let mut part_number = 1;

        while offset < data.len() {
            let end = std::cmp::min(offset + MULTIPART_PART_SIZE, data.len());
            let part_data = data.slice(offset..end);

            let upload_part_result = self
                .client
                .upload_part()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(upload_id)
                .part_number(part_number)
                .body(ByteStream::from(part_data.to_vec()))
                .send()
                .await
                .map_err(|e| {
                    // Attempt to abort the upload on failure
                    self.abort_multipart_upload(key, upload_id);
                    Error::Storage {
                        message: format!("Failed to upload part {}: {}", part_number, e),
                    }
                })?;

            let etag = upload_part_result.e_tag().map(String::from);
            completed_parts.push(
                CompletedPart::builder()
                    .part_number(part_number)
                    .set_e_tag(etag)
                    .build(),
            );

            debug!(part_number, offset, end, "Uploaded part");
            offset = end;
            part_number += 1;
        }

        // Complete multipart upload
        let completed_upload = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        self.client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed_upload)
            .send()
            .await
            .map_err(|e| Error::Storage {
                message: format!("Failed to complete multipart upload: {}", e),
            })?;

        debug!(key, size, "Completed multipart upload");
        Ok(size)
    }

    /// Abort a multipart upload (best effort, for cleanup)
    fn abort_multipart_upload(&self, key: &str, upload_id: &str) {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = key.to_string();
        let upload_id = upload_id.to_string();

        tokio::spawn(async move {
            let _ = client
                .abort_multipart_upload()
                .bucket(&bucket)
                .key(&key)
                .upload_id(&upload_id)
                .send()
                .await;
        });
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    #[instrument(skip(self), fields(backend = "s3", bucket = %self.bucket))]
    async fn read(&self, path: &str) -> Result<Bytes> {
        let key = self.s3_key(path);
        debug!(%key, "Reading from S3");

        self.with_retry("read", || async {
            let result = self
                .client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| {
                    if e.to_string().contains("NoSuchKey") {
                        Error::StoragePathNotFound {
                            path: path.to_string(),
                        }
                    } else {
                        Error::Storage {
                            message: format!("S3 get_object failed: {}", e),
                        }
                    }
                })?;

            let bytes = result.body.collect().await.map_err(|e| Error::Storage {
                message: format!("Failed to read S3 response body: {}", e),
            })?;

            Ok(Bytes::from(bytes.to_vec()))
        })
        .await
    }

    #[instrument(skip(self, data), fields(backend = "s3", bucket = %self.bucket, size = data.len()))]
    async fn write(&self, path: &str, data: Bytes) -> Result<u64> {
        let key = self.s3_key(path);
        let size = data.len();
        debug!(%key, size, "Writing to S3");

        if size > MULTIPART_THRESHOLD {
            return self.multipart_upload(&key, data).await;
        }

        self.with_retry("write", || {
            let data = data.clone();
            let key = key.clone();
            async move {
                self.client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .body(ByteStream::from(data.to_vec()))
                    .send()
                    .await
                    .map_err(|e| Error::Storage {
                        message: format!("S3 put_object failed: {}", e),
                    })?;

                Ok(size as u64)
            }
        })
        .await
    }

    #[instrument(skip(self), fields(backend = "s3", bucket = %self.bucket))]
    async fn delete(&self, path: &str) -> Result<()> {
        let key = self.s3_key(path);
        debug!(%key, "Deleting from S3");

        self.with_retry("delete", || async {
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Storage {
                    message: format!("S3 delete_object failed: {}", e),
                })?;

            Ok(())
        })
        .await
    }

    #[instrument(skip(self), fields(backend = "s3", bucket = %self.bucket))]
    async fn exists(&self, path: &str) -> Result<bool> {
        let key = self.s3_key(path);
        debug!(%key, "Checking existence in S3");

        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.to_string().contains("NotFound") || e.to_string().contains("404") {
                    Ok(false)
                } else {
                    Err(Error::Storage {
                        message: format!("S3 head_object failed: {}", e),
                    })
                }
            }
        }
    }

    #[instrument(skip(self), fields(backend = "s3", bucket = %self.bucket))]
    async fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let s3_prefix = self.s3_key(prefix);
        debug!(%s3_prefix, "Listing S3 objects");

        let mut results = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&s3_prefix);

            if let Some(token) = continuation_token.take() {
                request = request.continuation_token(token);
            }

            let response = request.send().await.map_err(|e| Error::Storage {
                message: format!("S3 list_objects failed: {}", e),
            })?;

            for object in response.contents() {
                if let Some(key) = object.key() {
                    // Remove the prefix to return relative paths
                    let relative = if self.prefix.is_empty() {
                        key.to_string()
                    } else {
                        key.strip_prefix(&format!("{}/", self.prefix.trim_end_matches('/')))
                            .unwrap_or(key)
                            .to_string()
                    };
                    results.push(relative);
                }
            }

            if response.is_truncated() == Some(true) {
                continuation_token = response.next_continuation_token().map(String::from);
            } else {
                break;
            }
        }

        debug!(count = results.len(), "Found S3 objects");
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to test s3_key logic without needing a real client
    fn make_s3_key(prefix: &str, path: &str) -> String {
        if prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{}", prefix.trim_end_matches('/'), path)
        }
    }

    #[test]
    fn test_s3_key_with_prefix() {
        let prefix = "training/";
        assert_eq!(make_s3_key(prefix, "model.bin"), "training/model.bin");
        assert_eq!(
            make_s3_key(prefix, "checkpoints/epoch-1.bin"),
            "training/checkpoints/epoch-1.bin"
        );
    }

    #[test]
    fn test_s3_key_without_prefix() {
        let prefix = "";
        assert_eq!(make_s3_key(prefix, "model.bin"), "model.bin");
    }

    #[test]
    fn test_s3_key_trailing_slash_normalization() {
        // Test that trailing slashes are handled correctly
        let prefix = "data/";
        assert_eq!(make_s3_key(prefix, "file.bin"), "data/file.bin");
        
        let prefix_no_slash = "data";
        assert_eq!(make_s3_key(prefix_no_slash, "file.bin"), "data/file.bin");
    }

    #[test]
    fn test_s3_config_default() {
        let config = S3Config::default();
        assert!(config.bucket.is_empty());
        assert!(config.prefix.is_none());
        assert!(config.endpoint_url.is_none());
        assert_eq!(config.region, Some("us-east-1".to_string()));
        assert!(!config.force_path_style);
    }

    #[test]
    fn test_s3_config_builder() {
        let config = S3Config {
            bucket: "my-bucket".to_string(),
            prefix: Some("checkpoints/".to_string()),
            endpoint_url: Some("http://localhost:9000".to_string()),
            region: Some("us-west-2".to_string()),
            force_path_style: true,
        };

        assert_eq!(config.bucket, "my-bucket");
        assert_eq!(config.prefix, Some("checkpoints/".to_string()));
        assert_eq!(config.endpoint_url, Some("http://localhost:9000".to_string()));
        assert_eq!(config.region, Some("us-west-2".to_string()));
        assert!(config.force_path_style);
    }
}
