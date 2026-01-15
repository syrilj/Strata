//! gRPC server implementation with graceful shutdown
//!
//! Provides the Tonic server setup with configurable bind address,
//! graceful shutdown handling, and health check endpoints.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::signal;
use tonic::transport::Server;
use tracing::{error, info};

use crate::proto::coordinator_server::CoordinatorServer as CoordinatorGrpcServer;
use crate::service::CoordinatorService;

/// Service handle type for sharing between gRPC and HTTP
pub type CoordinatorServiceHandle = Arc<CoordinatorService>;

/// Coordinator server configuration
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Address to bind to
    pub addr: SocketAddr,

    /// TCP keepalive interval
    pub tcp_keepalive: Option<Duration>,

    /// Request timeout
    pub request_timeout: Option<Duration>,

    /// Enable gRPC reflection
    pub enable_reflection: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: "0.0.0.0:50051".parse().unwrap(),
            tcp_keepalive: Some(Duration::from_secs(60)),
            request_timeout: Some(Duration::from_secs(300)),
            enable_reflection: true,
        }
    }
}

/// Coordinator gRPC server
pub struct CoordinatorServer {
    config: ServerConfig,
    service: CoordinatorService,
}

impl CoordinatorServer {
    /// Create a new coordinator server
    pub fn new(service: CoordinatorService) -> Self {
        Self {
            config: ServerConfig::default(),
            service,
        }
    }

    /// Create with custom configuration
    pub fn with_config(service: CoordinatorService, config: ServerConfig) -> Self {
        Self { config, service }
    }

    /// Run the server until shutdown signal
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.config.addr;

        info!(address = %addr, "Starting coordinator server");

        // Build the gRPC service
        let grpc_service = CoordinatorGrpcServer::new(self.service)
            .max_decoding_message_size(64 * 1024 * 1024)  // 64MB
            .max_encoding_message_size(64 * 1024 * 1024);

        // Build server
        let mut server_builder = Server::builder();

        if let Some(keepalive) = self.config.tcp_keepalive {
            server_builder = server_builder.tcp_keepalive(Some(keepalive));
        }

        if let Some(timeout) = self.config.request_timeout {
            server_builder = server_builder.timeout(timeout);
        }

        let server = server_builder
            .add_service(grpc_service)
            .serve_with_shutdown(addr, shutdown_signal());

        info!(address = %addr, "Coordinator server listening");

        server.await.map_err(|e| {
            error!(error = %e, "Server error");
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        info!("Coordinator server shutdown complete");
        Ok(())
    }

    /// Run the server on a specific address
    pub async fn run_on(
        self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut server = self;
        server.config.addr = addr;
        server.run().await
    }
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown");
        }
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ServerConfig::default();
        assert_eq!(config.addr, "0.0.0.0:50051".parse().unwrap());
        assert!(config.tcp_keepalive.is_some());
        assert!(config.enable_reflection);
    }
}
