#!/usr/bin/env python3
"""
Real distributed task demo - decrypt a message split across workers.

This demonstrates the coordinator actually coordinating work:
1. A secret message is encrypted and split into chunks
2. Each worker gets assigned a chunk to decrypt
3. Workers sync at barriers to ensure ordering
4. Final message is assembled from all workers' results

Run with: python3 scripts/distributed_task.py
"""

import sys
import os
import time
import hashlib
import base64
import threading
import grpc
from typing import List

# Add scripts dir to path for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import coordinator_pb2
import coordinator_pb2_grpc

# Simple XOR encryption for demo
def xor_encrypt(data: bytes, key: bytes) -> bytes:
    return bytes(d ^ key[i % len(key)] for i, d in enumerate(data))

def xor_decrypt(data: bytes, key: bytes) -> bytes:
    return xor_encrypt(data, key)  # XOR is symmetric

# The secret message to decrypt
SECRET_MESSAGE = """
===========================================
 DISTRIBUTED TRAINING RUNTIME - DEMO
===========================================

This message was encrypted and split across
multiple worker containers. Each worker:

1. Connected to the coordinator
2. Received its shard assignment  
3. Decrypted its portion of the message
4. Synchronized at barriers
5. Contributed to the final result

The coordinator managed:
- Worker registration & heartbeats
- Data shard distribution
- Checkpoint coordination
- Barrier synchronization

All without any worker knowing about others!

===========================================
"""

ENCRYPTION_KEY = b"distributed-training-2024"

# Shared state for collecting results
results = {}
results_lock = threading.Lock()


def encrypt_and_split(message: str, num_chunks: int) -> List[bytes]:
    """Encrypt message and split into chunks."""
    encrypted = xor_encrypt(message.encode(), ENCRYPTION_KEY)
    # Split evenly, keeping track of exact boundaries
    chunk_size = (len(encrypted) + num_chunks - 1) // num_chunks
    chunks = []
    for i in range(num_chunks):
        start = i * chunk_size
        end = min(start + chunk_size, len(encrypted))
        if start < len(encrypted):
            chunks.append((start, encrypted[start:end]))
    return chunks


