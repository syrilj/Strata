#!/usr/bin/env python3
"""
Simulated distributed worker for stock market prediction training.
Each container trains on a different stock's data.
"""

import os
import sys
import time
import random
import socket
import grpc

sys.path.insert(0, '/app/proto')
import coordinator_pb2
import coordinator_pb2_grpc

# Stock assignments based on worker ID
STOCK_DATA = {
    'gpu-node-1': {'symbol': 'AAPL', 'name': 'Apple Inc.', 'samples': 50000},
    'gpu-node-2': {'symbol': 'GOOGL', 'name': 'Alphabet Inc.', 'samples': 48000},
    'gpu-node-3': {'symbol': 'MSFT', 'name': 'Microsoft Corp.', 'samples': 52000},
    'gpu-node-4': {'symbol': 'AMZN', 'name': 'Amazon.com Inc.', 'samples': 45000},
}

def get_stock_for_worker(worker_id):
    return STOCK_DATA.get(worker_id, {
        'symbol': 'SPY', 'name': 'S&P 500 ETF', 'samples': 40000
    })

def simulate_metrics(epoch, step, total_steps):
    """Generate realistic training metrics."""
    progress = (epoch * total_steps + step) / (3 * total_steps)
    base_loss = 0.9 - (progress * 0.6)
    base_acc = 0.45 + (progress * 0.45)
    return {
        'loss': max(0.1, base_loss + random.uniform(-0.05, 0.05)),
        'accuracy': min(0.95, base_acc + random.uniform(-0.03, 0.03)),
    }


def run_worker():
    worker_id = os.environ.get('WORKER_ID', f'worker-{random.randint(1000, 9999)}')
    coordinator_url = os.environ.get('COORDINATOR_URL', 'coordinator:50051')
    gpu_count = int(os.environ.get('GPU_COUNT', '4'))
    hostname = socket.gethostname()
    
    stock = get_stock_for_worker(worker_id)
    symbol = stock['symbol']
    
    print(f"[{worker_id}] 📈 Stock Prediction Worker")
    print(f"[{worker_id}] Training on: {symbol} ({stock['name']})")
    print(f"[{worker_id}] Dataset: {stock['samples']:,} samples")
    print(f"[{worker_id}] Connecting to coordinator at {coordinator_url}...")
    
    time.sleep(3)  # Wait for coordinator
    
    channel = grpc.insecure_channel(coordinator_url)
    stub = coordinator_pb2_grpc.CoordinatorStub(channel)
    
    # Register
    try:
        response = stub.RegisterWorker(coordinator_pb2.WorkerInfo(
            worker_id=worker_id,
            hostname=hostname,
            port=50052,
            gpu_count=gpu_count,
            memory_bytes=32 * 1024**3,
        ))
        print(f"[{worker_id}] ✓ Registered as rank {response.rank}/{response.world_size}")
    except grpc.RpcError as e:
        print(f"[{worker_id}] ✗ Registration failed: {e}")
        return

    # Register dataset
    try:
        stub.RegisterDataset(coordinator_pb2.DatasetInfo(
            dataset_id=f"stock-{symbol.lower()}",
            path=f"/data/stocks/{symbol}",
            format="parquet",
            total_samples=stock['samples'],
            shard_size=5000,
            shuffle=True,
            seed=42,
        ))
    except grpc.RpcError:
        pass  # May already exist

    # Training config
    epochs = 3
    batch_size = 256
    steps_per_epoch = stock['samples'] // batch_size
    
    print(f"[{worker_id}] 🚀 Starting training: {epochs} epochs, {steps_per_epoch} steps/epoch")
    
    for epoch in range(epochs):
        print(f"[{worker_id}] ═══ Epoch {epoch + 1}/{epochs} ═══")
        
        # Update status
        stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
            worker_id=worker_id,
            timestamp_ms=int(time.time() * 1000),
            status=coordinator_pb2.WorkerStatus(
                state=coordinator_pb2.WorkerStatus.TRAINING,
                current_step=0,
                current_epoch=epoch,
                current_task=f"training_{symbol}_epoch{epoch}",
            ),
            resources=coordinator_pb2.ResourceUsage(
                cpu_percent=random.uniform(70, 95),
                memory_used_bytes=int(random.uniform(20, 28) * 1024**3),
                gpu_usage=[
                    coordinator_pb2.GpuUsage(
                        gpu_id=i,
                        utilization_percent=random.uniform(88, 99),
                        memory_used_bytes=int(random.uniform(18, 22) * 1024**3),
                        memory_total_bytes=24 * 1024**3,
                        temperature_celsius=random.uniform(68, 82),
                    ) for i in range(gpu_count)
                ],
            ),
        ))
        
        # Simulate training (abbreviated for demo)
        for step in range(min(10, steps_per_epoch)):
            metrics = simulate_metrics(epoch, step, steps_per_epoch)
            time.sleep(0.3)
            
            if step % 5 == 4:
                print(f"[{worker_id}]   Step {step+1}: loss={metrics['loss']:.4f} acc={metrics['accuracy']:.1%}")

        # Checkpoint
        ckpt_size = random.randint(150, 400) * 1024 * 1024
        stub.NotifyCheckpoint(coordinator_pb2.CheckpointInfo(
            worker_id=worker_id,
            checkpoint_id=f"ckpt-{symbol}-epoch{epoch}",
            step=steps_per_epoch,
            epoch=epoch,
            storage_path=f"/checkpoints/{symbol}/epoch_{epoch}.pt",
            size_bytes=ckpt_size,
            timestamp_ms=int(time.time() * 1000),
            type=coordinator_pb2.FULL,
        ))
        print(f"[{worker_id}] 💾 Checkpoint: {ckpt_size // (1024*1024)} MB")
        
        # Barrier sync
        stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
            worker_id=worker_id,
            timestamp_ms=int(time.time() * 1000),
            status=coordinator_pb2.WorkerStatus(
                state=coordinator_pb2.WorkerStatus.CHECKPOINTING,
                current_step=steps_per_epoch,
                current_epoch=epoch,
                current_task=f"barrier_epoch{epoch}",
            ),
        ))
        
        try:
            barrier = stub.WaitBarrier(coordinator_pb2.BarrierRequest(
                worker_id=worker_id,
                barrier_id=f"epoch-{epoch}-sync",
                step=epoch,
            ), timeout=120)
            print(f"[{worker_id}] 🔗 Sync: {barrier.arrival_order}/{barrier.participants} workers")
        except grpc.RpcError as e:
            print(f"[{worker_id}] Barrier timeout, continuing...")
    
    # Done
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.IDLE,
            current_step=steps_per_epoch,
            current_epoch=epochs,
            current_task="complete",
        ),
    ))
    
    final = simulate_metrics(epochs-1, steps_per_epoch, steps_per_epoch)
    print(f"[{worker_id}] ✅ Training complete!")
    print(f"[{worker_id}] Final: loss={final['loss']:.4f} accuracy={final['accuracy']:.1%}")


if __name__ == '__main__':
    run_worker()
