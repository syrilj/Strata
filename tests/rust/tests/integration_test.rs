use anyhow::Result;
use coordinator::proto::{
    BarrierRequest, CheckpointInfo, CheckpointType, DatasetInfo, RecoveryRequest, ShardRequest,
    WorkerInfo,
};
use coordinator::CoordinatorClient; // The generated gRPC client
use coordinator::CoordinatorService;
use coordinator::CoordinatorServiceServer; // The generated gRPC server wrapper
use std::time::Duration;
use tokio::time::sleep;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

// Helper to start coordinator on a random port and return the address + shutdown sender
async fn start_coordinator() -> Result<(String, tokio::sync::oneshot::Sender<()>)> {
    let service = CoordinatorService::new()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // Bind to an OS-assigned port (IPv4 loopback only, so this works without IPv6).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let (tx, rx) = tokio::sync::oneshot::channel();

    // We use the generated CoordinatorServiceServer to wrap our implementation
    let svc = CoordinatorServiceServer::new(service);

    let server = Server::builder()
        .add_service(svc)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
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
    let resp = client
        .register_worker(WorkerInfo {
            worker_id: worker_id.to_string(),
            hostname: "127.0.0.1".to_string(),
            port: 8080,
            gpu_count: 0,
            memory_bytes: 1024,
            metadata: Default::default(),
        })
        .await?;
    assert!(!resp.get_ref().assigned_id.is_empty());

    // 4. Register Dataset
    let dataset_id = "test-dataset";
    // Ensure the path exists so directory checks passed if any (though currently it might not check existence thoroughly in simple impl)
    // But let's use a temp dir just in case
    let temp_dir = tempfile::tempdir()?;
    let dataset_path = temp_dir.path().to_str().unwrap();

    let resp = client
        .register_dataset(DatasetInfo {
            dataset_id: dataset_id.to_string(),
            path: dataset_path.to_string(),
            format: "parquet".to_string(),
            total_samples: 100,
            shard_size: 10,
            shuffle: false,
            seed: 42,
            metadata: Default::default(),
        })
        .await?;
    assert!(resp.get_ref().success);

    // 5. Get Data Shard
    let resp = client
        .get_data_shard(ShardRequest {
            dataset_id: dataset_id.to_string(),
            worker_id: worker_id.to_string(),
            epoch: 0,
        })
        .await?;
    let shard = resp.get_ref();
    assert_eq!(shard.shard_id, 0);
    // Single worker might get all shards or one depending on logic, but let's assume valid response

    // 6. Checkpoint Coordination
    // Notify complete
    let _ = client
        .notify_checkpoint(CheckpointInfo {
            worker_id: worker_id.to_string(),
            checkpoint_id: "ckpt-100".to_string(),
            step: 100,
            epoch: 0,
            storage_path: "/tmp/ckpt/100".to_string(),
            size_bytes: 1024,
            timestamp_ms: 0,
            r#type: CheckpointType::Full as i32,
            metadata: Default::default(),
        })
        .await?;

    // Get latest
    let resp = client
        .get_latest_checkpoint(RecoveryRequest {
            worker_id: worker_id.to_string(),
            job_id: "test-job".to_string(),
        })
        .await?;
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

    // Register w1 and w2 before calling wait_barrier so expected participants = 2
    for (idx, worker_id) in ["w1", "w2"].iter().enumerate() {
        let mut client = CoordinatorClient::connect(addr.clone()).await?;
        client
            .register_worker(WorkerInfo {
                worker_id: (*worker_id).to_string(),
                hostname: "127.0.0.1".to_string(),
                port: 8080 + idx as i32,
                gpu_count: 1,
                memory_bytes: 1024,
                metadata: Default::default(),
            })
            .await?;
    }

    // Use Arc to share address for spawned tasks
    let addr = std::sync::Arc::new(addr);

    // Spawn two tasks waiting on barrier
    let addr1 = addr.clone();
    let h1 = tokio::spawn(async move {
        let mut client = CoordinatorClient::connect(addr1.as_str().to_string())
            .await
            .expect("Failed to connect");
        client
            .wait_barrier(BarrierRequest {
                worker_id: "w1".to_string(),
                barrier_id: barrier_id.to_string(),
                step: 1,
            })
            .await
    });

    let addr2 = addr.clone();
    let h2 = tokio::spawn(async move {
        let mut client = CoordinatorClient::connect(addr2.as_str().to_string())
            .await
            .expect("Failed to connect");
        // Add a small delay for one client to ensure barrier wait logic is exercised
        sleep(Duration::from_millis(100)).await;
        client
            .wait_barrier(BarrierRequest {
                worker_id: "w2".to_string(),
                barrier_id: barrier_id.to_string(),
                step: 1,
            })
            .await
    });

    // Wait for tasks
    let (r1, r2) = tokio::join!(h1, h2);

    // Unwrap the join results
    let res1 = r1?;
    let res2 = r2?;

    assert!(res1.is_ok(), "Client 1 barrier failed: {:?}", res1.err());
    assert!(res2.is_ok(), "Client 2 barrier failed: {:?}", res2.err());
    let b1 = res1?.into_inner();
    let b2 = res2?.into_inner();
    assert!(b1.released);
    assert!(b2.released);
    assert_eq!(b1.participants, 2);
    assert_eq!(b2.participants, 2);

    Ok(())
}

