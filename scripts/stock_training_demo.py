#!/usr/bin/env python3
"""
Stock Market Prediction Training Demo

Demonstrates distributed training across multiple workers,
each training on different stock data (AAPL, GOOGL, MSFT, AMZN).

This shows:
- Workers registering and getting assigned different stocks
- Parallel training with progress updates
- Checkpoint saving after each epoch
- Barrier sync between epochs
- Real-time dashboard updates
"""

import sys
import os
import time
import random
import threading
import grpc
from typing import List, Dict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import coordinator_pb2
import coordinator_pb2_grpc

# Stock data configuration
STOCKS = {
    'AAPL': {'name': 'Apple Inc.', 'samples': 50000, 'features': 128},
    'GOOGL': {'name': 'Alphabet Inc.', 'samples': 48000, 'features': 128},
    'MSFT': {'name': 'Microsoft Corp.', 'samples': 52000, 'features': 128},
    'AMZN': {'name': 'Amazon.com Inc.', 'samples': 45000, 'features': 128},
    'NVDA': {'name': 'NVIDIA Corp.', 'samples': 35000, 'features': 128},
    'TSLA': {'name': 'Tesla Inc.', 'samples': 30000, 'features': 128},
    'META': {'name': 'Meta Platforms', 'samples': 42000, 'features': 128},
    'JPM': {'name': 'JPMorgan Chase', 'samples': 55000, 'features': 128},
}

EPOCHS = 3
BATCH_SIZE = 256
LEARNING_RATE = 0.001

# Shared results
results = {}
results_lock = threading.Lock()


def simulate_training_step(stock: str, epoch: int, step: int, total_steps: int) -> Dict:
    """Simulate a training step with realistic metrics."""
    # Simulate decreasing loss over time
    base_loss = 0.8 - (epoch * 0.2) - (step / total_steps * 0.1)
    loss = base_loss + random.uniform(-0.05, 0.05)
    
    # Simulate increasing accuracy
    base_acc = 0.5 + (epoch * 0.15) + (step / total_steps * 0.05)
    accuracy = min(0.95, base_acc + random.uniform(-0.02, 0.02))
    
    # Simulate GPU metrics
    gpu_util = random.uniform(85, 98)
    gpu_mem = random.uniform(70, 90)
    
    return {
        'loss': round(loss, 4),
        'accuracy': round(accuracy, 4),
        'gpu_util': round(gpu_util, 1),
        'gpu_mem': round(gpu_mem, 1),
    }


