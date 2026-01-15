//! Benchmarks for coordinator operations

use criterion::{criterion_group, criterion_main, Criterion, Throughput, BenchmarkId};
use coordinator::{CoordinatorService, CoordinatorServiceServer, CoordinatorClient};
use coordinator::proto::{WorkerInfo, BarrierRequest, DatasetInfo, ShardRequest};
use tonic::transport::Server;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

fn start_coordinator_for_bench(rt: &tokio::runtime::Runtime) -> String {
    let (addr, _shutdown) = rt.block_on(async {
        let service = CoordinatorService::new().await.unwrap();
        let port = portpicker::pick_unused_port().expect("No ports free");
        let addr = SocketAddr::from_str(&format!("127.0.0.1:{}", port)).unwrap();
        
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        let svc = CoordinatorServiceServer::new(service);
        let server = Server::builder()
            .add_service(svc)
            .serve_with_shutdown(addr, async {
                rx.await.ok();
            });

        tokio::spawn(server);
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        (format!("http://127.0.0.1:{}", port), tx)
    });
    addr
}

fn bench_worker_registration(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let addr = start_coordinator_for_bench(&rt);
    
    let mut group = c.benchmark_group("worker_registration");
    group.throughput(Throughput::Elements(1));
    
    group.bench_function("register_single_worker", |b| {
        let addr = addr.clone();
        b.to_async(&rt).iter(|| async {
            let mut client = CoordinatorClient::connect(addr.clone()).await.unwrap();
            client.register_worker(WorkerInfo {
                worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
                hostname: "127.0.0.1".to_string(),
                port: 8080,
                gpu_count: 4,
                memory_bytes: 32 * 1024 * 1024 * 1024,
                metadata: Default::default(),
            }).await.unwrap();
        });
    });
    
    group.finish();
}

fn bench_heartbeat_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let addr = start_coordinator_for_bench(&rt);
    
    // Register workers first
    let worker_ids: Vec<String> = (0..10)
        .map(|i| format!("worker-{}", i))
        .collect();
    
    rt.block_on(async {
        let mut client = CoordinatorClient::connect(addr.clone()).await.unwrap();
        for worker_id in &worker_ids {
            client.register_worker(WorkerInfo {
                worker_id: worker_id.clone(),
                hostname: "127.0.0.1".to_string(),
                port: 8080,
                gpu_count: 4,
                memory_bytes: 32 * 1024 * 1024 * 1024,
                metadata: Default::default(),
            }).await.unwrap();
        }
    });
    
    let mut group = c.benchmark_group("heartbeat");
    group.throughput(Throughput::Elements(1));
    
    group.bench_function("heartbeat_10_workers", |b| {
        let addr = addr.clone();
        let worker_ids = worker_ids.clone();
        b.to_async(&rt).iter(|| async {
            let mut client = CoordinatorClient::connect(addr.clone()).await.unwrap();
            for worker_id in &worker_ids {
                client.heartbeat(coordinator::proto::HeartbeatRequest {
                    worker_id: worker_id.clone(),
                }).await.unwrap();
            }
        });
    });
    
    group.finish();
}

fn bench_barrier_synchronization(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("barrier_sync");
    
    for num_workers in [2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_workers),
            num_workers,
            |b, &workers| {
                b.iter_batched(
                    || start_coordinator_for_bench(&rt),
                    |addr| {
                        rt.block_on(async {
                            let barrier_id = format!("barrier-{}", uuid::Uuid::new_v4());
                            let mut handles = vec![];
                            
                            for i in 0..workers {
                                let addr = addr.clone();
                                let barrier_id = barrier_id.clone();
                                let handle = tokio::spawn(async move {
                                    let mut client = CoordinatorClient::connect(addr).await.unwrap();
                                    client.wait_barrier(BarrierRequest {
                                        worker_id: format!("worker-{}", i),
                                        barrier_id,
                                        step: 1,
                                    }).await.unwrap();
                                });
                                handles.push(handle);
                            }
                            
                            for handle in handles {
                                handle.await.unwrap();
                            }
                        });
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    
    group.finish();
}

fn bench_data_shard_assignment(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let addr = start_coordinator_for_bench(&rt);
    
    // Register dataset
    rt.block_on(async {
        let mut client = CoordinatorClient::connect(addr.clone()).await.unwrap();
        client.register_dataset(DatasetInfo {
            dataset_id: "bench-dataset".to_string(),
            path: "/tmp/bench".to_string(),
            format: "parquet".to_string(),
            total_samples: 1_000_000,
            shard_size: 10000,
            shuffle: false,
            seed: 42,
            metadata: Default::default(),
        }).await.unwrap();
    });
    
    let mut group = c.benchmark_group("shard_assignment");
    group.throughput(Throughput::Elements(1));
    
    group.bench_function("get_shard", |b| {
        let addr = addr.clone();
        b.to_async(&rt).iter(|| async {
            let mut client = CoordinatorClient::connect(addr.clone()).await.unwrap();
            client.get_data_shard(ShardRequest {
                dataset_id: "bench-dataset".to_string(),
                worker_id: format!("worker-{}", uuid::Uuid::new_v4()),
                epoch: 0,
            }).await.unwrap();
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_worker_registration,
    bench_heartbeat_processing,
    bench_barrier_synchronization,
    bench_data_shard_assignment,
);
criterion_main!(benches);
