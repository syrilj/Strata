//! Benchmarks for consistent hash operations

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use data_shard::ConsistentHash;

fn bench_add_node(c: &mut Criterion) {
    c.bench_function("add_node", |b| {
        b.iter(|| {
            let ring = ConsistentHash::new();
            ring.add_node("worker-1");
        })
    });
}

fn bench_get_node(c: &mut Criterion) {
    let ring = ConsistentHash::new();
    for i in 0..10 {
        ring.add_node(&format!("worker-{}", i));
    }

    c.bench_function("get_node", |b| b.iter(|| ring.get_node("some-key-12345")));
}

fn bench_get_shards_for_node(c: &mut Criterion) {
    let ring = ConsistentHash::new();
    for i in 0..10 {
        ring.add_node(&format!("worker-{}", i));
    }

    let mut group = c.benchmark_group("get_shards_for_node");

    for total_shards in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(total_shards),
            total_shards,
            |b, &shards| b.iter(|| ring.get_shards_for_node("worker-1", "dataset-1", shards)),
        );
    }

    group.finish();
}

fn bench_distribution_evenness(c: &mut Criterion) {
    c.bench_function("distribution_100k_shards", |b| {
        let ring = ConsistentHash::new();
        for i in 0..100 {
            ring.add_node(&format!("worker-{}", i));
        }

        b.iter(|| {
            let mut counts = std::collections::HashMap::new();
            for i in 0..100_000 {
                let node = ring.get_node_for_shard("dataset", i).unwrap();
                *counts.entry(node).or_insert(0) += 1;
            }
            counts
        })
    });
}

criterion_group!(
    benches,
    bench_add_node,
    bench_get_node,
    bench_get_shards_for_node,
    bench_distribution_evenness,
);
criterion_main!(benches);
