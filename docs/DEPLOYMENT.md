# Deployment Guide

What has and has not been verified:

| Path | Status |
|------|--------|
| Build from source, run coordinator, Python package, tests, benchmarks | Verified on Linux (see [BENCHMARKS.md](../BENCHMARKS.md) for the machine) |
| `docker compose up` (`docker-compose.yml`) | Files exist; **not verified** during the latest cleanup (no Docker daemon available) |
| Kubernetes (`k8s/`) | Manifests exist; **not verified** against a cluster |
| S3 checkpoint storage | **Not available** from the coordinator or Python API (see [S3 status](#s3-status)) |

There is no authentication or TLS on the gRPC or HTTP endpoints. Only run the coordinator on a trusted network.

---

## Build from Source

Prerequisites: Rust (stable, via `rustup`), `protoc` (e.g. `apt-get install protobuf-compiler`), Python 3.10+.

```bash
git clone https://github.com/syrilj/Strata.git
cd Strata

# Rust workspace
cargo build --release

# Python package (builds the PyO3 extension via maturin)
python3 -m venv .venv && . .venv/bin/activate
pip install -e ".[dev]"
```

## Run the Coordinator

```bash
# gRPC on 50051; HTTP API defaults to gRPC port + 1000 (51051)
cargo run --release -p coordinator --bin coordinator -- 127.0.0.1:50051
```

Check it is up:

```bash
curl http://127.0.0.1:51051/api/health
```

Use `0.0.0.0:50051` to accept connections from other hosts (no auth — trusted networks only).

### Environment variables read by the coordinator

| Variable | Meaning | Default |
|----------|---------|---------|
| `GRPC_ADDR` | gRPC listen address (CLI argument also accepted) | `0.0.0.0:50051` |
| `HTTP_ADDR` | HTTP API listen address | gRPC port + 1000 |
| `CHECKPOINT_DIR` | Local checkpoint directory | `./data` |
| `DEMO_MODE` | `true` serves a fixed synthetic cluster for UI demos | `false` |
| `HEARTBEAT_TIMEOUT_SECS` | Worker considered dead after this long without a heartbeat | `30` |
| `DEAD_WORKER_CHECK_INTERVAL_SECS` | Reaper interval | `5` |
| `MAX_WORKERS` | Registration limit | `10000` |
| `CHECKPOINT_COMPRESSION`, `CHECKPOINT_COMPRESSION_LEVEL`, `MAX_CHECKPOINTS_KEPT`, `WRITE_BUFFER_SIZE` | Checkpoint manager settings | see `config.rs` |
| `STRATA_CONFIG` | Path to a TOML config file (same as `--config <path>`; example: `config/production.toml`) | unset |
| `RUST_LOG` | `tracing` filter, e.g. `info,coordinator=debug` | `info` |

`STORAGE_BACKEND`, `CHECKPOINT_BUCKET`, `AWS_REGION` and `MINIO_ENDPOINT` are parsed but have no effect on where checkpoints are written; see below.

## Run a Worker

See the Quick Start in the [README](../README.md) for a minimal worker script, and `scripts/real_worker.py` for a longer one that uses the PyO3 package. `./run-local.sh` starts a coordinator, local workers and the dashboard in one go.

## S3 Status

The `storage` crate contains an `S3Storage` implementation (multipart upload, retry with exponential backoff, custom endpoint support for MinIO/LocalStack) behind the non-default `s3` cargo feature. However:

- The coordinator binary does not enable that feature. If `STORAGE_BACKEND=s3` is set, it logs a warning and **writes checkpoints to local disk** (`CHECKPOINT_DIR`).
- The Python `CheckpointManager` only accepts a local directory.
- `S3Storage` has unit tests for key and config handling only; it has not been run against S3 or MinIO.

Do not drop the checkpoint volume/PVC on the assumption that checkpoints go to S3.

## Docker (not verified in the latest cleanup)

`docker-compose.yml` builds:

- `coordinator` from `Dockerfile`: Rust coordinator + built dashboard, ports 50051 (gRPC), 51051 (HTTP), 3000 (dashboard)
- four `worker-*` services from `Dockerfile.worker`: `scripts/simulated_worker.py`, a plain gRPC client that sends heartbeats and simulated progress. These containers do not use the Rust/PyO3 package.

```bash
docker compose up --build
```

`docker-compose.prod.yml` runs only the coordinator. Despite setting `STORAGE_BACKEND=s3`, it stores checkpoints on the container's local filesystem (see [S3 Status](#s3-status)) and mounts no volume, so checkpoints are lost when the container is removed.

## Kubernetes (not verified against a cluster)

`k8s/` contains a Kustomize set: namespace, ConfigMap, a 20Gi PVC for checkpoints, a single-replica coordinator Deployment (`Recreate` strategy) with a Service, a worker Deployment (4 replicas of the simulated worker image) with an HPA (2–10 replicas at 70% CPU), and an nginx Ingress for host `strata.local`.

```bash
# Build images and push them to a registry your cluster can pull from,
# then set `image:` in k8s/coordinator.yaml and k8s/workers.yaml.
docker build -t <registry>/strata-coordinator:latest .
docker build -f Dockerfile.worker -t <registry>/strata-worker:latest .
docker push <registry>/strata-coordinator:latest
docker push <registry>/strata-worker:latest

kubectl apply -k k8s/
kubectl get pods -n strata

# Without an ingress controller:
kubectl port-forward -n strata svc/coordinator 3000:3000 51051:51051
```

The coordinator is a single replica by design: its state is in memory and it is not highly available.

## Troubleshooting

- **`protoc` not found during build**: install `protobuf-compiler` (the coordinator's `build.rs` compiles `proto/coordinator.proto`).
- **Workers can't connect**: the coordinator must bind `0.0.0.0` (not `127.0.0.1`) to be reachable from other hosts; check firewalls for 50051/51051.
- **Python import error for `dtruntime._core`**: re-run `pip install -e ".[dev]"` in the active virtualenv.
- **Logs**: set `RUST_LOG=debug` for verbose coordinator logs. There is no metrics exporter (Prometheus/OpenTelemetry); the HTTP API exposes `/api/metrics` and `/api/metrics/history` as JSON.
