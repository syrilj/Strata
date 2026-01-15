# Deployment Guide

## Local Development Setup

### Prerequisites

- Rust 1.75+ ([install](https://rustup.rs/))
- Python 3.9+ 
- Cargo and pip
- (Optional) Docker for containerized deployment

### Build from Source

```bash
# Clone repository
git clone https://github.com/user/distributed-training-runtime.git
cd distributed-training-runtime

# Build Rust components
cargo build --release

# Install Python package in development mode
pip install -e ".[dev]"
```

### Run Tests

```bash
# Rust tests
cargo test --all

# Python tests (requires coordinator to be running)
pytest tests/python/ -v

# Integration tests
cargo test -p integration-tests
```

### Run Benchmarks

```bash
# Checkpoint benchmarks
cargo bench --bench checkpoint_throughput

# Coordinator benchmarks
cargo bench --bench coordinator

# Data loading benchmarks
cargo bench --bench data_loading

# Generate HTML report
cargo bench -- --save-baseline main
```

---

## Single-Node Deployment

### Start Coordinator

```bash
# Run coordinator on default port (50051)
cargo run --release -p coordinator --bin coordinator -- 0.0.0.0:50051
```

Or with custom configuration:

```bash
# Create config file
cat > coordinator.toml <<EOF
[coordinator]
address = "0.0.0.0:50051"
heartbeat_interval_ms = 1000
heartbeat_timeout_ms = 30000

[storage]
backend = "local"
path = "/tmp/checkpoints"
EOF

# Run with config
RUST_LOG=info cargo run --release -p coordinator --bin coordinator -- 0.0.0.0:50051
```

### Run Training Worker

```python
# train.py
import asyncio
from dtruntime import TrainingOrchestrator, DatasetRegistry, CheckpointManager

async def main():
    # Connect to coordinator
    orch = TrainingOrchestrator(
        worker_id="worker-0",
        coordinator_url="http://localhost:50051",
        world_size=1,
        rank=0
    )
    
    await orch.register_worker(ip="127.0.0.1", port=8080)
    
    # Training code here...
    print("Training started!")

if __name__ == "__main__":
    asyncio.run(main())
```

```bash
python train.py
```

---

## Multi-Node Deployment

### Architecture

```
┌─────────────────┐
│  Coordinator    │  (1 instance)
│  coordinator:0  │
└─────────────────┘
        │
    ┌───┴───┬───────┬───────┐
    │       │       │       │
┌───┴───┐ ┌─┴─────┐ ┌─────┴─┐ ┌─────────┐
│Worker0│ │Worker1│ │Worker2│ │ Worker3 │
│ GPU×8 │ │ GPU×8 │ │ GPU×8 │ │  GPU×8  │
└───────┘ └───────┘ └───────┘ └─────────┘
```

### Step 1: Start Coordinator

On coordinator node:

```bash
# Bind to all interfaces
cargo run --release -p coordinator --bin coordinator -- 0.0.0.0:50051
```

Make sure port 50051 is accessible from worker nodes (firewall rules).

### Step 2: Launch Workers

On each worker node:

```bash
# Set environment variables
export COORDINATOR_URL="http://<coordinator-ip>:50051"
export RANK=0  # 0, 1, 2, 3 for each worker
export WORLD_SIZE=4
export WORKER_ID="worker-${RANK}"

# Run training script
python train.py
```

### Step 3: Distributed Training Script

```python
import os
import asyncio
import socket
from dtruntime import TrainingOrchestrator, DatasetRegistry, CheckpointManager

async def main():
    rank = int(os.environ["RANK"])
    world_size = int(os.environ["WORLD_SIZE"])
    coordinator_url = os.environ["COORDINATOR_URL"]
    
    # Initialize orchestrator
    orch = TrainingOrchestrator(
        worker_id=f"worker-{rank}",
        coordinator_url=coordinator_url,
        world_size=world_size,
        rank=rank
    )
    
    # Register worker
    hostname = socket.gethostname()
    ip = socket.gethostbyname(hostname)
    await orch.register_worker(ip=ip, port=8080 + rank)
    
    # Register dataset (only rank 0)
    if rank == 0:
        registry = DatasetRegistry(coordinator_url)
        registry.register(
            "training_data",
            "/shared/data",  # NFS or shared storage
            "parquet",
            total_samples=1_000_000,
            shard_size=10_000
        )
    
    # Wait for dataset registration
    await orch.wait_barrier("dataset_ready")
    
    # Get shard for this worker
    registry = DatasetRegistry(coordinator_url)
    shard_files = registry.get_shard("training_data", worker_rank=rank, epoch=0)
    
    print(f"Worker {rank} got {len(shard_files)} shard files")
    
    # Training loop...

if __name__ == "__main__":
    asyncio.run(main())
```

---

## Docker Deployment

### Build Docker Images

**Coordinator Dockerfile**:

```dockerfile
# Dockerfile.coordinator
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release -p coordinator

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/coordinator /usr/local/bin/coordinator

EXPOSE 50051
CMD ["coordinator", "0.0.0.0:50051"]
```

**Worker Dockerfile**:

```dockerfile
# Dockerfile.worker
FROM nvidia/cuda:12.1.0-base-ubuntu22.04

# Install Rust
RUN apt-get update && apt-get install -y curl build-essential python3.10 python3-pip
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy and build
WORKDIR /app
COPY . .
RUN cargo build --release
RUN pip3 install -e .

CMD ["python3", "train.py"]
```

### Build and Run

```bash
# Build images
docker build -f Dockerfile.coordinator -t dt-coordinator .
docker build -f Dockerfile.worker -t dt-worker .

# Run coordinator
docker run -d --name coordinator \
  -p 50051:50051 \
  dt-coordinator

# Run workers
for i in {0..3}; do
  docker run -d --name worker-$i \
    --gpus all \
    -e COORDINATOR_URL="http://coordinator:50051" \
    -e RANK=$i \
    -e WORLD_SIZE=4 \
    --link coordinator \
    dt-worker
done
```

---

## Kubernetes Deployment

### Coordinator Deployment

```yaml
# coordinator-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: coordinator
spec:
  replicas: 1
  selector:
    matchLabels:
      app: coordinator
  template:
    metadata:
      labels:
        app: coordinator
    spec:
      containers:
      - name: coordinator
        image: dt-coordinator:latest
        ports:
        - containerPort: 50051
        resources:
          requests:
            cpu: "2"
            memory: "4Gi"
          limits:
            cpu: "4"
            memory: "8Gi"
---
apiVersion: v1
kind: Service
metadata:
  name: coordinator
spec:
  selector:
    app: coordinator
  ports:
  - port: 50051
    targetPort: 50051
  type: ClusterIP
```

### Worker StatefulSet

```yaml
# worker-statefulset.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: worker
spec:
  serviceName: "worker"
  replicas: 4
  selector:
    matchLabels:
      app: worker
  template:
    metadata:
      labels:
        app: worker
    spec:
      containers:
      - name: worker
        image: dt-worker:latest
        env:
        - name: COORDINATOR_URL
          value: "http://coordinator:50051"
        - name: WORLD_SIZE
          value: "4"
        - name: RANK
          valueFrom:
            fieldRef:
              fieldPath: metadata.labels['statefulset.kubernetes.io/pod-name']
        resources:
          requests:
            nvidia.com/gpu: 8
            cpu: "32"
            memory: "256Gi"
          limits:
            nvidia.com/gpu: 8
            cpu: "64"
            memory: "512Gi"
        volumeMounts:
        - name: data
          mountPath: /data
        - name: checkpoints
          mountPath: /checkpoints
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: [ "ReadOnlyMany" ]
      storageClassName: nfs-client
      resources:
        requests:
          storage: 1Ti
  - metadata:
      name: checkpoints
    spec:
      accessModes: [ "ReadWriteOnce" ]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 500Gi
```

### Deploy

```bash
# Apply configurations
kubectl apply -f coordinator-deployment.yaml
kubectl apply -f worker-statefulset.yaml

# Check status
kubectl get pods
kubectl logs -f coordinator-<pod-id>
kubectl logs -f worker-0

# Scale workers
kubectl scale statefulset worker --replicas=8
```

---

## Cloud Deployment (AWS)

### S3 Storage Backend

```python
from dtruntime import CheckpointManager

# Use S3 backend
manager = CheckpointManager(
    storage_path="s3://my-training-checkpoints/experiment-1",
    backend="s3"
)

# Checkpoints automatically saved to S3
await manager.save_async(data, step=1000)
```

### EC2 Setup

**Launch Instances**:

```bash
# Coordinator (t3.large)
aws ec2 run-instances \
  --image-id ami-0c55b159cbfafe1f0 \
  --instance-type t3.large \
  --key-name my-key \
  --security-group-ids sg-coordinator \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=coordinator}]'

# Workers (p4d.24xlarge with 8x A100)
for i in {0..3}; do
  aws ec2 run-instances \
    --image-id ami-0c55b159cbfafe1f0 \
    --instance-type p4d.24xlarge \
    --key-name my-key \
    --security-group-ids sg-worker \
    --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=worker-$i}]"
done
```

**Security Groups**:

```bash
# Coordinator security group
aws ec2 create-security-group \
  --group-name coordinator-sg \
  --description "Coordinator security group"

# Allow gRPC port
aws ec2 authorize-security-group-ingress \
  --group-name coordinator-sg \
  --protocol tcp \
  --port 50051 \
  --cidr 0.0.0.0/0
```

### EKS Deployment

```bash
# Create EKS cluster
eksctl create cluster \
  --name dt-cluster \
  --region us-west-2 \
  --nodegroup-name gpu-nodes \
  --node-type p3.16xlarge \
  --nodes 4 \
  --nodes-min 1 \
  --nodes-max 10

# Deploy to EKS
kubectl apply -f coordinator-deployment.yaml
kubectl apply -f worker-statefulset.yaml
```

---

## Configuration Options

### Coordinator Configuration

```toml
[coordinator]
# Bind address
address = "0.0.0.0:50051"

# Heartbeat settings
heartbeat_interval_ms = 1000  # How often workers send heartbeats
heartbeat_timeout_ms = 30000  # Mark worker dead after this timeout

# Worker failure handling
max_worker_failures = 3       # Max failures before job abort
failure_backoff_ms = 5000     # Wait before reassigning failed worker's shards

[storage]
backend = "s3"               # "local" or "s3"
bucket = "my-checkpoints"    # S3 bucket (if backend=s3)
region = "us-west-2"         # AWS region
path = "/checkpoints"        # Local path (if backend=local)

[checkpoint]
interval_steps = 1000        # Checkpoint every N steps
keep_last_n = 5              # Keep only last N checkpoints
compression = "gzip"         # "none", "gzip", or "zstd"
async_write = true           # Background async writes

[dataset]
default_shard_size = 10000   # Default samples per shard
prefetch_shards = 2          # Number of shards to prefetch
cache_size_gb = 10           # Dataset cache size
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `COORDINATOR_URL` | Coordinator address | `http://localhost:50051` |
| `WORKER_ID` | Worker identifier | `worker-0` |
| `RANK` | Worker rank | `0` |
| `WORLD_SIZE` | Total workers | `1` |
| `RUST_LOG` | Log level | `info` |
| `AWS_REGION` | AWS region for S3 | `us-west-2` |
| `CHECKPOINT_DIR` | Checkpoint directory | `/tmp/checkpoints` |

---

## Monitoring and Logging

### Metrics

The coordinator exposes metrics on port 9090 (Prometheus format):

```bash
curl http://coordinator:9090/metrics
```

**Key Metrics**:
- `coordinator_workers_active`: Number of active workers
- `coordinator_heartbeats_total`: Total heartbeats received
- `coordinator_checkpoints_total`: Total checkpoints saved
- `coordinator_barriers_waiting`: Workers waiting at barriers
- `checkpoint_write_duration_seconds`: Checkpoint write latency histogram
- `shard_assignment_duration_seconds`: Shard assignment latency

### Logging

```bash
# Set log level
export RUST_LOG=debug

# Log to file
cargo run --release -p coordinator 2>&1 | tee coordinator.log

# Structured JSON logging
export RUST_LOG=info
export LOG_FORMAT=json
```

### Tracing with Jaeger

```bash
# Start Jaeger
docker run -d --name jaeger \
  -p 6831:6831/udp \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest

# Enable tracing
export JAEGER_AGENT_HOST=localhost
export JAEGER_AGENT_PORT=6831

# View traces at http://localhost:16686
```

---

## Troubleshooting

### Coordinator Won't Start

```bash
# Check port availability
lsof -i :50051

# Check logs
RUST_LOG=debug cargo run -p coordinator

# Common issues:
# - Port already in use: Change port or kill existing process
# - Permission denied: Run with sudo or use port >1024
```

### Workers Can't Connect

```bash
# Test connectivity
telnet <coordinator-ip> 50051

# Check firewall
iptables -L

# DNS resolution
ping coordinator

# Common fixes:
# - Update security groups (cloud)
# - Check coordinator is binding to 0.0.0.0 not 127.0.0.1
# - Verify COORDINATOR_URL is correct
```

### Checkpoint Failures

```bash
# Check disk space
df -h /checkpoints

# Check permissions
ls -la /checkpoints

# Test S3 access
aws s3 ls s3://my-checkpoints/

# Common issues:
# - Disk full: Clean old checkpoints or increase storage
# - No S3 permissions: Check IAM role/credentials
# - Network timeout: Increase timeout in config
```

### Performance Issues

```bash
# Profile coordinator
cargo build --release
perf record -g ./target/release/coordinator
perf report

# Check resource usage
htop
nvidia-smi

# Common bottlenecks:
# - Disk I/O: Use faster storage (NVMe SSD)
# - Network: Check bandwidth with iperf
# - CPU: Increase coordinator CPU allocation
```
