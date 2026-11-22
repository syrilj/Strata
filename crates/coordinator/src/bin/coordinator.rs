//! Coordinator binary entry point
//!
//! Starts the gRPC coordinator server and HTTP REST API server for distributed training coordination.

use std::net::SocketAddr;
use std::sync::Arc;

use runtime_core::config::StorageBackend as CoreStorageBackend;
use runtime_core::RuntimeConfig;
use storage::{LocalStorage, StorageBackend};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use coordinator::server::ServerConfig;
use coordinator::{http_api, CoordinatorServer, CoordinatorService};

fn parse_config_path_arg() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    for i in 1..args.len() {
        if args[i] == "--config" {
            if let Some(path) = args.get(i + 1) {
                return Some(path.clone());
            }
        } else if let Some(path) = args[i].strip_prefix("--config=") {
            return Some(path.to_string());
        }
    }
    std::env::var("STRATA_CONFIG")
        .ok()
        .filter(|s| !s.trim().is_empty())
}

fn parse_positional_grpc_arg() -> Option<SocketAddr> {
    let args: Vec<String> = std::env::args().collect();
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--config" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        if let Ok(addr) = arg.parse::<SocketAddr>() {
            return Some(addr);
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load RuntimeConfig from file (--config / STRATA_CONFIG) or environment
    let mut runtime_config = if let Some(config_path) = parse_config_path_arg() {
        RuntimeConfig::from_toml_file(&config_path)?
    } else {
        RuntimeConfig::from_env()
    };

    // Apply env overrides if config file was loaded
    let env_config = RuntimeConfig::from_env();
    if std::env::var("DEMO_MODE").is_ok() {
        runtime_config.coordinator.demo_mode = env_config.coordinator.demo_mode;
    }
    if std::env::var("GRPC_ADDR").is_ok() {
        runtime_config.coordinator.bind_address = env_config.coordinator.bind_address;
        runtime_config.coordinator.port = env_config.coordinator.port;
    }
    if std::env::var("HTTP_ADDR").is_ok() {
        runtime_config.coordinator.http_address = env_config.coordinator.http_address;
    }

    // Initialize tracing
    let default_filter = format!(
        "coordinator={},runtime_core={}",
        runtime_config.logging.level, runtime_config.logging.level
    );
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default_filter.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Determine gRPC listen address
    let grpc_addr: SocketAddr = parse_positional_grpc_arg().unwrap_or_else(|| {
        format!(
            "{}:{}",
            runtime_config.coordinator.bind_address, runtime_config.coordinator.port
        )
        .parse()
        .unwrap_or_else(|_| "0.0.0.0:50051".parse().unwrap())
    });

    // Determine HTTP API listen address (HTTP_ADDR / config.coordinator.http_address, else grpc_port + 1000)
    let http_addr: SocketAddr = std::env::var("HTTP_ADDR")
        .ok()
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            runtime_config
                .coordinator
                .http_address
                .as_deref()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or_else(|| {
            format!("0.0.0.0:{}", grpc_addr.port() + 1000)
                .parse()
                .unwrap()
        });

    // Ensure local checkpoint directory exists and initialize storage backend
    tokio::fs::create_dir_all(&runtime_config.storage.base_path).await?;
    let storage: Arc<dyn StorageBackend> = match &runtime_config.storage.backend {
        CoreStorageBackend::Local => Arc::new(LocalStorage::new(&runtime_config.storage.base_path)),
        CoreStorageBackend::S3 {
            bucket,
            region,
            endpoint,
        } => {
            tracing::warn!(
                bucket = %bucket,
                region = %region,
                endpoint = ?endpoint,
                "S3 backend configured in RuntimeConfig; coordinator binary built without storage/s3 feature, falling back to LocalStorage at {}",
                runtime_config.storage.base_path
            );
            Arc::new(LocalStorage::new(&runtime_config.storage.base_path))
        }
    };

    tracing::info!("Starting coordinator gRPC on {}", grpc_addr);
    tracing::info!("Starting coordinator HTTP API on {}", http_addr);

    let service = CoordinatorService::from_runtime_config(
        &runtime_config,
        storage,
        grpc_addr.to_string(),
        http_addr.to_string(),
    )
    .await?;

    let shared_service = Arc::new(service.clone());
    shared_service.spawn_background_tasks();

    let http_router = http_api::create_router(Arc::clone(&shared_service));

    // Spawn HTTP server
    let http_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await?;
        tracing::info!("HTTP API listening on {}", http_addr);
        axum::serve(listener, http_router).await?;
        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    });

    // Create and run gRPC server
    let server_config = ServerConfig {
        addr: grpc_addr,
        ..Default::default()
    };
    let server = CoordinatorServer::with_config(service, server_config);
    let grpc_handle = tokio::spawn(async move { server.run().await });

    // Wait for either server to finish
    tokio::select! {
        res = http_handle => {
            match res {
                Ok(Ok(())) => tracing::info!("HTTP server stopped"),
                Ok(Err(e)) => tracing::error!(error = %e, "HTTP server failed"),
                Err(e) => tracing::error!(error = %e, "HTTP task panicked"),
            }
        }
        res = grpc_handle => {
            match res {
                Ok(Ok(())) => tracing::info!("gRPC server stopped"),
                Ok(Err(e)) => tracing::error!(error = %e, "gRPC server failed"),
                Err(e) => tracing::error!(error = %e, "gRPC task panicked"),
            }
        }
    }

    Ok(())
}
