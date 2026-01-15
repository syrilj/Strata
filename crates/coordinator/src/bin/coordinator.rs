//! Coordinator binary entry point
//!
//! Starts the gRPC coordinator server for distributed training coordination.

use std::net::SocketAddr;
use std::sync::Arc;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use coordinator::{http_api, CoordinatorServer, CoordinatorService};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "coordinator=info,runtime_core=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse gRPC address from args or use default
    let grpc_addr: SocketAddr = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| "0.0.0.0:50051".parse().unwrap());

    // HTTP API address (gRPC port + 1000)
    let http_addr: SocketAddr = format!("0.0.0.0:{}", grpc_addr.port() + 1000)
        .parse()
        .unwrap();

    tracing::info!("Starting coordinator gRPC on {}", grpc_addr);
    tracing::info!("Starting coordinator HTTP API on {}", http_addr);

    // Create service (Clone-able, so we can share between gRPC and HTTP)
    let service = CoordinatorService::new().await?;

    // Create HTTP API router with cloned service
    let http_service = Arc::new(service.clone());
    let http_router = http_api::create_router(http_service);

    // Spawn HTTP server
    let http_handle = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(http_addr).await.unwrap();
        tracing::info!("HTTP API listening on {}", http_addr);
        axum::serve(listener, http_router).await.unwrap();
    });

    // Create and run gRPC server
    let server = CoordinatorServer::new(service);
    let grpc_handle = tokio::spawn(async move {
        server.run_on(grpc_addr).await.unwrap();
    });

    // Wait for either server to finish
    tokio::select! {
        _ = http_handle => {
            tracing::info!("HTTP server stopped");
        }
        _ = grpc_handle => {
            tracing::info!("gRPC server stopped");
        }
    }

    Ok(())
}