def run_worker(worker_id: str, worker_rank: int, chunk_data: tuple, 
               coordinator_url: str = "localhost:50051", registration_barrier=None):
    """Worker that decrypts its chunk."""
    chunk_offset, encrypted_chunk = chunk_data
    
    print(f"[{worker_id}] Starting worker (rank {worker_rank})")
    
    # Configure channel with better connection settings
    options = [
        ('grpc.keepalive_time_ms', 10000),
        ('grpc.keepalive_timeout_ms', 5000),
        ('grpc.http2.min_time_between_pings_ms', 10000),
        ('grpc.http2.max_pings_without_data', 0),
    ]
    channel = grpc.insecure_channel(coordinator_url, options=options)
    stub = coordinator_pb2_grpc.CoordinatorStub(channel)
    
    # Register worker
    try:
        response = stub.RegisterWorker(
            coordinator_pb2.WorkerInfo(
                worker_id=worker_id,
                hostname=f"{worker_id}.container",
                port=50052 + worker_rank,
                gpu_count=1,
                memory_bytes=8 * 1024**3,
            ),
            timeout=30
        )
        print(f"[{worker_id}] Registered - Rank: {response.rank}, World: {response.world_size}")
    except grpc.RpcError as e:
        print(f"[{worker_id}] Registration failed: {e.details()}")
        channel.close()
        return
    
    # Wait for all workers to register before starting work
    if registration_barrier:
        print(f"[{worker_id}] Waiting for all workers to register...")
        registration_barrier.wait()
        print(f"[{worker_id}] All workers registered, starting work")
    
    # Simulate "loading" the encrypted data
    print(f"[{worker_id}] Received encrypted chunk: {len(encrypted_chunk)} bytes")
    
    # Send heartbeat - processing
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.LOADING_DATA,
            current_step=0,
            current_epoch=0,
            current_task="loading_encrypted_data",
        ),
    ), timeout=10)
    
    time.sleep(0.5)  # Simulate loading time
    
    # Decrypt the chunk using the correct offset for XOR key alignment
    print(f"[{worker_id}] Decrypting chunk...")
    
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.TRAINING,
            current_step=1,
            current_epoch=0,
            current_task="decrypting",
        ),
    ), timeout=10)
    
    # XOR decrypt with proper key offset
    key = ENCRYPTION_KEY
    decrypted = bytes(d ^ key[(chunk_offset + i) % len(key)] for i, d in enumerate(encrypted_chunk))
    
    time.sleep(0.3)  # Simulate processing time
    
    # Store result with offset for proper reassembly
    with results_lock:
        results[worker_rank] = (chunk_offset, decrypted)
    
    print(f"[{worker_id}] Decrypted {len(decrypted)} bytes")
    
    # Save "checkpoint" of our work
    stub.NotifyCheckpoint(coordinator_pb2.CheckpointInfo(
        worker_id=worker_id,
        checkpoint_id=f"decrypt-{worker_id}",
        step=1,
        epoch=0,
        storage_path=f"/results/{worker_id}.bin",
        size_bytes=len(decrypted),
        timestamp_ms=int(time.time() * 1000),
        type=coordinator_pb2.FULL,
    ), timeout=10)
    
    # Wait at barrier for all workers to finish
    print(f"[{worker_id}] Waiting at sync barrier...")
    
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.CHECKPOINTING,
            current_step=1,
            current_epoch=0,
            current_task="barrier_sync",
        ),
    ), timeout=10)
    
    try:
        barrier = stub.WaitBarrier(coordinator_pb2.BarrierRequest(
            worker_id=worker_id,
            barrier_id="decryption-complete",
            step=1,
        ), timeout=60)
        print(f"[{worker_id}] Barrier released! ({barrier.arrival_order}/{barrier.participants})")
    except grpc.RpcError as e:
        print(f"[{worker_id}] Barrier timeout - continuing anyway")
    
    # Final heartbeat
    stub.Heartbeat(coordinator_pb2.HeartbeatRequest(
        worker_id=worker_id,
        timestamp_ms=int(time.time() * 1000),
        status=coordinator_pb2.WorkerStatus(
            state=coordinator_pb2.WorkerStatus.IDLE,
            current_step=1,
            current_epoch=1,
            current_task="complete",
        ),
    ), timeout=10)
    
    print(f"[{worker_id}] Done!")


def main():
    num_workers = int(sys.argv[1]) if len(sys.argv) > 1 else 4
    
    print("=" * 60)
    print(" DISTRIBUTED DECRYPTION DEMO")
    print("=" * 60)
    print()
    print(f"Splitting encrypted message across {num_workers} workers...")
    print("Watch the dashboard at http://localhost:3000")
    print()
    
    # Encrypt and split the message
    chunks = encrypt_and_split(SECRET_MESSAGE, num_workers)
    
    print(f"Message encrypted: {len(SECRET_MESSAGE)} bytes -> {num_workers} chunks")
    print()
    
    # Use a barrier to ensure all workers register before starting work
    registration_barrier = threading.Barrier(num_workers)
    
    # Start workers
    threads = []
    for i in range(num_workers):
        worker_id = f"decrypt-worker-{i}"
        t = threading.Thread(
            target=run_worker, 
            args=(worker_id, i, chunks[i], "localhost:50051", registration_barrier)
        )
        t.start()
        threads.append(t)
        time.sleep(0.1)  # Small stagger to avoid connection storms
    
    # Wait for all workers
    for t in threads:
        t.join()
    
    print()
    print("=" * 60)
    print(" ASSEMBLING RESULTS FROM ALL WORKERS")
    print("=" * 60)
    print()
    
    # Assemble the decrypted message
    if len(results) == num_workers:
        # Sort by offset and concatenate
        sorted_results = sorted(results.values(), key=lambda x: x[0])
        final_message = b"".join(chunk for _, chunk in sorted_results)
        
        print("DECRYPTED MESSAGE:")
        print(final_message.decode('utf-8', errors='ignore'))
    else:
        print(f"ERROR: Only got {len(results)}/{num_workers} results")
    
    print()
    print("Demo complete! Check the dashboard for worker activity.")


if __name__ == "__main__":
    main()