async fn send_http_json(
    router: &axum::Router,
    method: axum::http::Method,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (axum::http::StatusCode, serde_json::Value) {
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let builder = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };

    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let parsed = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, parsed)
}

#[tokio::test]
async fn test_http_rest_api_end_to_end() -> Result<()> {
    use axum::http::{Method, StatusCode};
    use coordinator::http_api::create_router;
    use std::sync::Arc;

    // Start gRPC + HTTP router backed by the same live-mode CoordinatorService (DEMO_MODE=false)
    let service = Arc::new(
        CoordinatorService::new()
            .await
            .map_err(|e| anyhow::anyhow!(e))?,
    );
    let router = create_router(Arc::clone(&service));

    // Bind to an OS-assigned port (IPv4 loopback only, so this works without IPv6).
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let grpc_svc = CoordinatorServiceServer::new((*service).clone());
    tokio::spawn(
        Server::builder()
            .add_service(grpc_svc)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = rx.await;
            }),
    );
    sleep(Duration::from_millis(100)).await;
    let grpc_url = format!("http://127.0.0.1:{}", port);

    // 1. Health & status
    let (status, health) = send_http_json(&router, Method::GET, "/api/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(health["status"], "ok");

    let (status, st) = send_http_json(&router, Method::GET, "/api/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(st["connected"], true);
    assert_eq!(st["demo_mode"], false);

    // 2. Register real workers via gRPC and inspect/delete via HTTP
    let mut client1 = CoordinatorClient::connect(grpc_url.clone()).await?;
    client1
        .register_worker(WorkerInfo {
            worker_id: "http-worker-1".to_string(),
            hostname: "127.0.0.1".to_string(),
            port: 50052,
            gpu_count: 4,
            memory_bytes: 16 * 1024 * 1024 * 1024,
            metadata: Default::default(),
        })
        .await?;

    let mut client2 = CoordinatorClient::connect(grpc_url.clone()).await?;
    client2
        .register_worker(WorkerInfo {
            worker_id: "http-worker-2".to_string(),
            hostname: "127.0.0.1".to_string(),
            port: 50053,
            gpu_count: 2,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            metadata: Default::default(),
        })
        .await?;

    let (status, workers) = send_http_json(&router, Method::GET, "/api/workers", None).await;
    assert_eq!(status, StatusCode::OK);
    let workers_arr = workers.as_array().unwrap();
    assert_eq!(workers_arr.len(), 2);
    assert!(workers_arr.iter().any(|w| w["id"] == "http-worker-1"));
    assert!(workers_arr.iter().any(|w| w["id"] == "http-worker-2"));

    // 3. Datasets: POST, GET, real CSV preview, shards, and DELETE
    let temp_dir = tempfile::tempdir()?;
    let csv_path = temp_dir.path().join("real_samples.csv");
    std::fs::write(
        &csv_path,
        "feature_a,feature_b,label\n10.5,20.25,alpha\n30.0,40.5,beta\n50.0,60.0,gamma\n",
    )?;

    let (status, reg_ds) = send_http_json(
        &router,
        Method::POST,
        "/api/datasets",
        Some(serde_json::json!({
            "id": "real-csv-ds",
            "name": "Real CSV Dataset",
            "path": csv_path.to_string_lossy(),
            "format": "csv",
            "shard_size": 2,
            "shuffle": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(reg_ds["success"], true);
    assert_eq!(reg_ds["dataset"]["total_samples"], 3);
    assert_eq!(reg_ds["dataset"]["shard_count"], 2);

    let (status, datasets) = send_http_json(&router, Method::GET, "/api/datasets", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(datasets.as_array().unwrap().len(), 1);
    assert_eq!(datasets[0]["id"], "real-csv-ds");

    let (status, preview) = send_http_json(
        &router,
        Method::GET,
        "/api/datasets/real-csv-ds/preview",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["source"], "local_files");
    assert_eq!(
        preview["columns"],
        serde_json::json!(["feature_a", "feature_b", "label"])
    );
    let samples = preview["samples"].as_array().unwrap();
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0]["features"]["feature_a"], 10.5);
    assert_eq!(samples[0]["label"], "alpha");
    assert_eq!(samples[1]["label"], "beta");
    assert_eq!(samples[2]["label"], "gamma");

    let (status, shards) = send_http_json(
        &router,
        Method::GET,
        "/api/datasets/real-csv-ds/shards",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shards["dataset_id"], "real-csv-ds");
    assert_eq!(shards["total_shards"], 2);

    // 4. Tasks: POST, GET, logs, stop
    let (status, task_resp) = send_http_json(
        &router,
        Method::POST,
        "/api/tasks",
        Some(serde_json::json!({
            "name": "live-training-run",
            "type": "tabular_training",
            "dataset_id": "real-csv-ds",
            "worker_count": 2,
            "config": { "epochs": 3 }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = task_resp["task_id"].as_str().unwrap().to_string();

    let (status, tasks) = send_http_json(&router, Method::GET, "/api/tasks", None).await;
    assert_eq!(status, StatusCode::OK);
    let tasks_arr = tasks.as_array().unwrap();
    assert_eq!(tasks_arr.len(), 1);
    assert_eq!(tasks_arr[0]["id"], task_id);
    assert_eq!(tasks_arr[0]["name"], "live-training-run");
    assert_eq!(tasks_arr[0]["status"], "running");

    let (status, task_logs) = send_http_json(
        &router,
        Method::GET,
        &format!("/api/tasks/{}/logs", task_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!task_logs.as_array().unwrap().is_empty());

    // 5. Barriers & Checkpoints via gRPC, then inspect/delete via HTTP
    let b1 = tokio::spawn({
        let url = grpc_url.clone();
        async move {
            let mut c = CoordinatorClient::connect(url).await.unwrap();
            c.wait_barrier(BarrierRequest {
                worker_id: "http-worker-1".to_string(),
                barrier_id: "http-barrier-1".to_string(),
                step: 1,
            })
            .await
            .unwrap()
        }
    });
    let b2 = tokio::spawn({
        let url = grpc_url.clone();
        async move {
            let mut c = CoordinatorClient::connect(url).await.unwrap();
            c.wait_barrier(BarrierRequest {
                worker_id: "http-worker-2".to_string(),
                barrier_id: "http-barrier-1".to_string(),
                step: 1,
            })
            .await
            .unwrap()
        }
    });
    let _ = tokio::try_join!(b1, b2)?;

    let (status, barriers) = send_http_json(&router, Method::GET, "/api/barriers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(barriers
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["id"] == "http-barrier-1"));

    client1
        .notify_checkpoint(CheckpointInfo {
            worker_id: "http-worker-1".to_string(),
            checkpoint_id: "http-ckpt-1".to_string(),
            step: 10,
            epoch: 1,
            storage_path: temp_dir
                .path()
                .join("ckpt_10.bin")
                .to_string_lossy()
                .to_string(),
            size_bytes: 2048,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            r#type: CheckpointType::Full as i32,
            metadata: Default::default(),
        })
        .await?;

    let (status, checkpoints) =
        send_http_json(&router, Method::GET, "/api/checkpoints", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(checkpoints
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == "http-ckpt-1"));

    // 6. Metrics, metrics/history, logs, and dashboard in live mode (no demo tasks/logs!)
    let (status, metrics) = send_http_json(&router, Method::GET, "/api/metrics", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(metrics["total_workers"], 2);

    let (status, history) =
        send_http_json(&router, Method::GET, "/api/metrics/history", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(history.is_array());

    let (status, logs) = send_http_json(&router, Method::GET, "/api/logs?limit=50", None).await;
    assert_eq!(status, StatusCode::OK);
    let logs_arr = logs.as_array().unwrap();
    assert!(!logs_arr.is_empty());
    assert!(!logs_arr.iter().any(|l| l["id"] == "demo_log_1"));

    let (status, dash) = send_http_json(&router, Method::GET, "/api/dashboard", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(dash["coordinator"]["demo_mode"], false);
    let dash_tasks = dash["tasks"].as_array().unwrap();
    assert_eq!(dash_tasks.len(), 1);
    assert_eq!(dash_tasks[0]["id"], task_id);
    assert!(!dash_tasks.iter().any(|t| t["id"] == "task_vision_training"));
    let dash_logs = dash["logs"].as_array().unwrap();
    assert!(!dash_logs.is_empty());
    assert!(!dash_logs.iter().any(|l| l["id"] == "demo_log_1"));

    // 7. Stop task, delete checkpoint, delete worker, delete dataset
    let (status, stop_resp) = send_http_json(
        &router,
        Method::POST,
        &format!("/api/tasks/{}/stop", task_id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stop_resp["success"], true);

    let (status, del_ckpt) = send_http_json(
        &router,
        Method::DELETE,
        "/api/checkpoints/http-ckpt-1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(del_ckpt["success"], true);

    let (status, del_worker) =
        send_http_json(&router, Method::DELETE, "/api/workers/http-worker-2", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(del_worker["success"], true);

    let (status, del_ds) =
        send_http_json(&router, Method::DELETE, "/api/datasets/real-csv-ds", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(del_ds["success"], true);

    let _ = tx.send(());
    Ok(())
}
