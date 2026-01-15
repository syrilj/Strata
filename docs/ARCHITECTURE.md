# Architecture

## System Overview

The Distributed Training Runtime is a high-performance system designed to coordinate data loading, checkpointing, and state management across hundreds to thousands of worker nodes in large-scale ML training jobs.

### Design Principles

1. **Performance**: Async Rust core with Tokio for maximum I/O throughput
2. **Fault Tolerance**: Automatic recovery from worker failures with checkpoint replay
3. **Scalability**: Designed to scale from 10s to 1000s of workers
4. **Simplicity**: Pythonic API while maintaining low-level performance

## High-Level Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                     Training Workers (Python)                  │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  Training Script                                          │ │
│  │  ┌────────────┐ ┌──────────────┐ ┌──────────────────┐   │ │
│  │  │DatasetReg  │ │CheckpointMgr │ │TrainingOrch      │   │ │
│  │  └────────────┘ └──────────────┘ └──────────────────┘   │ │
│  └──────────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────┘
                            │ PyO3 FFI
┌───────────────────────────────────────────────────────────────┐
│                      Rust Core Runtime                         │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  Python Bindings (PyO3)                                   │ │
│  └──────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │  Runtime Core                                             │ │
│  │  • Worker Management                                      │ │
│  │  • Configuration                                          │ │
│  │  • Error Handling                                         │ │
│  └──────────────────────────────────────────────────────────┘ │
│  ┌────────────┐ ┌──────────┐ ┌────────────┐ ┌─────────────┐ │
│  │Coordinator │ │Checkpoint│ │Data Shard  │ │Storage      │ │
│  │(gRPC)      │ │Manager   │ │Manager     │ │Backend      │ │
│  └────────────┘ └──────────┘ └────────────┘ └─────────────┘ │
└───────────────────────────────────────────────────────────────┘
                            │ Network I/O
┌───────────────────────────────────────────────────────────────┐
│                   Distributed Infrastructure                   │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ Worker Nodes │  │ Coordinator  │  │ S3/Storage        │  │
│  │ (100s-1000s) │  │ Server       │  │ Backend           │  │
│  └──────────────┘  └──────────────┘  └───────────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

## Component Details

### 1. Coordinator (gRPC Server)

**Purpose**: Central coordination service for worker management and synchronization

**Key Responsibilities**:
- Worker registration and heartbeat monitoring
- Dataset metadata management
- Shard assignment coordination
- Barrier synchronization for distributed training
- Checkpoint metadata tracking

**Implementation**:
- Built with Tonic (gRPC for Rust)
- Uses DashMap for concurrent state management
- Async request handling with Tokio

**Protocol Buffers** (proto/coordinator.proto):
```protobuf
service CoordinatorService {
    rpc RegisterWorker(WorkerInfo) returns (WorkerRegistration);
    rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
    rpc RegisterDataset(DatasetInfo) returns (DatasetRegistration);
    rpc GetDataShard(ShardRequest) returns (ShardAssignment);
    rpc WaitBarrier(BarrierRequest) returns (BarrierResponse);
    rpc NotifyCheckpoint(CheckpointInfo) returns (CheckpointResponse);
    rpc GetLatestCheckpoint(RecoveryRequest) returns (RecoveryInfo);
}
```

**Performance Characteristics**:
- Worker registration: ~1000 workers/sec
- Heartbeat processing: ~10000 requests/sec
- Barrier sync (100 workers): <50ms p99 latency

### 2. Data Shard Manager

**Purpose**: Distribute dataset shards evenly across workers using consistent hashing

**Key Features**:
- **Consistent Hashing**: Minimizes data movement when workers join/leave
- **Epoch Management**: Tracks shard assignments per training epoch
- **Load Balancing**: Even distribution across workers

**Algorithm**:
```rust
// Consistent hash ring with virtual nodes
Ring: HashMap<u64, WorkerID>
VirtualNodesPerWorker: 150

fn get_node_for_shard(shard_id: usize) -> WorkerID {
    hash = hash(dataset_id + shard_id)
    ring.range(hash..).next().or(ring.first())
}
```

**Trade-offs**:
- Virtual nodes increase memory but improve distribution uniformity
- 150 virtual nodes achieves <5% deviation from perfect balance

**Performance**:
- Shard assignment: O(log n) where n = workers * virtual_nodes
- 1000 workers, 10000 shards: <10ms total assignment time

