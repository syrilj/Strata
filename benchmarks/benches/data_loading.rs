//! Benchmarks for data loading and shard assignment

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use data_shard::{ConsistentHash, ShardManager};
use std::collections::HashMap;

fn bench_shard_assignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("shard_assignment");

    for num_workers in [10, 100, 1000].iter() {
        for total_shards in [100, 1000, 10000].iter() {
            group.throughput(Throughput::Elements(*total_shards as u64));

            group.bench_with_input(
                BenchmarkId::new(
                    format!("{}_workers", num_workers),
                    format!("{}_shards", total_shards),
                ),
                &(num_workers, total_shards),
                |b, &(&workers, &shards)| {
                    b.iter(|| {
                        let manager = ShardManager::new();
                        manager.register_dataset_params(
                            "dataset-1",
                            shards as u64 * 1000,
                            1000,
                            true,
                            42,
                        );

                        for i in 0..workers {
                            manager.register_worker(&format!("worker-{}", i));
                        }

                        // Get shard assignments for all workers
                        for i in 0..workers {
                            manager.get_shard_for_worker("dataset-1", &format!("worker-{}", i), 0);
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
            let manager = ShardManager::new();
            manager.register_dataset_params("dataset-1", 100000, 100, true, 42);

            for i in 0..10 {
                manager.register_worker(&format!("worker-{}", i));
            }

            for epoch in 0..100u64 {
                for worker in 0..10 {
                    manager.get_shard_for_worker("dataset-1", &format!("worker-{}", worker), epoch);
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
                    for shard in 0u64..10000 {
                        if let Some(node) = ring.get_node_for_shard("dataset-1", shard) {
                            *distribution.entry(node.to_string()).or_insert(0) += 1;
                        }
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
            let mut before: HashMap<String, Vec<u64>> = HashMap::new();
            for shard in 0u64..10000 {
                if let Some(node) = ring.get_node_for_shard("dataset-1", shard) {
                    before.entry(node.to_string()).or_default().push(shard);
                }
            }

            // Add new worker
            ring.add_node("worker-new");

            // Calculate new distribution
            let mut after: HashMap<String, Vec<u64>> = HashMap::new();
            for shard in 0u64..10000 {
                if let Some(node) = ring.get_node_for_shard("dataset-1", shard) {
                    after.entry(node.to_string()).or_default().push(shard);
                }
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
                b.iter(|| {
                    rt.block_on(async {
                        let manager = std::sync::Arc::new(ShardManager::new());
                        manager.register_dataset_params("dataset-1", 100000, 10, true, 42);

                        for i in 0..threads {
                            manager.register_worker(&format!("worker-{}", i));
                        }

                        let mut handles = vec![];

                        for i in 0..threads {
                            let manager = manager.clone();
                            let handle = tokio::spawn(async move {
                                for epoch in 0..10u64 {
                                    manager.get_shard_for_worker(
                                        "dataset-1",
                                        &format!("worker-{}", i),
                                        epoch,
                                    );
                                }
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
    bench_shard_assignment,
    bench_epoch_management,
    bench_consistent_hash_distribution,
    bench_shard_rebalancing,
    bench_parallel_shard_access,
);
criterion_main!(benches);
