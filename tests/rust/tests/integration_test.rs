use anyhow::Result;
use coordinator::CoordinatorService;
use coordinator::CoordinatorServiceServer; // The generated gRPC server wrapper
use coordinator::CoordinatorClient; // The generated gRPC client
use coordinator::proto::{
    WorkerInfo, DatasetInfo, ShardRequest, CheckpointInfo, BarrierRequest,
    CheckpointType, RecoveryRequest,
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use tonic::transport::Server;
use std::str::FromStr;

// Helper to start coordinator on a random port and return the address + shutdown sender
async fn start_coordinator() -> Result<(String, tokio::sync::oneshot::Sender<()>)> {
    let service = CoordinatorService::new().await
        .map_err(|e| anyhow::anyhow!(e))?;
    
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = SocketAddr::from_str(&format!("127.0.0.1:{}", port))?;
    
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    // We use the generated CoordinatorServiceServer to wrap our implementation
    let svc = CoordinatorServiceServer::new(service);

    let server = Server::builder()
        .add_service(svc)
        .serve_with_shutdown(addr, async {
            rx.await.ok();
        });

    tokio::spawn(server);
    
    // Give it a moment to start
    sleep(Duration::from_millis(100)).await;
    
    Ok((format!("http://127.0.0.1:{}", port), tx))
}

#[tokio::test]
async fn test_full_flow() -> Result<()> {
    // 1. Start Coordinator
    let (addr, _shutdown) = start_coordinator().await?;
    
    // 2. Connect Client
    let mut client = CoordinatorClient::connect(addr.clone()).await?;
    
    // 3. Register Worker
    let worker_id = "test-worker-01";
    let resp = client.register_worker(WorkerInfo {
        worker_id: worker_id.to_string(),
        hostname: "127.0.0.1".to_string(),
        port: 8080,
        gpu_count: 0,
        memory_bytes: 1024,
        metadata: Default::default(),
    }).await?;
    assert!(!resp.get_ref().assigned_id.is_empty());
    
    // 4. Register Dataset
    let dataset_id = "test-dataset";
    // Ensure the path exists so directory checks passed if any (though currently it might not check existence thoroughly in simple impl)
    // But let's use a temp dir just in case
    let temp_dir = tempfile::tempdir()?;
    let dataset_path = temp_dir.path().to_str().unwrap();
    
    let resp = client.register_dataset(DatasetInfo {
        dataset_id: dataset_id.to_string(),
        path: dataset_path.to_string(),
        format: "parquet".to_string(),
        total_samples: 100,
        shard_size: 10,
        shuffle: false,
        seed: 42,
        metadata: Default::default(),
    }).await?;
    assert!(resp.get_ref().success);
    
    // 5. Get Data Shard
    let resp = client.get_data_shard(ShardRequest {
        dataset_id: dataset_id.to_string(),
        worker_id: worker_id.to_string(),
        epoch: 0,
    }).await?;
    let shard = resp.get_ref();
    assert_eq!(shard.shard_id, 0);
    // Single worker might get all shards or one depending on logic, but let's assume valid response
   
    // 6. Checkpoint Coordination
    // Notify complete
    let _ = client.notify_checkpoint(CheckpointInfo {
        worker_id: worker_id.to_string(),
        checkpoint_id: "ckpt-100".to_string(),
        step: 100,
        epoch: 0,
        storage_path: "/tmp/ckpt/100".to_string(),
        size_bytes: 1024,
        timestamp_ms: 0,
        r#type: CheckpointType::Full as i32,
        metadata: Default::default(),
    }).await?;
    
    // Get latest
    let resp = client.get_latest_checkpoint(RecoveryRequest {
        worker_id: worker_id.to_string(),
        job_id: "test-job".to_string(),
    }).await?;
    let recovery = resp.get_ref();
    assert!(recovery.has_checkpoint);
    if let Some(ckpt) = &recovery.latest_checkpoint {
        assert_eq!(ckpt.step, 100);
        assert_eq!(ckpt.storage_path, "/tmp/ckpt/100");
    } else {
        panic!("No checkpoint returned");
    }
    
    Ok(())
}

#[tokio::test]
async fn test_barrier() -> Result<()> {
    let (addr, _shutdown) = start_coordinator().await?;
    
    let barrier_id = "sync-step-1";
    
    // Use Arc to share address for spawned tasks
    let addr = std::sync::Arc::new(addr);
    
    // Spawn two tasks waiting on barrier
    let addr1 = addr.clone();
    let h1 = tokio::spawn(async move {
        let mut client = CoordinatorClient::connect(addr1.as_str().to_string()).await.expect("Failed to connect");
        client.wait_barrier(BarrierRequest {
            worker_id: "w1".to_string(),
            barrier_id: barrier_id.to_string(),
            step: 1,
        }).await
    });
    
    let addr2 = addr.clone();
    let h2 = tokio::spawn(async move {
        let mut client = CoordinatorClient::connect(addr2.as_str().to_string()).await.expect("Failed to connect");
        // Add a small delay for one client to ensure barrier wait logic is exercised
        sleep(Duration::from_millis(100)).await;
        client.wait_barrier(BarrierRequest {
            worker_id: "w2".to_string(),
            barrier_id: barrier_id.to_string(),
            step: 1,
        }).await
    });
    
    // Wait for tasks
    let (r1, r2) = tokio::join!(h1, h2);
    
    // Unwrap the join results
    let res1 = r1?;
    let res2 = r2?;
    
    assert!(res1.is_ok(), "Client 1 barrier failed: {:?}", res1.err());
    assert!(res2.is_ok(), "Client 2 barrier failed: {:?}", res2.err());
    
    Ok(())
}
