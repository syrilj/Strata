# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Rust integration/e2e tests and the coordinator benchmark no longer require IPv6 (bind `127.0.0.1:0` instead of using `portpicker`)
- Checkpoint benchmark now measures durable local writes (temp file + fsync + rename) and `save_async` call latency separately; the old version reported enqueue time as throughput

### Changed
- Removed performance numbers that were never measured from the README, docs and this changelog; added `BENCHMARKS.md` with measured single-machine results
- Removed `docs/INTERVIEW_GUIDE.md`, `docs/SYSTEM_ARCHITECTURE.tex` and `tests/demos/demo.py`
- Documented that S3 storage is not wired into the coordinator or Python API

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
  - Non-blocking async writes (background Tokio task)
  - Local storage backend
  - Atomic checkpoint writes with rename
  - Background I/O with Tokio

- **Storage Backends**:
  - Local filesystem backend with atomic writes
  - S3 backend with multipart upload support (library only, behind the `s3` feature; not used by the coordinator or Python API)
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
  - Deployment guide (Docker, Kubernetes)

- **Benchmarks** (Criterion):
  - Checkpoint write benchmarks
  - Coordinator RPC benchmarks
  - Data loading and shard assignment benchmarks
  - Consistent hash benchmarks

### Technical Details
- Rust 2021 edition with Tokio async runtime
- gRPC with Tonic for network communication
- Protocol Buffers for service definitions
- PyO3 for Python bindings
- DashMap (sharded locking) for concurrent state management
- AWS SDK for S3 integration

[Unreleased]: https://github.com/syrilj/Strata/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/syrilj/Strata/releases/tag/v0.1.0
