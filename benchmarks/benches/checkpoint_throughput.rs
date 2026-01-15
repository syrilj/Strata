//! Benchmarks for checkpoint write and read throughput

use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use checkpoint::{CheckpointManager, CheckpointWriter};
use storage::{StorageBackend, LocalBackend};
use tempfile::TempDir;
use std::sync::Arc;
use bytes::Bytes;

fn checkpoint_write_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("checkpoint_write");
    
    for size in [1_000_000, 10_000_000, 100_000_000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        
        group.bench_function(format!("{}MB", size / 1_000_000), |b| {
            b.to_async(&rt).iter(|| async {
                let temp_dir = TempDir::new().unwrap();
                let backend = Arc::new(LocalBackend::new(temp_dir.path().to_str().unwrap()).unwrap());
                let writer = CheckpointWriter::new(backend);
                
                let data = vec![0u8; *size];
                writer.write_checkpoint(
                    "bench_checkpoint",
                    100,
                    Bytes::from(data)
                ).await.unwrap();
            });
        });
    }
    
    group.finish();
}

fn checkpoint_read_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("checkpoint_read");
    
    for size in [1_000_000, 10_000_000, 100_000_000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        
        // Setup: write checkpoint first
        let temp_dir = TempDir::new().unwrap();
        let backend = Arc::new(LocalBackend::new(temp_dir.path().to_str().unwrap()).unwrap());
        let writer = CheckpointWriter::new(backend.clone());
        
        let data = vec![0u8; *size];
        rt.block_on(async {
            writer.write_checkpoint(
                "bench_checkpoint",
                100,
                Bytes::from(data)
            ).await.unwrap();
        });
        
        group.bench_function(format!("{}MB", size / 1_000_000), |b| {
            let backend = backend.clone();
            b.to_async(&rt).iter(|| async {
                let manager = CheckpointManager::new(backend.clone());
                manager.load_checkpoint("bench_checkpoint", 100)
                    .await
                    .unwrap();
            });
        });
    }
    
    group.finish();
}

fn checkpoint_concurrent_writes(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("checkpoint_concurrent");
    
    for num_workers in [1, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_workers),
            num_workers,
            |b, &workers| {
                b.to_async(&rt).iter(|| async move {
                    let temp_dir = TempDir::new().unwrap();
                    let backend = Arc::new(LocalBackend::new(temp_dir.path().to_str().unwrap()).unwrap());
                    
                    let mut handles = vec![];
                    for i in 0..workers {
                        let backend = backend.clone();
                        let handle = tokio::spawn(async move {
                            let writer = CheckpointWriter::new(backend);
                            let data = vec![0u8; 1_000_000];
                            writer.write_checkpoint(
                                &format!("worker_{}", i),
                                100,
                                Bytes::from(data)
                            ).await.unwrap();
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    checkpoint_write_benchmark,
    checkpoint_read_benchmark,
    checkpoint_concurrent_writes,
);
criterion_main!(benches);