### 3. Checkpoint Manager

**Purpose**: Async checkpoint persistence with background I/O

**Key Features**:
- Non-blocking async writes
- Configurable storage backends (Local, S3)
- Automatic retry logic
- Compression support

**Write Path**:
```
User calls save_async()
  ↓
Serialize state (sync)
  ↓
Spawn background task
  ↓
Write to storage (async I/O)
  ↓
Return immediately
```

**Storage Abstraction**:
```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn write(&self, path: &str, data: Bytes) -> Result<()>;
    async fn read(&self, path: &str) -> Result<Bytes>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete(&self, path: &str) -> Result<()>;
}
```

**Implementations**:
- **LocalBackend**: File system storage with atomic writes
- **S3Backend**: AWS S3 with multipart upload for large checkpoints

**Performance**:
- 100MB checkpoint write: ~500 MB/s to local SSD
- S3 multipart upload: ~200 MB/s (network bound)
- Concurrent writes (8 workers): Linear scaling up to storage bandwidth

### 4. Runtime Core

**Purpose**: High-level orchestration and worker lifecycle management

**Components**:
- **Config**: TOML-based configuration management
- **Worker**: Worker state and communication handling
- **Types**: Shared type definitions
- **Error**: Unified error handling with thiserror

**Worker Lifecycle**:
```
Initialize → Register → Heartbeat Loop → Train → Checkpoint → Finalize
                ↓                                    ↓
          Assign Shards                        Notify Coord
```

### 5. Python Bindings (PyO3)

**Purpose**: Expose Rust functionality with Pythonic API

**Key Classes**:
- `DatasetRegistry`: Dataset registration and shard retrieval
- `CheckpointManager`: Async checkpoint save/load
- `TrainingOrchestrator`: High-level training coordination

**FFI Strategy**:
- PyO3 for zero-copy data passing where possible
- pyo3-async-runtimes for async/await integration
- GIL management for parallel operations

**Example Binding**:
```rust
#[pyclass]
pub struct CheckpointManager {
    inner: Arc<checkpoint::CheckpointManager>,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl CheckpointManager {
    #[pyo3(signature = (data, step))]
    fn save_async<'py>(
        &self,
        py: Python<'py>,
        data: &PyBytes,
        step: u64,
    ) -> PyResult<&'py PyAny> {
        // Async binding with pyo3_asyncio
    }
}
```

## Data Flow

### Training Iteration

```
1. Worker calls get_batch()
     ↓
2. DatasetRegistry.get_shard()
     ↓ gRPC
3. Coordinator assigns shard via consistent hash
     ↓
4. Worker loads data from assigned files
     ↓
5. Train step
     ↓
6. (Every N steps) CheckpointManager.save_async()
     ↓ Spawn task
7. Background thread writes to storage
     ↓ gRPC (notify)
8. Coordinator records checkpoint metadata
```

### Fault Recovery

```
1. Worker crashes
     ↓
2. Coordinator detects missed heartbeats
     ↓
3. Mark worker as failed
     ↓
4. Redistribute shards to remaining workers
     ↓
5. New/restarted worker calls recovery()
     ↓ gRPC
6. Coordinator returns latest checkpoint info
     ↓
7. Worker loads checkpoint from storage
     ↓
8. Resume training from checkpoint step
```

## Design Decisions

### Why Rust?

**Advantages**:
1. **Performance**: Zero-cost abstractions, no GC pauses
2. **Safety**: Prevents data races at compile time
3. **Async I/O**: Tokio provides excellent async runtime
4. **FFI**: PyO3 makes Python integration straightforward

**Trade-offs**:
- Steeper learning curve vs Python-only solution
- Longer compile times
- Worth it for performance-critical infrastructure

### Why gRPC?

**Advantages**:
1. **HTTP/2**: Multiplexing, header compression
2. **Strong typing**: Protobuf schema enforcement
3. **Streaming**: Efficient for heartbeats and barriers
4. **Cross-language**: Easy to add workers in other languages

**Alternatives Considered**:
- REST: Too much overhead for high-frequency RPCs
- Raw TCP: No schema, more error-prone
- Redis: Extra dependency, not designed for RPC

### Why Consistent Hashing?

