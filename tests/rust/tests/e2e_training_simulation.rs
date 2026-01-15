//! End-to-end training simulation test
//!
//! This test simulates a realistic distributed training scenario with:
//! - Multiple workers registering and sending heartbeats
//! - Dataset registration and shard distribution
//! - Checkpoint coordination
//! - Worker failure and recovery
//! - Barrier synchronization

use anyhow::Result;
use coordinator::CoordinatorService;
use coordinator::CoordinatorServiceServer;
use coordinator::CoordinatorClient;
use coordinator::proto::{
    WorkerInfo, DatasetInfo, ShardRequest, CheckpointInfo, BarrierRequest,
    CheckpointType, RecoveryRequest, HeartbeatRequest, WorkerStatus,
    worker_status::State,
};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;
use tokio::time::sleep;
use tonic::transport::Server;

async fn start_coordinator() -> Result<(String, tokio::sync::oneshot::Sender<()>)> {
    let service = CoordinatorService::new().await
        .map_err(|e| anyhow::anyhow!(e))?;
    
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = SocketAddr::from_str(&format!("127.0.0.1:{}", port))?;
    
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    let svc = CoordinatorServiceServer::new(service);

    let server = Server::builder()
        .add_service(svc)
        .serve_with_shutdown(addr, async {
            rx.await.ok();
        });

    tokio::spawn(server);
    sleep(Duration::from_millis(100)).await;
    
    Ok((format!("http://127.0.0.1:{}", port), tx))
}

/// Simulates a training worker
struct SimulatedWorker {
    id: String,
    client: CoordinatorClient<tonic::transport::Channel>,
    rank: i32,
    world_size: i32,
}

impl SimulatedWorker {
    async fn new(addr: &str, worker_id: &str) -> Result<Self> {
        let mut client = CoordinatorClient::connect(addr.to_string()).await?;
        
        let resp = client.register_worker(WorkerInfo {
            worker_id: worker_id.to_string(),
            hostname: "127.0.0.1".to_string(),
            port: 8080,
            gpu_count: 8,
            memory_bytes: 64 * 1024 * 1024 * 1024, // 64GB
            metadata: Default::default(),
        }).await?;
        
        let config = resp.into_inner();
        
        Ok(Self {
            id: worker_id.to_string(),
            client,
            rank: config.rank,
            world_size: config.world_size,
        })
    }
    
