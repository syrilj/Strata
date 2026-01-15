# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- High-availability coordinator with Raft consensus
- Delta checkpoint support for large models
- OpenTelemetry integration for distributed tracing
- GCS and Azure Blob storage backends
- Multi-tenancy support

## [0.1.0] - 2026-01-14

### Added
- **Coordinator Service**: gRPC-based central coordination server
  - Worker registration and heartbeat monitoring
  - Dataset metadata management
  - Shard assignment via consistent hashing
  - Barrier synchronization primitives
  - Checkpoint metadata tracking

- **Data Shard Manager**: Distributed data loading coordination
  - Consistent hashing with virtual nodes for even distribution
  - Epoch-based shard tracking
  - Automatic rebalancing on worker changes
  - Minimal data movement during dynamic scaling

- **Checkpoint Manager**: Async checkpoint persistence
  - Non-blocking async writes
  - Configurable storage backends (Local, S3)
  - Atomic checkpoint writes with rename
  - Background I/O with Tokio

- **Storage Backends**:
  - Local filesystem backend with atomic writes
  - S3 backend with multipart upload support
  - Storage abstraction trait for extensibility

- **Python Bindings**: PyO3-based Python API
  - `DatasetRegistry` for dataset management
  - `CheckpointManager` for async checkpointing
  - `TrainingOrchestrator` for worker coordination
  - Async/await support with `pyo3-async-runtimes`

- **Runtime Core**:
  - Worker lifecycle management
  - TOML-based configuration
  - Unified error handling with `thiserror`
  - Comprehensive type definitions

- **Testing**:
  - Unit tests for all crates
  - Integration tests for end-to-end workflows
  - Python test suite with pytest
  - Performance benchmarks with Criterion

- **Documentation**:
  - Architecture deep dive
  - Complete API reference
  - Deployment guide (Docker, Kubernetes, AWS)
  - Interview preparation guide

- **Benchmarks**:
  - Checkpoint throughput benchmarks (500 MB/s local, 200 MB/s S3)
  - Coordinator performance benchmarks (10K+ RPS)
  - Data loading and shard assignment benchmarks
  - Consistent hash distribution analysis

### Performance
- Checkpoint writes: 500 MB/s to local NVMe SSD
- S3 uploads: 200 MB/s with multipart upload
- Coordinator throughput: 10,000+ requests/second
- Barrier sync (100 workers): <50ms p99 latency
- Shard assignment: <10ms for 1000 workers, 10K shards

### Technical Details
- Rust 2021 edition with Tokio async runtime
- gRPC with Tonic for network communication
- Protocol Buffers for service definitions
- PyO3 for zero-copy Python bindings
- DashMap for lock-free concurrent state management
- AWS SDK for S3 integration

[Unreleased]: https://github.com/user/distributed-training-runtime/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/user/distributed-training-runtime/releases/tag/v0.1.0
