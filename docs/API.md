# API Reference

## Python API

### `DatasetRegistry`

Manages dataset registration and shard assignment for distributed data loading (supports local mode with `coordinator_url=None` or coordinator-backed gRPC mode).

```python
from dtruntime import DatasetRegistry

registry = DatasetRegistry(coordinator_url: str | None = None)
```

#### Methods

- `register_worker(worker_id: str) -> None`
- `remove_worker(worker_id: str) -> None`
- `register_dataset(dataset_id: str, total_samples: int, shard_size: int, shuffle: bool = True, seed: int = 42, path: str = "", format: str = "parquet") -> None`
- `register(dataset_id: str, path: str, format: str = "parquet", total_samples: int = 10000, shard_size: int = 1000, shuffle: bool = True, seed: int = 42) -> None`
- `get_shards(dataset_id: str, worker_id: str, epoch: int = 0) -> list[ShardInfo]`
- `get_shard(dataset_id: str, worker_id: str | None = None, worker_rank: int | None = None, epoch: int = 0) -> ShardInfo`
- `advance_epoch(dataset_id: str) -> int`
- `current_epoch(dataset_id: str) -> int`
- `active_workers() -> list[str]`
- `datasets() -> list[str]`
- Properties: `coordinator_url`, `worker_count`, `dataset_count`

**Example**:
```python
registry = DatasetRegistry("http://localhost:50051")
registry.register(
    "imagenet",
    "/data/imagenet",
    "parquet",
    total_samples=1_281_167,
    shard_size=10_000,
    shuffle=True,
    seed=42,
)
shard = registry.get_shard("imagenet", worker_rank=0, epoch=0)
# ShardInfo(dataset_id='imagenet', shard_id=..., start_index=..., end_index=..., file_paths=[...])
```

---

### `CheckpointManager`

Handles synchronous and async checkpoint saving, loading, listing, and deletion.

```python
from dtruntime import CheckpointManager

manager = CheckpointManager(base_path: str, keep_count: int = 5, compression: bool = True)
```

#### Methods

- `save(data: bytes, step: int, epoch: int = 0, metadata: dict[str, str] | None = None) -> str`
- `async save_async(data: bytes, step: int, epoch: int = 0, metadata: dict[str, str] | None = None) -> str`
- `load(checkpoint_id_or_step: str | int) -> bytes`
- `async load_async(checkpoint_id_or_step: str | int) -> bytes`
- `delete(checkpoint_id: str) -> bool`
- `list_checkpoints() -> list[int]`
- `latest() -> CheckpointInfo | None`
- `get_by_step(step: int) -> CheckpointInfo | None`
- `all_checkpoints() -> list[CheckpointInfo]`
- `wait_pending() -> None`

**Example**:
```python
import io
import torch
from dtruntime import CheckpointManager

manager = CheckpointManager("/tmp/checkpoints", keep_count=5, compression=True)

buf = io.BytesIO()
torch.save(model.state_dict(), buf)
ckpt_id = await manager.save_async(buf.getvalue(), step=1000, epoch=2)

raw = await manager.load_async(1000)  # load by step or checkpoint_id
state_dict = torch.load(io.BytesIO(raw))
steps = manager.list_checkpoints()
```

---

### `TrainingOrchestrator`

High-level gRPC orchestration client for distributed training workers.

```python
from dtruntime import TrainingOrchestrator

orchestrator = TrainingOrchestrator(
    coordinator_url: str,
    worker_id: str | None = None,
    world_size: int | None = None,
    rank: int | None = None,
)
```

**Parameters**:
- `coordinator_url`: Coordinator server address (e.g., `"http://localhost:50051"` or `"localhost:50051"`)
- `worker_id` (optional): Worker identifier (may also be supplied to `register_worker`)
- `world_size` (optional): Expected world size hint
- `rank` (optional): Worker rank hint

#### Methods

