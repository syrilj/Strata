//! Benchmarks for checkpoint write and read throughput

use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use checkpoint::{CheckpointManager, CheckpointManagerConfig};
use runtime_core::CheckpointType;
use tempfile::TempDir;
use bytes::Bytes;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;

fn checkpoint_write_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("checkpoint_write");
    
    for size in [1_000_000, 10_000_000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        
        group.bench_function(format!("{}MB", size / 1_000_000), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let temp_dir = TempDir::new().unwrap();
                    let config = CheckpointManagerConfig {
                        base_path: PathBuf::from(temp_dir.path()),
                        keep_count: 5,
                        write_buffer_size: 64 * 1024 * 1024,
                        compression: false,
                        compression_level: 3,
                    };
                    let manager = CheckpointManager::new(config).await.unwrap();
                    
                    let data = vec![0u8; *size];
                    manager.save_async(
                        Bytes::from(data),
                        0, // step
                        0, // epoch
                        CheckpointType::Full,
                        HashMap::new(),
                    ).await.unwrap();
                });
            });
        });
    }
    
    group.finish();
}

fn checkpoint_concurrent_writes(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("checkpoint_concurrent");
    
    for num_workers in [1, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_workers),
            num_workers,
            |b, &workers| {
                b.iter(|| {
                    rt.block_on(async {
                        let temp_dir = TempDir::new().unwrap();
                        let config = CheckpointManagerConfig {
                            base_path: PathBuf::from(temp_dir.path()),
                            keep_count: 100,
                            write_buffer_size: 64 * 1024 * 1024,
                            compression: false,
                            compression_level: 3,
                        };
                        let manager = Arc::new(CheckpointManager::new(config).await.unwrap());
                        
                        let mut handles = vec![];
                        for i in 0..workers {
                            let manager_ref = manager.clone();
                            let handle = tokio::spawn(async move {
                                let data = vec![0u8; 1_000_000];
                                manager_ref.save_async(
                                    Bytes::from(data),
                                    i as u64, // step
                                    i as u64, // epoch
                                    CheckpointType::Full,
                                    HashMap::new(),
                                ).await.unwrap();
                            });
                            handles.push(handle);
                        }
                        
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    checkpoint_write_benchmark,
    checkpoint_concurrent_writes,
);
criterion_main!(benches);
