# API Reference

## Python API

### DatasetRegistry

Manages dataset registration and shard assignment for distributed data loading.

```python
from dtruntime import DatasetRegistry

registry = DatasetRegistry(coordinator_url: str)
```

#### Methods

##### `register(dataset_id: str, path: str, format: str, **kwargs) -> None`

Register a dataset with the coordinator.

**Parameters**:
- `dataset_id`: Unique identifier for the dataset
- `path`: Path to dataset (local filesystem or S3)
- `format`: Data format (`"parquet"`, `"csv"`, `"tfrecord"`)
- `total_samples` (optional): Total number of samples
- `shard_size` (optional): Samples per shard (default: 10000)
- `shuffle` (optional): Whether to shuffle data (default: False)
- `seed` (optional): Random seed for shuffling

**Example**:
```python
registry.register(
    "imagenet",
    "/data/imagenet",
    "parquet",
    total_samples=1_281_167,
    shard_size=10_000,
    shuffle=True,
    seed=42
)
```

##### `get_shard(dataset_id: str, worker_rank: int, epoch: int) -> List[str]`

Get shard assignment for a worker.

**Parameters**:
- `dataset_id`: Dataset identifier
- `worker_rank`: Worker rank/ID
- `epoch`: Training epoch number

**Returns**: List of file paths for this worker's shard

**Example**:
```python
files = registry.get_shard("imagenet", worker_rank=0, epoch=0)
# Returns: ["/data/imagenet/shard_0.parquet", "/data/imagenet/shard_5.parquet", ...]
```

---

### CheckpointManager

Handles async checkpoint saving and loading.

```python
from dtruntime import CheckpointManager

manager = CheckpointManager(storage_path: str, backend: str = "local")
```

**Parameters**:
- `storage_path`: Base path for checkpoints
- `backend`: Storage backend (`"local"` or `"s3"`)

#### Methods

##### `async save_async(data: bytes, step: int) -> None`

Asynchronously save a checkpoint.

**Parameters**:
- `data`: Checkpoint data (serialized model state)
- `step`: Training step number

**Example**:
```python
import torch

# Serialize model
state_dict = model.state_dict()
data = torch.save(state_dict)  # Returns bytes

# Save asynchronously (non-blocking)
await manager.save_async(data, step=1000)
```

##### `async load(step: int) -> bytes`

Load a checkpoint from storage.

**Parameters**:
- `step`: Training step number to load

**Returns**: Checkpoint data as bytes

**Example**:
```python
data = await manager.load(step=1000)
state_dict = torch.load(io.BytesIO(data))
model.load_state_dict(state_dict)
```

##### `async list_checkpoints() -> List[int]`

List all available checkpoint steps.

**Returns**: Sorted list of checkpoint step numbers

**Example**:
```python
steps = await manager.list_checkpoints()
# Returns: [100, 200, 300, 400, 500]
latest_step = steps[-1]
```

---

### TrainingOrchestrator

High-level orchestration for distributed training.

```python
from dtruntime import TrainingOrchestrator

orchestrator = TrainingOrchestrator(
    worker_id: str,
    coordinator_url: str,
    world_size: int,
    rank: int
)
```

**Parameters**:
- `worker_id`: Unique worker identifier
- `coordinator_url`: Coordinator server address (e.g., `"http://localhost:50051"`)
- `world_size`: Total number of workers
- `rank`: This worker's rank (0 to world_size-1)

#### Methods

##### `async register_worker(ip: str, port: int, **kwargs) -> str`

Register this worker with the coordinator.

**Parameters**:
- `ip`: Worker IP address
- `port`: Worker port
- `gpu_count` (optional): Number of GPUs
- `memory_bytes` (optional): Available memory

**Returns**: Assigned worker ID from coordinator

**Example**:
```python
assigned_id = await orchestrator.register_worker(
    ip="192.168.1.10",
    port=8080,
    gpu_count=8,
    memory_bytes=512 * 1024**3  # 512 GB
)
```