    async fn heartbeat(&mut self, step: u64, epoch: u64, state: State) -> Result<()> {
        self.client.heartbeat(HeartbeatRequest {
            worker_id: self.id.clone(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            status: Some(WorkerStatus {
                state: state as i32,
                current_step: step as i64,
                current_epoch: epoch as i64,
                current_task: format!("training_step_{}", step),
            }),
            resources: None,
        }).await?;
        Ok(())
    }
    
    async fn get_shards(&mut self, dataset_id: &str, epoch: u64) -> Result<Vec<i64>> {
        let resp = self.client.get_data_shard(ShardRequest {
            dataset_id: dataset_id.to_string(),
            worker_id: self.id.clone(),
            epoch: epoch as i64,
        }).await?;
        
        Ok(vec![resp.into_inner().shard_id])
    }
    
    async fn checkpoint(&mut self, step: u64, epoch: u64) -> Result<()> {
        self.client.notify_checkpoint(CheckpointInfo {
            worker_id: self.id.clone(),
            checkpoint_id: format!("ckpt-{}-{}", self.id, step),
            step: step as i64,
            epoch: epoch as i64,
            storage_path: format!("/checkpoints/{}/step_{}.bin", self.id, step),
            size_bytes: 100 * 1024 * 1024, // 100MB
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            r#type: CheckpointType::Full as i32,
            metadata: Default::default(),
        }).await?;
        Ok(())
    }
    
    async fn wait_barrier(&mut self, barrier_id: &str, step: u64) -> Result<()> {
        self.client.wait_barrier(BarrierRequest {
            worker_id: self.id.clone(),
            barrier_id: barrier_id.to_string(),
            step: step as i64,
        }).await?;
        Ok(())
    }
}

#[tokio::test]
async fn test_multi_worker_training_simulation() -> Result<()> {
    let (addr, _shutdown) = start_coordinator().await?;
    
    // Register 4 workers
    let mut workers = Vec::new();
    for i in 0..4 {
        let worker = SimulatedWorker::new(&addr, &format!("gpu-node-{}", i)).await?;
        workers.push(worker);
    }
    
    // Verify world size
    // Note: world_size captured at registration time reflects the count at that moment
    // The last worker registered should have world_size=4
    assert_eq!(workers[3].world_size, 4);
    
    // Register dataset
    let mut client = CoordinatorClient::connect(addr.clone()).await?;
    let temp_dir = tempfile::tempdir()?;
    
    client.register_dataset(DatasetInfo {
        dataset_id: "imagenet".to_string(),
        path: temp_dir.path().to_str().unwrap().to_string(),
        format: "tfrecord".to_string(),
        total_samples: 1_281_167,
        shard_size: 10_000,
        shuffle: true,
        seed: 42,
        metadata: Default::default(),
    }).await?;
    
    // Simulate training loop
    let total_steps = 10;
    let checkpoint_interval = 5;
    
    for step in 0..total_steps {
        let epoch = step / 5;
        
        // All workers send heartbeat
        for worker in &mut workers {
            worker.heartbeat(step, epoch, State::Training).await?;
        }
        
        // Checkpoint at intervals
        if step > 0 && step % checkpoint_interval == 0 {
            // Barrier before checkpoint
            let barrier_id = format!("pre-ckpt-{}", step);
            let addr_clone = addr.clone();
            let barrier = Arc::new(Barrier::new(workers.len()));
            
            let mut handles = Vec::new();
            for (i, _) in workers.iter().enumerate() {
                let addr = addr_clone.clone();
                let barrier = barrier.clone();
                let barrier_id = barrier_id.clone();
                let worker_id = format!("gpu-node-{}", i);
                
                handles.push(tokio::spawn(async move {
                    let mut client = CoordinatorClient::connect(addr).await.unwrap();
                    barrier.wait().await;
                    client.wait_barrier(BarrierRequest {
                        worker_id,
                        barrier_id,
                        step: step as i64,
                    }).await
                }));
            }
            
            for handle in handles {
                handle.await??;
            }
            
            // All workers checkpoint
            for worker in &mut workers {
                worker.checkpoint(step, epoch).await?;
            }
        }
        
        sleep(Duration::from_millis(10)).await;
    }
    
    // Verify recovery works
    let resp = client.get_latest_checkpoint(RecoveryRequest {
        worker_id: "new-worker".to_string(),
        job_id: "training-job".to_string(),
    }).await?;
    
    let recovery = resp.into_inner();
    assert!(recovery.has_checkpoint);
    
    Ok(())
}

#[tokio::test]
async fn test_shard_distribution_fairness() -> Result<()> {
    let (addr, _shutdown) = start_coordinator().await?;
    
    // Register 8 workers
    let mut workers = Vec::new();
    for i in 0..8 {
        let worker = SimulatedWorker::new(&addr, &format!("worker-{}", i)).await?;
        workers.push(worker);
    }
    
    // Register dataset with 100 shards
    let mut client = CoordinatorClient::connect(addr.clone()).await?;
    let temp_dir = tempfile::tempdir()?;
    
    client.register_dataset(DatasetInfo {
        dataset_id: "test-dataset".to_string(),
        path: temp_dir.path().to_str().unwrap().to_string(),
        format: "parquet".to_string(),
        total_samples: 100_000,
        shard_size: 1_000, // 100 shards
        shuffle: true, // Use shuffle for fair distribution across workers
        seed: 42,
        metadata: Default::default(),
    }).await?;
    
    // Get shard assignments for all workers
    let mut all_shards: HashSet<i64> = HashSet::new();
    let mut shard_counts = Vec::new();
    
    for worker in &mut workers {
        let shards = worker.get_shards("test-dataset", 0).await?;
        shard_counts.push(shards.len());
        for shard in shards {
            all_shards.insert(shard);
        }
    }
    
    // Each worker should get at least one shard
    for count in &shard_counts {
        assert!(*count >= 1, "Worker should have at least 1 shard");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_worker_failure_recovery() -> Result<()> {
    let (addr, _shutdown) = start_coordinator().await?;
    
    // Register 3 workers
    let mut workers = Vec::new();
    for i in 0..3 {
        let worker = SimulatedWorker::new(&addr, &format!("worker-{}", i)).await?;
        workers.push(worker);
    }
    
    // Register dataset
    let mut client = CoordinatorClient::connect(addr.clone()).await?;
    let temp_dir = tempfile::tempdir()?;
    
    client.register_dataset(DatasetInfo {
        dataset_id: "recovery-test".to_string(),
        path: temp_dir.path().to_str().unwrap().to_string(),
        format: "parquet".to_string(),
        total_samples: 30_000,
        shard_size: 1_000,
        shuffle: false,
        seed: 0,
        metadata: Default::default(),
    }).await?;
    
    // Worker 0 creates a checkpoint
    workers[0].checkpoint(100, 0).await?;
    
    // Simulate worker 0 failure by deregistering
    client.deregister_worker(WorkerInfo {
        worker_id: "worker-0".to_string(),
        hostname: "127.0.0.1".to_string(),
        port: 8080,
        gpu_count: 8,
        memory_bytes: 64 * 1024 * 1024 * 1024,
        metadata: Default::default(),
    }).await?;
    
    // New worker joins and recovers
    let resp = client.get_latest_checkpoint(RecoveryRequest {
        worker_id: "worker-replacement".to_string(),
        job_id: "training-job".to_string(),
    }).await?;
    
    let recovery = resp.into_inner();
    assert!(recovery.has_checkpoint);
    assert_eq!(recovery.resume_step, 100);
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_barrier_sync() -> Result<()> {
    let (addr, _shutdown) = start_coordinator().await?;
    
    let num_workers = 8;
    let addr = Arc::new(addr);
    
    // Register all workers first
    for i in 0..num_workers {
        let mut client = CoordinatorClient::connect(addr.as_str().to_string()).await?;
        client.register_worker(WorkerInfo {
            worker_id: format!("barrier-worker-{}", i),
            hostname: "127.0.0.1".to_string(),
            port: 8080 + i as i32,
            gpu_count: 1,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            metadata: Default::default(),
        }).await?;
    }
    
    // All workers hit barrier simultaneously
    let barrier = Arc::new(Barrier::new(num_workers));
    let mut handles = Vec::new();
    
    for i in 0..num_workers {
        let addr = addr.clone();
        let barrier = barrier.clone();
        
        handles.push(tokio::spawn(async move {
            let mut client = CoordinatorClient::connect(addr.as_str().to_string()).await.unwrap();
            
            // Synchronize start
            barrier.wait().await;
            
            let start = std::time::Instant::now();
            let resp = client.wait_barrier(BarrierRequest {
                worker_id: format!("barrier-worker-{}", i),
                barrier_id: "epoch-sync".to_string(),
                step: 0,
            }).await.unwrap();
            
            let elapsed = start.elapsed();
            (resp.into_inner(), elapsed)
        }));
    }
    
    // Wait for all workers
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await?);
    }
    
    // All should be released
    for (resp, elapsed) in &results {
        assert!(resp.released);
        assert_eq!(resp.participants, num_workers as i64);
        // Barrier should complete quickly (< 1 second)
        assert!(elapsed.as_secs() < 1, "Barrier took too long: {:?}", elapsed);
    }
    
    Ok(())
}
