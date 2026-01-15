#!/usr/bin/env python3
"""
Test script that simulates multiple workers training.
Run this while the coordinator is running to see live data in the dashboard.
"""

import sys
import time
import random
import threading
import grpc

# Import generated protobuf
import coordinator_pb2
import coordinator_pb2_grpc


def run_worker(worker_id: str, gpu_count: int, coordinator_url: str = "localhost:50051"):
    """Simulate a single worker doing training."""
    print(f"[{worker_id}] Starting worker with {gpu_count} GPUs")
    
    channel = grpc.insecure_channel(coordinator_url)
    stub = coordinator_pb2_grpc.CoordinatorStub(channel)
    
    # Register worker
    try:
        response = stub.RegisterWorker(coordinator_pb2.WorkerInfo(
            worker_id=worker_id,
            hostname=f"{worker_id}.local",
            port=50052 + hash(worker_id) % 100,
            gpu_count=gpu_count,
            memory_bytes=64 * 1024 * 1024 * 1024,
        ))
        print(f"[{worker_id}] Registered! Rank: {response.rank}, World size: {response.world_size}")
    except grpc.RpcError as e:
        print(f"[{worker_id}] Failed to register: {e.code()} - {e.details()}")
        return
    
    # Register dataset (only first worker does this)
    if response.rank == 0:
        try:
            stub.RegisterDataset(coordinator_pb2.DatasetInfo(
                dataset_id="imagenet-1k",
                path="/data/imagenet",
                format="tfrecord",
                total_samples=1_281_167,
                shard_size=10_000,
                shuffle=True,
                seed=42,
            ))
            print(f"[{worker_id}] Dataset registered")
        except grpc.RpcError as e:
            print(f"[{worker_id}] Dataset registration: {e.code()}")
    
    # Simulate training
    step = 0
    for epoch in range(5):
        print(f"[{worker_id}] Starting epoch {epoch}")
        
        # Get shard assignment
        try:
            shard = stub.GetDataShard(coordinator_pb2.ShardRequest(
                worker_id=worker_id,
                dataset_id="imagenet-1k",
                epoch=epoch,
            ))
            print(f"[{worker_id}] Got shard {shard.shard_id}/{shard.total_shards}")
        except grpc.RpcError as e:
            print(f"[{worker_id}] Shard error: {e.code()}")
        
        # Training steps
        for batch in range(20):
            step += 1
            
            # Send heartbeat with metrics
            try:
                stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
                    worker_id=worker_id,
                    timestamp_ms=int(time.time() * 1000),
                    status=coordinator_pb2.WorkerStatus(
                        state=coordinator_pb2.WorkerStatus.TRAINING,
                        current_step=step,
                        current_epoch=epoch,
                        current_task="forward_backward",
                    ),
                    resources=coordinator_pb2.ResourceUsage(
                        cpu_percent=random.uniform(70, 95),
                        memory_used_bytes=random.randint(50, 60) * 1024**3,
                        gpu_usage=[
                            coordinator_pb2.GpuUsage(
                                gpu_id=i,
                                utilization_percent=random.uniform(90, 99),
                                memory_used_bytes=random.randint(70, 78) * 1024**3,
                                memory_total_bytes=80 * 1024**3,
                                temperature_celsius=random.uniform(65, 78),
                            )
                            for i in range(gpu_count)
                        ],
                    ),
                ))
            except grpc.RpcError as e:
                print(f"[{worker_id}] Heartbeat error: {e.code()}")
            
            time.sleep(0.3)  # Simulate training time
        
        # Checkpoint at end of epoch
        try:
            stub.NotifyCheckpoint(coordinator_pb2.CheckpointInfo(
                worker_id=worker_id,
                checkpoint_id=f"ckpt-epoch{epoch}-{worker_id}",
                step=step,
                epoch=epoch,
                storage_path=f"/checkpoints/epoch_{epoch}.pt",
                size_bytes=random.randint(200, 400) * 1024 * 1024,
                timestamp_ms=int(time.time() * 1000),
                type=coordinator_pb2.FULL,
            ))
            print(f"[{worker_id}] Checkpoint saved at epoch {epoch}")
        except grpc.RpcError as e:
            print(f"[{worker_id}] Checkpoint error: {e.code()}")
        
        # Barrier sync
        try:
            barrier = stub.WaitBarrier(coordinator_pb2.BarrierRequest(
                worker_id=worker_id,
                barrier_id=f"epoch-sync-{epoch}",
                step=step,
            ))
            print(f"[{worker_id}] Barrier released: {barrier.arrival_order}/{barrier.participants}")
        except grpc.RpcError as e:
            print(f"[{worker_id}] Barrier error: {e.code()}")
    
    print(f"[{worker_id}] Training complete!")
    
    # Deregister
    try:
        stub.DeregisterWorker(coordinator_pb2.WorkerInfo(worker_id=worker_id))
    except:
        pass


def main():
    num_workers = int(sys.argv[1]) if len(sys.argv) > 1 else 4
    
    print(f"Starting {num_workers} simulated workers...")
    print("Watch the dashboard at http://localhost:3000")
    print()
    
    workers = [
        ("gpu-node-0", 8),
        ("gpu-node-1", 8),
        ("gpu-node-2", 4),
        ("gpu-node-3", 4),
        ("gpu-node-4", 8),
        ("gpu-node-5", 8),
    ][:num_workers]
    
    threads = []
    for worker_id, gpu_count in workers:
        t = threading.Thread(target=run_worker, args=(worker_id, gpu_count))
        t.start()
        threads.append(t)
        time.sleep(0.5)  # Stagger worker starts
    
    for t in threads:
        t.join()
    
    print("\nAll workers finished!")


if __name__ == "__main__":
    main()
