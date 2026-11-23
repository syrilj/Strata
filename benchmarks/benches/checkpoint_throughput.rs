//! Benchmarks for checkpoint persistence on local storage.
//!
//! What each group measures:
//!
//! - `local_storage_write/<size>`: one `LocalStorage::write` call, i.e. the I/O
//!   path the checkpoint writer task uses: write a temp file, `fsync`, then
//!   atomically rename it into place. Reported throughput is durable bytes/s.
//! - `local_storage_concurrent_write/<n>`: `n` concurrent 1 MB `LocalStorage::write`
//!   calls into the same directory. Reported throughput is total bytes/s.
//! - `checkpoint_save_async_call/<size>`: wall time of the `save_async` call as seen
//!   by the caller. `save_async` only queues the write for a background task, so
//!   this is *not* a disk throughput number; it shows how long the training loop
//!   is blocked. Manager construction and waiting for the write to land are
//!   excluded from the timing.
//!
//! All numbers are for whatever filesystem backs the OS temp directory.

use bytes::Bytes;
use checkpoint::{CheckpointManager, CheckpointManagerConfig};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use runtime_core::CheckpointType;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{LocalStorage, StorageBackend};
use tempfile::TempDir;

const MB: usize = 1_000_000;

fn local_storage_write(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let storage = LocalStorage::new(temp_dir.path());

    let mut group = c.benchmark_group("local_storage_write");
    group.sample_size(20);

    for size in [MB, 10 * MB, 100 * MB] {
        let data = Bytes::from(vec![0xABu8; size]);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(format!("{}MB", size / MB), |b| {
            b.iter_custom(|iters| {
                rt.block_on(async {
                    let mut total = Duration::ZERO;
                    for i in 0..iters {
                        // Bytes::clone is a refcount bump, not a copy.
                        let key = format!("ckpt-{}-{}.bin", size, i);
                        let start = Instant::now();
                        storage.write(&key, data.clone()).await.unwrap();
                        total += start.elapsed();
                        // Untimed: remove the file so the benchmark does not fill the disk.
                        storage.delete(&key).await.unwrap();
                    }
                    total
                })
            });
        });
    }

    group.finish();
}

fn local_storage_concurrent_write(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(LocalStorage::new(temp_dir.path()));
    let data = Bytes::from(vec![0xABu8; MB]);

    let mut group = c.benchmark_group("local_storage_concurrent_write");
    group.sample_size(20);

    for writers in [1usize, 4, 8] {
        group.throughput(Throughput::Bytes((writers * MB) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(writers),
            &writers,
            |b, &writers| {
                b.iter_custom(|iters| {
                    rt.block_on(async {
                        let mut total = Duration::ZERO;
                        for _ in 0..iters {
                            let keys: Vec<String> = (0..writers)
                                .map(|w| format!("concurrent-{}-{}.bin", writers, w))
                                .collect();
                            let start = Instant::now();
                            let mut handles = Vec::with_capacity(writers);
                            for key in &keys {
                                let key = key.clone();
                                let storage = Arc::clone(&storage);
                                let data = data.clone();
                                handles.push(tokio::spawn(async move {
                                    storage.write(&key, data).await.unwrap();
                                }));
                            }
                            for h in handles {
                                h.await.unwrap();
                            }
                            total += start.elapsed();
                            // Untimed cleanup.
                            for key in &keys {
                                storage.delete(key).await.unwrap();
                            }
                        }
                        total
                    })
                });
            },
        );
    }

    group.finish();
}

fn checkpoint_save_async_call(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("checkpoint_save_async_call");
    group.sample_size(20);

    for size in [MB, 10 * MB] {
        let data = Bytes::from(vec![0xABu8; size]);
        group.bench_function(format!("{}MB", size / MB), |b| {
            b.iter_custom(|iters| {
                rt.block_on(async {
                    let mut total = Duration::ZERO;
                    for step in 0..iters {
                        // Untimed: fresh manager so the queue is always empty.
                        let temp_dir = TempDir::new().unwrap();
                        let manager = CheckpointManager::new(CheckpointManagerConfig {
                            base_path: PathBuf::from(temp_dir.path()),
                            keep_count: 5,
                            write_buffer_size: 64 * 1024 * 1024,
                            compression: false,
                            compression_level: 3,
                        })
                        .await
                        .unwrap();

                        // Timed: only the call the training loop would make.
                        let start = Instant::now();
                        manager
                            .save_async(data.clone(), step, 0, CheckpointType::Full, HashMap::new())
                            .await
                            .unwrap();
                        total += start.elapsed();

                        // Untimed: let the background write finish before the
                        // temp dir is removed, so iterations don't overlap.
                        manager.wait_pending().await.unwrap();
                    }
                    total
                })
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    local_storage_write,
    local_storage_concurrent_write,
    checkpoint_save_async_call,
);
criterion_main!(benches);