##### `async wait_barrier(barrier_id: str) -> None`

Synchronization barrier for distributed training.

**Parameters**:
- `barrier_id`: Unique barrier identifier

**Blocks** until all workers in `world_size` reach this barrier.

**Example**:
```python
# All workers must reach this point before any continue
await orchestrator.wait_barrier("start_epoch_5")

# Training continues after all workers arrive
for batch in dataloader:
    train_step(model, batch)
```

##### `async heartbeat() -> None`

Send heartbeat to coordinator (called automatically).

---

## Full Training Example

```python
import asyncio
from dtruntime import DatasetRegistry, CheckpointManager, TrainingOrchestrator
import torch
import torch.distributed as dist

async def main():
    # Initialize
    rank = int(os.environ["RANK"])
    world_size = int(os.environ["WORLD_SIZE"])
    
    orch = TrainingOrchestrator(
        worker_id=f"worker-{rank}",
        coordinator_url="http://coordinator:50051",
        world_size=world_size,
        rank=rank
    )
    
    # Register worker
    await orch.register_worker(
        ip=socket.gethostbyname(socket.gethostname()),
        port=8080
    )
    
    # Setup dataset
    registry = DatasetRegistry("http://coordinator:50051")
    registry.register("imagenet", "/data/imagenet", "parquet")
    
    # Get shard for this worker
    shard_files = registry.get_shard("imagenet", worker_rank=rank, epoch=0)
    
    # Setup checkpointing
    ckpt_mgr = CheckpointManager("/checkpoints", backend="s3")
    
    # Initialize model
    model = YourModel()
    optimizer = torch.optim.Adam(model.parameters())
    
    # Recovery from checkpoint
    try:
        ckpt_data = await ckpt_mgr.load(step=0)  # Load latest
        state = torch.load(io.BytesIO(ckpt_data))
        model.load_state_dict(state["model"])
        optimizer.load_state_dict(state["optimizer"])
        start_step = state["step"]
    except FileNotFoundError:
        start_step = 0
    
    # Training loop
    dataloader = create_dataloader(shard_files)
    
    for step in range(start_step, total_steps):
        # Barrier sync at epoch boundaries
        if step % steps_per_epoch == 0:
            await orch.wait_barrier(f"epoch_{step // steps_per_epoch}")
        
        # Train
        batch = next(dataloader)
        loss = train_step(model, batch, optimizer)
        
        # Checkpoint
        if step % checkpoint_interval == 0:
            state = {
                "model": model.state_dict(),
                "optimizer": optimizer.state_dict(),
                "step": step,
            }
            data = io.BytesIO()
            torch.save(state, data)
            await ckpt_mgr.save_async(data.getvalue(), step=step)
            
            print(f"Step {step}, Loss: {loss:.4f}")
    
    print("Training complete!")

if __name__ == "__main__":
    asyncio.run(main())
```

---

## Rust API

Full Rust API documentation can be generated with:

```bash
cargo doc --no-deps --open
```

### Key Crates

#### `runtime-core`

Core runtime and orchestration logic.

```rust
use runtime_core::{Runtime, Config, Worker};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_file("config.toml")?;
    let runtime = Runtime::new(config).await?;
    
    runtime.start().await?;
    Ok(())
}
```

#### `coordinator`

gRPC coordinator service.

```rust
use coordinator::{CoordinatorService, CoordinatorServiceServer};
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<()> {
    let service = CoordinatorService::new().await?;
    let addr = "0.0.0.0:50051".parse()?;
    
    Server::builder()
        .add_service(CoordinatorServiceServer::new(service))
        .serve(addr)
        .await?;
    
    Ok(())
}
```

#### `checkpoint`

Checkpoint management.