def run_stock_worker(worker_id: str, rank: int, stock_symbol: str, 
                     coordinator_url: str = "localhost:50051",
                     registration_barrier=None, epoch_barriers=None):
    """Worker that trains on a specific stock's data."""
    
    stock_info = STOCKS[stock_symbol]
    print(f"[{worker_id}] 📈 Training on {stock_symbol} ({stock_info['name']})")
    
    # Connect to coordinator
    options = [
        ('grpc.keepalive_time_ms', 10000),
        ('grpc.keepalive_timeout_ms', 5000),
    ]
    channel = grpc.insecure_channel(coordinator_url, options=options)
    stub = coordinator_pb2_grpc.CoordinatorStub(channel)

    # Register worker
    try:
        response = stub.RegisterWorker(
            coordinator_pb2.WorkerInfo(
                worker_id=worker_id,
                hostname=f"{stock_symbol.lower()}-trainer.local",
                port=50052 + rank,
                gpu_count=2,
                memory_bytes=32 * 1024**3,
            ),
            timeout=30
        )
        print(f"[{worker_id}] ✓ Registered (rank {response.rank}/{response.world_size})")
    except grpc.RpcError as e:
        print(f"[{worker_id}] ✗ Registration failed: {e.details()}")
        return
    
    # Wait for all workers
    if registration_barrier:
        registration_barrier.wait()
    
    # Calculate training parameters
    total_samples = stock_info['samples']
    steps_per_epoch = total_samples // BATCH_SIZE
    
    print(f"[{worker_id}] 📊 Dataset: {total_samples:,} samples, {steps_per_epoch} steps/epoch")
    
    # Send initial heartbeat
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.LOADING_DATA,
            current_step=0,
            current_epoch=0,
            current_task=f"loading_{stock_symbol}_data",
        ),
    ), timeout=10)
    
    time.sleep(0.5)  # Simulate data loading

    final_metrics = None
    
    # Training loop
    for epoch in range(EPOCHS):
        print(f"[{worker_id}] 🔄 Epoch {epoch + 1}/{EPOCHS}")
        
        # Update status to training
        stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
            worker_id=worker_id,
            timestamp_ms=int(time.time() * 1000),
            status=coordinator_pb2.WorkerStatus(
                state=coordinator_pb2.WorkerStatus.TRAINING,
                current_step=0,
                current_epoch=epoch,
                current_task=f"training_{stock_symbol}_epoch{epoch}",
            ),
        ), timeout=10)
        
        # Simulate training steps (just a few for demo)
        for step in range(min(5, steps_per_epoch)):
            metrics = simulate_training_step(stock_symbol, epoch, step, steps_per_epoch)
            time.sleep(0.2)  # Simulate computation
            
            if step == 4 or step == steps_per_epoch - 1:
                print(f"[{worker_id}]   Step {step+1}: loss={metrics['loss']:.4f}, acc={metrics['accuracy']:.2%}")
                final_metrics = metrics
        
        # Save checkpoint after epoch
        stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
            worker_id=worker_id,
            timestamp_ms=int(time.time() * 1000),
            status=coordinator_pb2.WorkerStatus(
                state=coordinator_pb2.WorkerStatus.CHECKPOINTING,
                current_step=steps_per_epoch,
                current_epoch=epoch,
                current_task=f"checkpoint_{stock_symbol}_epoch{epoch}",
            ),
        ), timeout=10)

        # Notify checkpoint
        ckpt_size = random.randint(100, 500) * 1024 * 1024  # 100-500 MB
        stub.NotifyCheckpoint(coordinator_pb2.CheckpointInfo(
            worker_id=worker_id,
            checkpoint_id=f"ckpt-{stock_symbol}-epoch{epoch}",
            step=steps_per_epoch,
            epoch=epoch,
            storage_path=f"/checkpoints/{stock_symbol}/epoch_{epoch}.pt",
            size_bytes=ckpt_size,
            timestamp_ms=int(time.time() * 1000),
            type=coordinator_pb2.FULL,
        ), timeout=10)
        
        print(f"[{worker_id}] 💾 Checkpoint saved ({ckpt_size // (1024*1024)} MB)")
        
        # Sync at epoch barrier
        if epoch_barriers and epoch < len(epoch_barriers):
            try:
                barrier = stub.WaitBarrier(coordinator_pb2.BarrierRequest(
                    worker_id=worker_id,
                    barrier_id=f"epoch-{epoch}-complete",
                    step=epoch,
                ), timeout=60)
                print(f"[{worker_id}] 🔗 Epoch {epoch+1} sync complete ({barrier.arrival_order}/{barrier.participants})")
            except grpc.RpcError:
                pass
            
            if epoch_barriers:
                epoch_barriers[epoch].wait()
    
    # Final status
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.IDLE,
            current_step=steps_per_epoch,
            current_epoch=EPOCHS,
            current_task="complete",
        ),
    ), timeout=10)
    
    # Store results
    with results_lock:
        results[stock_symbol] = final_metrics
    
    print(f"[{worker_id}] ✅ Training complete!")


def main():
    num_workers = int(sys.argv[1]) if len(sys.argv) > 1 else 4
    num_workers = min(num_workers, len(STOCKS))
    
    stock_list = list(STOCKS.keys())[:num_workers]
    
    print("=" * 65)
    print(" 📈 DISTRIBUTED STOCK PREDICTION MODEL TRAINING")
    print("=" * 65)
    print()
    print(f"Training {num_workers} models in parallel on different stocks:")
    for i, stock in enumerate(stock_list):
        info = STOCKS[stock]
        print(f"  Worker {i}: {stock} ({info['name']}) - {info['samples']:,} samples")
    print()
    print(f"Configuration: {EPOCHS} epochs, batch_size={BATCH_SIZE}, lr={LEARNING_RATE}")
    print()
    print("📊 Watch the dashboard at http://localhost:3000")
    print()
    print("-" * 65)
    
    # Create barriers
    registration_barrier = threading.Barrier(num_workers)
    epoch_barriers = [threading.Barrier(num_workers) for _ in range(EPOCHS)]
    
    # Start workers
    threads = []
    for i, stock in enumerate(stock_list):
        worker_id = f"{stock.lower()}-trainer"
        t = threading.Thread(
            target=run_stock_worker,
            args=(worker_id, i, stock, "localhost:50051", registration_barrier, epoch_barriers)
        )
        t.start()
        threads.append(t)
        time.sleep(0.1)
    
    # Wait for completion
    for t in threads:
        t.join()
    
    print()
    print("=" * 65)
    print(" 📊 TRAINING RESULTS")
    print("=" * 65)
    print()
    
    if results:
        print(f"{'Stock':<8} {'Final Loss':<12} {'Accuracy':<12}")
        print("-" * 35)
        for stock in stock_list:
            if stock in results:
                m = results[stock]
                print(f"{stock:<8} {m['loss']:<12.4f} {m['accuracy']:<12.2%}")
        
        avg_acc = sum(r['accuracy'] for r in results.values()) / len(results)
        print("-" * 35)
        print(f"{'Average':<8} {'':<12} {avg_acc:<12.2%}")
    
    print()
    print("✅ All models trained! Check dashboard for worker activity.")


if __name__ == "__main__":
    main()
