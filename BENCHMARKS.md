# Benchmarks

All numbers below come from **one run** of the Criterion benchmarks in this repository on **one machine**, with checkpoints written to **local storage**. Nothing here was measured on a multi-node cluster, on GPUs, or against S3. Do not read these as cluster-scale results.

If you change a benchmark or re-run it on different hardware, replace the table and the environment section together. Do not add numbers that were not measured (see `CLAUDE.md`).

## Environment

| | |
|---|---|
| Date | 2026-11-22, 03:30–03:39 UTC |
| CPU | Intel Xeon @ 2.10 GHz, 4 vCPUs (KVM guest, 1 thread per core) |
| RAM | 15 GiB |
| OS | Ubuntu 24.04.5 LTS, Linux 6.18 |
| Storage | ext4 on a virtio block device (`/tmp`, where the benchmarks write) |
| Rust | rustc 1.97.0, `release` profile (Criterion 0.5) |
| Commit | `3829e9a` (benchmark code as of the "fix(bench)" commit) |

This was a shared cloud VM; expect run-to-run noise, especially in the gRPC and fsync numbers.

## Command

```bash
cargo bench --workspace
```

Criterion's default settings were used (3 s warm-up, 5 s measurement; checkpoint groups use 20 samples). Values are Criterion's point estimate with its 95% confidence interval. Throughput is derived from the point estimate.

## Results

### Checkpoint storage (`benchmarks/benches/checkpoint_throughput.rs`)

`local_storage_write` times one `LocalStorage::write`: write a temp file, `fsync`, rename into place. This is the path the background checkpoint writer uses.

| Benchmark | Time | 95% CI | Throughput |
|---|---|---|---|
| `local_storage_write/1MB` | 2.15 ms | 2.11 – 2.19 ms | 443 MiB/s |
| `local_storage_write/10MB` | 13.1 ms | 12.8 – 13.5 ms | 726 MiB/s |
| `local_storage_write/100MB` | 137 ms | 135 – 139 ms | 698 MiB/s |
| `local_storage_concurrent_write/1` (1 × 1 MB) | 2.30 ms | 2.24 – 2.35 ms | 415 MiB/s |
| `local_storage_concurrent_write/4` (4 × 1 MB) | 5.41 ms | 5.32 – 5.54 ms | 705 MiB/s |
| `local_storage_concurrent_write/8` (8 × 1 MB) | 9.19 ms | 8.98 – 9.45 ms | 830 MiB/s |

`checkpoint_save_async_call` times only the Rust `CheckpointManager::save_async` call, which queues the write for a background task and returns. Manager construction and waiting for the write to finish are excluded. This is how long a training loop is blocked by queuing a checkpoint, **not** a storage throughput number.

| Benchmark | Time | 95% CI |
|---|---|---|
| `checkpoint_save_async_call/1MB` | 10.5 µs | 9.89 – 11.2 µs |
| `checkpoint_save_async_call/10MB` | 11.0 µs | 10.2 – 11.5 µs |

### Coordinator gRPC (`benchmarks/benches/coordinator.rs`)

Each benchmark starts a real coordinator on `127.0.0.1` in the same process. **Every iteration opens a new gRPC connection** (`CoordinatorClient::connect`) before making its calls, so these times include connection setup and are not per-RPC latencies on an established connection.

| Benchmark | What one iteration does | Time | 95% CI |
|---|---|---|---|
| `worker_registration/register_single_worker` | connect + 1 `RegisterWorker` | 6.00 ms | 5.21 – 6.82 ms |
| `heartbeat/heartbeat_10_workers` | connect + 10 sequential `Heartbeat` calls | 8.51 ms | 7.06 – 10.0 ms |
| `shard_assignment/get_shard` | connect + 1 `GetDataShard` (4 workers, 100 shards) | 5.17 ms | 4.39 – 5.98 ms |
| `barrier_sync/2` | 2 tasks each connect + `WaitBarrier`; until all released | 24.1 ms | 19.8 – 28.4 ms |
| `barrier_sync/4` | same, 4 tasks | 38.5 ms | 35.5 – 41.3 ms |
| `barrier_sync/8` | same, 8 tasks | 37.2 ms | 34.2 – 39.9 ms |

