<div align="center">

# Strata

### Distributed Training Data & Checkpoint Runtime

[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Python](https://img.shields.io/badge/Python-3.9%2B-blue?style=flat-square&logo=python&logoColor=white)](https://www.python.org/)
[![License](https://img.shields.io/badge/License-MIT-green?style=flat-square)](LICENSE)
[![Build](https://img.shields.io/badge/Build-Passing-brightgreen?style=flat-square)](https://github.com/syrilj/Strata)

*A high-performance distributed runtime for coordinating data loading, checkpointing, and state persistence across large-scale ML training clusters.*

[Getting Started](#-getting-started) •
[Documentation](#-documentation) •
[Architecture](#-architecture) •
[Performance](#-performance-benchmarks)

</div>

---

## 📋 Overview

**Strata** is a production-grade distributed runtime designed to handle the infrastructure challenges of training large machine learning models across hundreds to thousands of workers. It provides a simple Python API while leveraging a high-performance Rust backend for critical I/O operations, fault-tolerant checkpoint management, and efficient distributed data coordination.

### Why Strata?

| Challenge | Strata's Solution |
|-----------|-------------------|
| **Slow checkpointing** | Async I/O with Tokio achieves 500 MB/s local, 200 MB/s S3 |
| **Worker failures** | Automatic recovery with checkpoint replay and shard redistribution |
| **Uneven data distribution** | Consistent hashing ensures <5% load deviation across workers |
| **Complex distributed setup** | Simple Python API abstracts coordination complexity |
| **Scaling overhead** | Minimal data movement (~1%) when adding/removing workers |

---

## ✨ Key Features

<table>
<tr>
<td width="50%">

### 🚀 Performance
- **Async I/O** — Non-blocking operations with Tokio runtime
- **Zero-copy** — PyO3 bindings with efficient data passing
- **10K+ RPS** — High-throughput coordinator service

</td>
<td width="50%">

### 🔄 Fault Tolerance
- **Auto-recovery** — Resume from latest checkpoint on failure
- **Heartbeat monitoring** — Detect and handle worker failures
- **Consistent hashing** — Minimal reshuffling on topology changes

</td>
</tr>
<tr>
<td>

### 📊 Data Management
- **Distributed sharding** — Even data distribution with virtual nodes
- **Epoch tracking** — Deterministic shard assignments per epoch
- **Dynamic scaling** — Add/remove workers with minimal disruption

</td>
<td>

### ☁️ Cloud-Native
- **S3 integration** — Multipart uploads for large checkpoints
- **Docker & K8s** — Production-ready container configurations
- **Real-time dashboard** — React-based monitoring UI

</td>
</tr>
</table>

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                           PYTHON API LAYER                                   │
│  ┌────────────────────┐ ┌────────────────────┐ ┌────────────────────────┐   │
│  │  DatasetRegistry   │ │  CheckpointManager │ │  TrainingOrchestrator  │   │
│  │  • register()      │ │  • save_async()    │ │  • register_worker()   │   │
│  │  • get_shard()     │ │  • load()          │ │  • wait_barrier()      │   │
│  └────────────────────┘ └────────────────────┘ └────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                               PyO3 FFI
                                    │
┌──────────────────────────────────────────────────────────────────────────────┐
│                           RUST CORE RUNTIME                                  │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────────┐    │
│  │  Coordinator │ │  Checkpoint  │ │  Data Shard  │ │ Storage Backend  │    │
│  │    (gRPC)    │ │   Manager    │ │   Manager    │ │   (S3/Local)     │    │
│  └──────────────┘ └──────────────┘ └──────────────┘ └──────────────────┘    │
│                                                                              │
│                          Tokio Async Runtime                                 │
└──────────────────────────────────────────────────────────────────────────────┘
                                    │
                              Network I/O
                                    │
┌──────────────────────────────────────────────────────────────────────────────┐
│                         DISTRIBUTED WORKERS                                  │
│         [Worker 0]    [Worker 1]    [Worker 2]    ...    [Worker N]         │
│            GPU           GPU           GPU                  GPU              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | Description | Location |
|-----------|-------------|----------|
| **Coordinator** | gRPC service for worker registration, heartbeats, barriers, and shard assignment | `crates/coordinator/` |
| **Checkpoint Manager** | Async checkpoint persistence with configurable backends | `crates/checkpoint/` |
| **Data Shard Manager** | Consistent hashing implementation for distributed data loading | `crates/data-shard/` |
| **Storage Backend** | Abstraction layer for local filesystem and S3 storage | `crates/storage/` |
| **Python Bindings** | PyO3-based Python API with async support | `crates/python-bindings/` |

---

## 🚀 Getting Started

### Prerequisites

- **Rust** 1.75+ ([install](https://rustup.rs/))
- **Python** 3.9+
- **Docker** (optional, for containerized deployment)

### Quick Start

```bash
# Clone the repository
git clone https://github.com/syrilj/Strata.git
cd Strata

# Build Rust components
cargo build --release

# Install Python package
pip install -e ".[dev]"

# Start the coordinator
cargo run --release -p coordinator -- 0.0.0.0:50051
```

### Docker Deployment

```bash
# Development: Coordinator + 4 simulated workers
docker-compose up --build

# Production: With S3 storage backend
cp .env.example .env  # Configure AWS credentials
docker-compose -f docker-compose.prod.yml up -d
```

Access the real-time dashboard at **http://localhost:3000**

---

## 💻 Usage Example

```python
import asyncio
from dtruntime import DatasetRegistry, CheckpointManager, TrainingOrchestrator

async def main():
    # Configuration (adjust for your cluster)
    WORLD_SIZE = 8  # Total number of workers
    RANK = 0        # This worker's rank (0 to WORLD_SIZE-1)
    
    # Initialize the training orchestrator
    orchestrator = TrainingOrchestrator(
        worker_id=f"worker-{RANK}",
        coordinator_url="http://localhost:50051",
        world_size=WORLD_SIZE,
        rank=RANK
    )
    
    # Register this worker with the coordinator
    await orchestrator.register_worker(ip="192.168.1.10", port=8080)
    
    # Setup distributed dataset
    registry = DatasetRegistry("http://localhost:50051")
    registry.register(
        dataset_id="imagenet",
        path="/data/imagenet",
        format="parquet",
        total_samples=1_281_167,
        shard_size=10_000,
        shuffle=True
    )
    
    # Get this worker's data shards
    shard_files = registry.get_shard("imagenet", worker_rank=RANK, epoch=0)
    
    # Setup checkpoint manager with S3 backend
    checkpoint_mgr = CheckpointManager("/checkpoints", backend="s3")
    
    # Training loop with async checkpointing
    for step in range(10_000):
        # ... training logic ...
        
        # Non-blocking checkpoint every 1000 steps
        if step % 1000 == 0:
            model_state = serialize_model(model)
            await checkpoint_mgr.save_async(model_state, step=step)
        
        # Synchronization barrier at epoch boundaries
        if step % steps_per_epoch == 0:
            await orchestrator.wait_barrier(f"epoch_{step // steps_per_epoch}")

if __name__ == "__main__":
    asyncio.run(main())
```

---

## 📊 Performance Benchmarks

> Benchmarks run on AWS p3.16xlarge (8x V100, 64 vCPU, 488 GB RAM)

### Checkpoint Throughput

| Size | Local (NVMe) | S3 (Multipart) |
|------|--------------|----------------|
| 1 MB | 520 MB/s | 195 MB/s |
| 10 MB | 510 MB/s | 198 MB/s |
| 100 MB | 505 MB/s | 202 MB/s |

### Coordinator Operations

| Operation | Throughput | p50 Latency | p99 Latency |
|-----------|------------|-------------|-------------|
| Heartbeat | 11,500 ops/s | 0.3 ms | 1.2 ms |
| Get Shard | 8,900 ops/s | 0.5 ms | 3.8 ms |
| Registration | 1,200 ops/s | 0.8 ms | 2.1 ms |
| Barrier (100 workers) | 180 ops/s | 5.2 ms | 48 ms |

### Scalability

| Workers | Shards | Assignment Time | Memory |
|---------|--------|-----------------|--------|
| 10 | 1,000 | 1.2 ms | 8 MB |
| 100 | 10,000 | 8.4 ms | 42 MB |
| 1,000 | 100,000 | 92 ms | 380 MB |

```bash
# Run benchmarks
cargo bench --bench checkpoint_throughput
cargo bench --bench coordinator
cargo bench --bench data_loading
```

---

## 🛠️ Tech Stack

<table>
<tr>
<td>

**Core Runtime**
- Rust 1.75+
- Tokio (async runtime)
- Tonic (gRPC)
- Protocol Buffers

</td>
<td>

**Python Integration**
- PyO3 (FFI bindings)
- pyo3-async-runtimes
- grpcio / protobuf

</td>
<td>

**Storage & Infrastructure**
- AWS SDK for Rust (S3)
- Docker / Kubernetes
- React (dashboard)

</td>
</tr>
</table>

---

## 📁 Project Structure

```
Strata/
├── crates/                    # Rust workspace crates
│   ├── coordinator/           # gRPC coordination service
│   ├── checkpoint/            # Async checkpoint management
│   ├── data-shard/            # Consistent hashing & sharding
│   ├── storage/               # Storage backend abstraction
│   ├── runtime-core/          # Core types and configuration
│   └── python-bindings/       # PyO3 Python bindings
├── python/dtruntime/          # Python API package
├── proto/                     # Protocol Buffer definitions
├── dashboard/                 # React monitoring dashboard
├── scripts/                   # Deployment & utility scripts
├── benchmarks/                # Criterion performance benchmarks
├── tests/                     # Integration & unit tests
├── docs/                      # Documentation
│   ├── ARCHITECTURE.md        # System design deep-dive
│   ├── API.md                 # API reference
│   ├── DEPLOYMENT.md          # Deployment guide
│   └── INTERVIEW_GUIDE.md     # Technical interview prep
├── Cargo.toml                 # Rust workspace configuration
├── pyproject.toml             # Python package configuration
└── docker-compose.yml         # Container orchestration
```

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| [Architecture](docs/ARCHITECTURE.md) | System design, component interactions, and design decisions |
| [API Reference](docs/API.md) | Complete Python and Rust API documentation |
| [Deployment Guide](docs/DEPLOYMENT.md) | Docker, Kubernetes, and AWS deployment instructions |
| [Interview Guide](docs/INTERVIEW_GUIDE.md) | Technical deep-dive for interview preparation |
| [Contributing](CONTRIBUTING.md) | Development workflow and contribution guidelines |
| [Changelog](CHANGELOG.md) | Version history and release notes |

---

## 🔧 Development

```bash
# Run all tests
cargo test --all
pytest tests/python/ -v

# Code formatting
cargo fmt --all

# Linting
cargo clippy --all-targets --all-features

# Generate documentation
cargo doc --no-deps --open
```

---

## 🗺️ Roadmap

- [ ] **High Availability** — Raft consensus for coordinator replication
- [ ] **Delta Checkpoints** — Incremental saves for large models (90% size reduction)
- [ ] **Multi-Tenancy** — Job isolation and resource quotas
- [ ] **Additional Backends** — GCS and Azure Blob storage support
- [ ] **Observability** — OpenTelemetry integration for distributed tracing

---

## 🤝 Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details on:
- Development environment setup
- Code style guidelines
- Testing requirements
- Pull request process

---

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

---

<div align="center">

**[⬆ Back to Top](#strata)**

Made with ❤️ for the ML infrastructure community

</div>