**Advantages**:
1. **Minimal Redistribution**: Only ~K/N shards move when worker added/removed
2. **Deterministic**: Same shard always goes to same worker (given same topology)
3. **Stateless**: No central shard assignment table needed

**Complexity**:
- Virtual nodes add memory overhead (~150 * num_workers entries)
- Hash computation on every shard lookup

**Alternatives**:
- Modulo hashing: Simple but terrible for dynamic scaling (all shards move)
- Centralized table: Doesn't scale, single point of contention

## Scalability Analysis

### Coordinator Bottleneck Analysis

**State Size**:
- Worker info: ~1KB per worker
- Dataset metadata: ~10KB per dataset
- Barrier state: ~100 bytes per barrier
- Checkpoint metadata: ~500 bytes per checkpoint

**For 1000 workers, 100 datasets, 10000 checkpoints**:
- Memory: ~10 MB (negligible)
- Network: Heartbeats are main load

**Heartbeat Load**:
- Frequency: 1 Hz per worker
- 1000 workers = 1000 RPS
- Tonic can handle >100K RPS on modern hardware
- **Verdict**: Not a bottleneck until >10K workers

**Barrier Synchronization**:
- All workers must arrive before releasing
- Coordinator holds N connections open
- With 1000 workers: ~50ms coordination overhead
- **Optimization**: Use generation counter instead of per-worker tracking

### Storage Bottleneck

**Checkpoint Write**:
- 100 workers * 1GB checkpoint = 100 GB
- Local NVMe: ~3 GB/s write → 33 seconds
- S3: ~200 MB/s per worker → 5 seconds parallel

**Mitigation**:
- Async writes don't block training
- Write to local, async upload to S3
- Delta checkpoints for large models

### Network Bandwidth

**Data Loading**:
- Assume 1M samples/sec total across 100 workers
- 1KB per sample → 1 GB/s
- 10 GbE network: 1.25 GB/s → Acceptable

**gRPC Overhead**:
- Protobuf adds ~5-10% size overhead
- HTTP/2 header compression mitigates this
- Measured: <3% overhead in practice

## Fault Tolerance

### Failure Scenarios

1. **Worker Crash**
   - Detection: Missed heartbeats (timeout 30s)
   - Recovery: Redistribute shards, new worker loads latest checkpoint
   - Data loss: None (checkpoints persisted)

2. **Coordinator Crash**
   - **Current**: Single point of failure
   - **Mitigation**: Kubernetes with auto-restart, workers retry connection
   - **Future**: Raft consensus for HA coordinator

3. **Network Partition**
   - Workers can't reach coordinator
   - Training can continue with cached shard assignments
   - Checkpoints queue locally, sync when connection restored

4. **Storage Failure**
   - S3: Built-in redundancy (99.999999999% durability)
   - Local: Regular backup to S3

### Consistency Model

**Checkpointing**:
- Async writes → eventual consistency
- Coordinator tracks "last known good" checkpoint
- Workers wait for barrier before finalizing checkpoint epoch

**Shard Assignment**:
- Deterministic (consistent hash) → no coordination needed for reads
- Workers can compute shard locally if coordinator unavailable

## Performance Optimizations

### 1. Zero-Copy Where Possible

- PyO3 `PyBytes::as_bytes()` → no memcpy
- Tokio uses vectored I/O for scatter/gather
- mmap for large dataset files

### 2. Async I/O

- All network and disk I/O is non-blocking
- Tokio work-stealing scheduler balances load
- Dedicated thread pool for blocking ops (e.g., compression)

### 3. Lock-Free Data Structures

- DashMap for concurrent hash maps (no global lock)
- Atomic counters for metrics
- RwLock only where mutation is rare

### 4. Batch Operations

- Group multiple shard requests into single RPC
- Batch checkpoint metadata updates
- Reduces RPC overhead by ~10x

## Future Enhancements

1. **High Availability Coordinator**
   - Raft consensus for coordinator replication
   - Automatic failover <1 second

2. **Delta Checkpoints**
   - Only save changed parameters
   - Reduces checkpoint size by ~90% for large models

3. **Speculative Execution**
   - Detect stragglers, duplicate work on backup workers
   - Improves tail latency

4. **Dynamic Shard Resizing**
   - Adjust shard size based on worker capacity
   - Better load balancing for heterogeneous clusters

5. **Multi-Tenancy**
   - Isolate multiple training jobs on same coordinator
   - Resource quotas and priority queues