```rust
use checkpoint::{CheckpointManager, CheckpointWriter};
use storage::LocalBackend;
use bytes::Bytes;

let backend = Arc::new(LocalBackend::new("/checkpoints")?);
let writer = CheckpointWriter::new(backend);

// Write checkpoint
let data = Bytes::from(vec![1, 2, 3, 4]);
writer.write_checkpoint("model", 100, data).await?;

// Read checkpoint
let manager = CheckpointManager::new(backend);
let loaded = manager.load_checkpoint("model", 100).await?;
```

#### `data-shard`

Data sharding with consistent hashing.

```rust
use data_shard::{ShardManager, ConsistentHash};

let manager = ShardManager::new("imagenet", 1000);

// Assign shards for epoch 0
let shards = manager.assign_shards(0, "worker-1");
// Returns: [0, 5, 17, 23, ...] (shard IDs)

// Consistent hash ring
let ring = ConsistentHash::new();
ring.add_node("worker-1");
ring.add_node("worker-2");

let node = ring.get_node_for_shard("imagenet", 42)?;
// Returns: "worker-1" or "worker-2"
```

#### `storage`

Storage backend abstraction.

```rust
use storage::{StorageBackend, LocalBackend, S3Backend};
use bytes::Bytes;

// Local storage
let local = LocalBackend::new("/data")?;
local.write("file.bin", Bytes::from("data")).await?;

// S3 storage
let s3 = S3Backend::new("my-bucket", "us-west-2").await?;
s3.write("checkpoints/model.bin", data).await?;
```

---

## gRPC Protocol

### Service Definition

See `proto/coordinator.proto` for complete definition.

**Key RPCs**:

```protobuf
service CoordinatorService {
    // Worker lifecycle
    rpc RegisterWorker(WorkerInfo) returns (WorkerRegistration);
    rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
    
    // Dataset management
    rpc RegisterDataset(DatasetInfo) returns (DatasetRegistration);
    rpc GetDataShard(ShardRequest) returns (ShardAssignment);
    
    // Synchronization
    rpc WaitBarrier(BarrierRequest) returns (BarrierResponse);
    
    // Checkpointing
    rpc NotifyCheckpoint(CheckpointInfo) returns (CheckpointResponse);
    rpc GetLatestCheckpoint(RecoveryRequest) returns (RecoveryInfo);
}
```

### Message Types

**WorkerInfo**:
```protobuf
message WorkerInfo {
    string worker_id = 1;
    string hostname = 2;
    uint32 port = 3;
    uint32 gpu_count = 4;
    uint64 memory_bytes = 5;
    map<string, string> metadata = 6;
}
```

**DatasetInfo**:
```protobuf
message DatasetInfo {
    string dataset_id = 1;
    string path = 2;
    string format = 3;
    uint64 total_samples = 4;
    uint32 shard_size = 5;
    bool shuffle = 6;
    uint64 seed = 7;
    map<string, string> metadata = 8;
}
```

**ShardAssignment**:
```protobuf
message ShardAssignment {
    uint32 shard_id = 1;
    repeated string file_paths = 2;
    uint64 start_sample = 3;
    uint64 end_sample = 4;
}
```

---

## Configuration

### Runtime Configuration (TOML)

```toml
[coordinator]
address = "0.0.0.0:50051"
heartbeat_interval_ms = 1000
heartbeat_timeout_ms = 30000

[storage]
backend = "s3"
bucket = "my-training-checkpoints"
region = "us-west-2"

[checkpoint]
interval_steps = 1000
keep_last_n = 5
compression = "gzip"

[dataset]
default_shard_size = 10000
prefetch_shards = 2
```

### Environment Variables

- `COORDINATOR_URL`: Coordinator address (default: `http://localhost:50051`)
- `WORKER_ID`: Worker identifier
- `RANK`: Worker rank in distributed training
- `WORLD_SIZE`: Total number of workers
- `RUST_LOG`: Logging level (`trace`, `debug`, `info`, `warn`, `error`)

**Example**:
```bash
export COORDINATOR_URL="http://coordinator:50051"
export WORKER_ID="worker-0"
export RANK=0
export WORLD_SIZE=8
export RUST_LOG=info

python train.py
```