- `connect() -> None`
- `register_worker(worker_id: str | None = None, hostname: str = "127.0.0.1", port: int = 50052, gpu_count: int = 0, memory_bytes: int = 0, ip: str | None = None, metadata: dict[str, str] | None = None) -> WorkerConfig`
- `async register_worker_async(worker_id: str | None = None, hostname: str = "127.0.0.1", port: int = 50052, gpu_count: int = 0, memory_bytes: int = 0, ip: str | None = None, metadata: dict[str, str] | None = None) -> WorkerConfig`
- `heartbeat(current_step: int = 0, current_epoch: int = 0, state: str = "training", current_task: str = "", cpu_percent: float = 0.0, memory_used_bytes: int = 0) -> bool`
- `heartbeat_with_commands(current_step: int = 0, current_epoch: int = 0, state: str = "training", current_task: str = "", cpu_percent: float = 0.0, memory_used_bytes: int = 0) -> tuple[bool, list[str]]`
- `async heartbeat_async(current_step: int = 0, current_epoch: int = 0, state: str = "training", current_task: str = "", cpu_percent: float = 0.0, memory_used_bytes: int = 0) -> bool`
- `register_dataset(dataset_id: str, path: str, total_samples: int, shard_size: int, format: str = "parquet", shuffle: bool = True, seed: int = 42) -> bool`
- `get_shard(dataset_id: str, epoch: int = 0) -> CoordinatorShardInfo`
- `notify_checkpoint(checkpoint_id: str, step: int, epoch: int, storage_path: str, size_bytes: int, checkpoint_type: str = "full", metadata: dict[str, str] | None = None) -> bool`
- `get_latest_checkpoint(job_id: str = "") -> CoordinatorCheckpointInfo | None`
- `barrier(barrier_id: str, step: int = 0) -> BarrierResult`
- `wait_barrier(barrier_id: str, step: int = 0) -> BarrierResult`
- `async wait_barrier_async(barrier_id: str, step: int = 0) -> BarrierResult`
- `deregister() -> None`
- Properties: `coordinator_url`, `worker_id`, `world_size`, `rank`, `last_commands`

**Example**:
```python
import asyncio
import socket
from dtruntime import TrainingOrchestrator

async def main():
    orch = TrainingOrchestrator(
        coordinator_url="http://localhost:50051",
        worker_id="worker-0",
        world_size=2,
        rank=0,
    )
    config = await orch.register_worker_async(
        hostname=socket.gethostname(),
        port=50052,
        gpu_count=4,
        memory_bytes=64 * 1024**3,
    )
    print(f"Registered rank {config.rank}/{config.world_size}")

    await orch.wait_barrier_async("epoch-0-start", step=0)
    await orch.heartbeat_async(current_step=1, current_epoch=0, state="training", current_task="train")
    orch.deregister()

asyncio.run(main())
```

---

## HTTP REST API

The coordinator exposes an HTTP REST API (default port `51051`, configurable via `HTTP_ADDR`).

### Health & Status

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Liveness check (`{"status": "ok"}`) |
| `GET` | `/api/status` | Coordinator status (`connected`, `address`, `http_address`, `uptime`, `version`, `storage_backend`, `demo_mode`) |
| `GET` | `/api/dashboard` | Consolidated snapshot (`coordinator`, `workers`, `datasets`, `checkpoints`, `barriers`, `metrics`, `tasks`, `logs`) |

### Workers

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/workers` | List all registered workers and their resource usage |
| `GET` | `/api/workers/:id` | Get a single worker by ID (`404` if not found) |
| `DELETE` | `/api/workers/:id` | Deregister a worker by ID |

### Datasets

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/datasets` | List all registered datasets |
| `POST` | `/api/datasets` | Register a dataset (`name`, `path`, `format`, `shard_size`, optional `id`, `total_samples`, `shuffle`, `seed`) |
| `DELETE` | `/api/datasets/:id` | Remove a registered dataset |
| `GET` | `/api/datasets/:id/shards` | List shard assignments for a dataset (`?epoch=0`) |
| `GET` | `/api/datasets/:id/preview` | Preview real sample rows from local files (`csv`, `jsonl`, `ndjson`, `json`, `txt`) or real shard metadata |

