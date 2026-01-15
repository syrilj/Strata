//! Coordinator gRPC server for distributed ML training
//!
//! This crate provides the central coordination server that manages:
//! - **Worker lifecycle**: Registration, heartbeats, failure detection
//! - **Data sharding**: Dataset registration and shard assignment
//! - **Checkpointing**: Distributed checkpoint coordination
//! - **Synchronization**: Barrier-based worker synchronization
//! - **Security**: Rate limiting, input validation, request metrics
//!
//! # Example
//!
//! ```ignore
//! use coordinator::{CoordinatorServer, CoordinatorService};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let service = CoordinatorService::new().await?;
//!     CoordinatorServer::new("0.0.0.0:50051", service).run().await?;
//!     Ok(())
//! }
//! ```

pub mod http_api;
pub mod middleware;
pub mod service;
pub mod server;

// Re-export generated protobuf types
pub mod proto {
    tonic::include_proto!("coordinator");
}

// Re-export main types
pub use service::CoordinatorService;
pub use server::CoordinatorServer;

// Re-export proto service trait for convenience
pub use proto::coordinator_server::CoordinatorServer as CoordinatorServiceServer;
pub use proto::coordinator_client::CoordinatorClient;
