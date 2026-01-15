//! Benchmarks for data loading and shard assignment

use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use data_shard::{ShardManager, Epoch, ConsistentHash};
use std::collections::HashMap;

fn bench_shard_assignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_assignment");
    
    for num_workers in [10, 100, 1000].iter() {
        for total_shards in [100, 1000, 10000].iter() {
            group.throughput(Throughput::Elements(*total_shards as u64));
            
            group.bench_with_input(
                BenchmarkId::new(
                    format!("{}_workers", num_workers),
                    format!("{}_shards", total_shards)
                ),
                &(num_workers, total_shards),
                |b, &(&workers, &shards)| {
                    b.iter(|| {
                        let manager = ShardManager::new("dataset-1", shards);
                        
                        for i in 0..workers {
                            manager.assign_shards(0, &format!("worker-{}", i));
                        }
                    });
                },
            );
        }
    }
    
    group.finish();
}

fn bench_epoch_management(c: &mut Criterion) {
    let mut group = c.benchmark_group("epoch_management");
    
    group.bench_function("create_and_track_100_epochs", |b| {
        b.iter(|| {
            let manager = ShardManager::new("dataset-1", 1000);
            
            for epoch in 0..100 {
                for worker in 0..10 {
                    manager.assign_shards(epoch, &format!("worker-{}", worker));
                }
            }
        });
    });
    
    group.finish();
}

fn bench_consistent_hash_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("consistent_hash_distribution");
    
    for num_workers in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_workers),
            num_workers,
            |b, &workers| {
                b.iter(|| {
                    let ring = ConsistentHash::new();
                    
                    // Add workers
                    for i in 0..workers {
                        ring.add_node(&format!("worker-{}", i));
                    }
                    
                    // Distribute 10000 shards
                    let mut distribution: HashMap<String, usize> = HashMap::new();
                    for shard in 0..10000 {
                        let node = ring.get_node_for_shard("dataset-1", shard).unwrap();
                        *distribution.entry(node.to_string()).or_insert(0) += 1;
                    }
                    
                    distribution
                });
            },
        );
    }
    
    group.finish();
}

fn bench_shard_rebalancing(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_rebalancing");
    
    group.bench_function("add_worker_to_100_worker_cluster", |b| {
        b.iter(|| {
            let ring = ConsistentHash::new();
            
            // Start with 100 workers
            for i in 0..100 {
                ring.add_node(&format!("worker-{}", i));
            }
            
            // Calculate initial distribution
            let mut before: HashMap<String, Vec<usize>> = HashMap::new();
            for shard in 0..10000 {
                let node = ring.get_node_for_shard("dataset-1", shard).unwrap();
                before.entry(node.to_string()).or_insert_with(Vec::new).push(shard);
            }
            
            // Add new worker
            ring.add_node("worker-new");
            
            // Calculate new distribution
            let mut after: HashMap<String, Vec<usize>> = HashMap::new();
            for shard in 0..10000 {
                let node = ring.get_node_for_shard("dataset-1", shard).unwrap();
                after.entry(node.to_string()).or_insert_with(Vec::new).push(shard);
            }
            
            // Count moved shards
            let mut moved = 0;
            for (node, shards) in &before {
                let new_shards = after.get(node).map(|s| s.len()).unwrap_or(0);
                if new_shards < shards.len() {
                    moved += shards.len() - new_shards;
                }
            }
            
            moved
        });
    });
    
    group.finish();
}

fn bench_parallel_shard_access(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("parallel_shard_access");
    
    for num_threads in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_threads),
            num_threads,
            |b, &threads| {
                b.to_async(&rt).iter(|| async move {
                    let manager = std::sync::Arc::new(ShardManager::new("dataset-1", 10000));
                    let mut handles = vec![];
                    
                    for i in 0..threads {
                        let manager = manager.clone();
                        let handle = tokio::spawn(async move {
                            for epoch in 0..10 {
                                manager.assign_shards(epoch, &format!("worker-{}", i));
                            }
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
    bench_shard_assignment,
    bench_epoch_management,
    bench_consistent_hash_distribution,
    bench_shard_rebalancing,
    bench_parallel_shard_access,
);
criterion_main!(benches);