### Checkpoints, Barriers, Metrics, Tasks & Logs

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/checkpoints` | List recorded checkpoints |
| `GET` | `/api/checkpoints/:id` | Get a single checkpoint (`404` if not found) |
| `DELETE` | `/api/checkpoints/:id` | Delete a checkpoint |
| `GET` | `/api/barriers` | List active and recently completed barriers |
| `GET` | `/api/metrics` | Current metrics (`checkpoint_throughput`, `coordinator_rps`, `active_workers`, `total_workers`, `barrier_latency_p99`, `shard_assignment_time`) |
| `GET` | `/api/metrics/history` | Rolling throughput/RPS history points |
| `GET` | `/api/tasks` | List training tasks |
| `POST` | `/api/tasks` | Create a training task (`name`, `type`, `dataset_id`, `worker_count`, `config`) |
| `POST` | `/api/tasks/:task_id/stop` | Stop a task and enqueue `STOP_TASK` commands to assigned workers |
| `GET` | `/api/tasks/:task_id/logs` | Get logs for a specific task |
| `GET` | `/api/logs` | Query bounded event logs (`?limit=100&level=...&source=...&task_id=...`) |

---

## Rust API

Full Rust API documentation can be generated with:

```bash
cargo doc --no-deps --open
```

### Key Crates

#### `runtime-core`

Core types, configuration (`RuntimeConfig`), worker registry (`WorkerRegistry`), and bounded structured event logging (`EventLog`).

#### `coordinator`

gRPC coordinator service (`CoordinatorService`, `CoordinatorServer`) and HTTP REST router (`http_api::create_router`).

```rust
use coordinator::{CoordinatorServer, CoordinatorService};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = CoordinatorService::new().await?;
    CoordinatorServer::new("0.0.0.0:50051", service).run().await?;
    Ok(())
}
```

#### `checkpoint`

Async checkpoint writer and manager (`CheckpointManager`, `CheckpointManagerConfig`).

#### `data-shard`

Consistent hashing and dataset shard assignment (`ShardManager`, `ConsistentHashRing`).

#### `storage`

Storage backend abstraction (`StorageBackend`, `LocalStorage`, `S3Storage`).

---

## gRPC Protocol

See `proto/coordinator.proto` for the full protobuf definition.

**Service RPCs (`coordinator.Coordinator`)**:
- `RegisterWorker(WorkerInfo) returns (WorkerConfig)`
- `Heartbeat(HeartbeatRequest) returns (HeartbeatResponse)`
- `DeregisterWorker(WorkerInfo) returns (WorkerConfig)`
- `RegisterDataset(DatasetInfo) returns (DatasetAck)`
- `GetDataShard(ShardRequest) returns (ShardAssignment)`
- `NotifyCheckpoint(CheckpointInfo) returns (CheckpointAck)`
- `GetLatestCheckpoint(RecoveryRequest) returns (RecoveryResponse)`
- `WaitBarrier(BarrierRequest) returns (BarrierResponse)`
- `StreamHeartbeats(stream HeartbeatRequest) returns (stream HeartbeatResponse)`

---

## Configuration

### Environment Variables

- `GRPC_ADDR`: Coordinator gRPC listen address (default: `0.0.0.0:50051`)
- `HTTP_ADDR`: Coordinator HTTP REST API listen address (default: `0.0.0.0:51051`)
- `DEMO_MODE`: Set `"true"` to serve seeded demo data; defaults to `"false"` (live mode)
- `STORAGE_BACKEND`: `"local"` or `"s3"`
- `CHECKPOINT_DIR`: Local storage/checkpoint directory (default: `./data`)
- `COORDINATOR_API_URL`: Upstream coordinator HTTP API URL used by `dashboard/serve.py` (default: `http://127.0.0.1:51051`)
- `COORDINATOR_URL`: Worker target gRPC URL (e.g., `http://localhost:50051`)
- `RUST_LOG`: Logging filter (e.g., `info,coordinator=debug`)