The barrier and connection costs have not been profiled; the numbers are reported as measured.

### Shard assignment, in-process (`benchmarks/benches/data_loading.rs`)

No network. `shard_assignment/<W>_workers/<S>_shards` creates a `ShardManager`, registers a dataset with `S` shards (`shuffle=true`), registers `W` workers, then asks for every worker's shards for epoch 0. All of that is inside the timed region, so the cost is dominated by building the manager and registering workers, not by a single lookup.

| Benchmark | Time | 95% CI |
|---|---|---|
| `shard_assignment/10_workers/100_shards` | 400 µs | 380 – 421 µs |
| `shard_assignment/10_workers/1000_shards` | 486 µs | 472 – 503 µs |
| `shard_assignment/10_workers/10000_shards` | 1.22 ms | 1.22 – 1.22 ms |
| `shard_assignment/100_workers/100_shards` | 3.99 ms | 3.91 – 4.07 ms |
| `shard_assignment/100_workers/1000_shards` | 4.21 ms | 4.13 – 4.31 ms |
| `shard_assignment/100_workers/10000_shards` | 6.64 ms | 6.59 – 6.69 ms |
| `shard_assignment/1000_workers/100_shards` | 58.1 ms | 56.4 – 60.1 ms |
| `shard_assignment/1000_workers/1000_shards` | 57.4 ms | 55.9 – 59.1 ms |
| `shard_assignment/1000_workers/10000_shards` | 77.8 ms | 76.9 – 78.9 ms |
| `epoch_management/create_and_track_100_epochs` (10 workers, 100 epochs) | 9.64 ms | 9.54 – 9.76 ms |
| `parallel_shard_access/1` (1 task × 10 epochs) | 6.98 ms | 6.87 – 7.10 ms |
| `parallel_shard_access/2` | 5.06 ms | 4.98 – 5.14 ms |
| `parallel_shard_access/4` | 3.44 ms | 3.41 – 3.48 ms |
| `parallel_shard_access/8` | 4.48 ms | 4.42 – 4.55 ms |

The "1000 workers" cases are 1000 worker IDs registered in one in-process `ShardManager`; they say nothing about running 1000 real workers.

### Consistent hash ring (`benchmarks/benches/data_loading.rs`, `crates/data-shard/benches/consistent_hash.rs`)

| Benchmark | What it times | Time | 95% CI |
|---|---|---|---|
| `get_node` | one key lookup, 10-node ring | 49.5 ns | 49.0 – 50.2 ns |
| `add_node` | new ring + add 1 node (150 virtual nodes) | 22.1 µs | 21.4 – 22.8 µs |
| `get_shards_for_node/100` | shards owned by 1 node, 10-node ring, 100 shards | 13.4 µs | 13.0 – 13.9 µs |
| `get_shards_for_node/1000` | same, 1,000 shards | 122 µs | 121 – 123 µs |
| `get_shards_for_node/10000` | same, 10,000 shards | 1.37 ms | 1.33 – 1.41 ms |
| `consistent_hash_distribution/10` | build 10-node ring + place 10,000 shards | 2.06 ms | 2.01 – 2.12 ms |
| `consistent_hash_distribution/100` | build 100-node ring + place 10,000 shards | 5.58 ms | 5.50 – 5.69 ms |
| `consistent_hash_distribution/1000` | build 1,000-node ring + place 10,000 shards | 48.2 ms | 47.8 – 48.7 ms |
| `distribution_100k_shards` | place 100,000 shards on a prebuilt 100-node ring | 18.1 ms | 18.0 – 18.3 ms |
| `shard_rebalancing/add_worker_to_100_worker_cluster` | build 100-node ring, place 10,000 shards, add 1 node, place again, count moves | 8.32 ms | 8.12 – 8.54 ms |

These benchmarks time the operations only. They do not report distribution quality (how evenly shards are spread) or how many shards moved; those have not been measured.

## Not measured

- Anything over a real network or with more than one machine
- S3 (the coordinator does not use S3; see `docs/DEPLOYMENT.md`)
- End-to-end training throughput, GPU workloads, or comparisons with other systems
- The Python API's overhead
- Load-balance quality of the hash ring
