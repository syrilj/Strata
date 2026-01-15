#!/usr/bin/env python3
"""
Demo script for Distributed Training Runtime
Shows active training tasks, workers, and real-time progress for interviews/presentations.
"""

import asyncio
import json
import time
import random
from datetime import datetime
from typing import Dict, List
import threading

# Simulated training state
class TrainingDemo:
    def __init__(self):
        self.workers = {
            "gpu-worker-01": {
                "id": "gpu-worker-01",
                "hostname": "gpu-node-01.cluster.local",
                "status": "training",
                "gpu_count": 8,
                "current_epoch": 3,
                "current_step": 1247,
                "total_steps": 2000,
                "current_task": "forward_pass",
                "loss": 0.234,
                "accuracy": 0.892,
                "gpu_utilization": [85, 87, 89, 84, 86, 88, 90, 83],
                "memory_used": 12.4,  # GB
                "last_heartbeat": time.time()
            },
            "gpu-worker-02": {
                "id": "gpu-worker-02", 
                "hostname": "gpu-node-02.cluster.local",
                "status": "training",
                "gpu_count": 8,
                "current_epoch": 3,
                "current_step": 1245,
                "total_steps": 2000,
                "current_task": "backward_pass",
                "loss": 0.241,
                "accuracy": 0.889,
                "gpu_utilization": [82, 84, 86, 88, 85, 87, 89, 81],
                "memory_used": 11.8,  # GB
                "last_heartbeat": time.time()
            },
            "cpu-worker-01": {
                "id": "cpu-worker-01",
                "hostname": "cpu-node-01.cluster.local", 
                "status": "loading_data",
                "gpu_count": 0,
                "current_epoch": 3,
                "current_step": 0,
                "total_steps": 2000,
                "current_task": "data_preprocessing",
                "cpu_utilization": 65,
                "memory_used": 4.2,  # GB
                "last_heartbeat": time.time()
            }
        }
        
        self.datasets = {
            "imagenet-train": {
                "id": "imagenet-train",
                "name": "ImageNet Training Set",
                "total_samples": 1281167,
                "processed_samples": 156000,
                "shard_count": 128,
                "format": "tfrecord",
                "status": "active"
            },
            "custom-model": {
                "id": "custom-model",
                "name": "Custom Vision Model",
                "total_samples": 500000,
                "processed_samples": 62000,
                "shard_count": 64,
                "format": "parquet",
                "status": "active"
            }
        }
        
        self.checkpoints = []
        self.barriers = []
        self.metrics = {
            "checkpoint_throughput": 45,
            "coordinator_rps": 127,
            "active_workers": 3,
            "total_workers": 3,
            "barrier_latency_p99": 23,
            "shard_assignment_time": 8
        }
        
        self.training_active = True
        self.start_time = time.time()

    def simulate_training_progress(self):
        """Simulate realistic training progress"""
        while self.training_active:
            # Update training progress
            for worker_id, worker in self.workers.items():
                if worker["status"] == "training":
                    # Advance steps occasionally
                    if random.random() < 0.3:
                        worker["current_step"] += 1
                        
                        # Update loss and accuracy with realistic values
                        worker["loss"] = max(0.1, worker["loss"] - random.uniform(0.001, 0.005))
                        worker["accuracy"] = min(0.95, worker["accuracy"] + random.uniform(0.001, 0.003))
                        
                        # Vary GPU utilization
                        worker["gpu_utilization"] = [
                            max(70, min(95, util + random.randint(-3, 3))) 
                            for util in worker["gpu_utilization"]
                        ]
                        
                        # Rotate tasks
                        tasks = ["forward_pass", "backward_pass", "gradient_sync", "parameter_update"]
                        worker["current_task"] = random.choice(tasks)
                        
                        # Check for epoch completion
                        if worker["current_step"] >= worker["total_steps"]:
                            worker["current_epoch"] += 1
                            worker["current_step"] = 0
                            
                            # Create checkpoint
                            checkpoint = {
                                "id": f"checkpoint_epoch_{worker['current_epoch']}_{int(time.time())}",
                                "epoch": worker["current_epoch"],
                                "step": worker["current_step"],
                                "worker_id": worker_id,
                                "size": random.randint(500, 800) * 1024 * 1024,  # 500-800MB
                                "created_at": int(time.time() * 1000),
                                "status": "completed"
                            }
                            self.checkpoints.append(checkpoint)
                            
                            # Keep only recent checkpoints
                            self.checkpoints = self.checkpoints[-10:]
                
                elif worker["status"] == "loading_data":
                    # Occasionally switch to training
                    if random.random() < 0.1:
                        worker["status"] = "training"
                        worker["current_task"] = "forward_pass"
                
                # Update heartbeat
                worker["last_heartbeat"] = time.time()
            
            # Update dataset progress
            for dataset in self.datasets.values():
                if dataset["status"] == "active":
                    dataset["processed_samples"] = min(
                        dataset["total_samples"],
                        dataset["processed_samples"] + random.randint(100, 500)
                    )
            
            # Update metrics
            self.metrics["checkpoint_throughput"] = random.randint(40, 60)
            self.metrics["coordinator_rps"] = random.randint(100, 150)
            self.metrics["barrier_latency_p99"] = random.randint(15, 35)
            
            # Occasionally create barriers
            if random.random() < 0.05:
                barrier = {
                    "id": f"epoch_sync_{int(time.time())}",
                    "name": f"Epoch {self.workers['gpu-worker-01']['current_epoch']} Sync",
                    "arrived": random.randint(1, 2),
                    "total": 3,
                    "status": "waiting",
                    "created_at": int(time.time() * 1000)
                }
                self.barriers.append(barrier)
                
                # Complete barrier after a short time
                def complete_barrier():
                    time.sleep(2)
                    if barrier in self.barriers:
                        barrier["arrived"] = barrier["total"]
                        barrier["status"] = "complete"
                        # Remove completed barriers after some time
                        threading.Timer(5, lambda: self.barriers.remove(barrier) if barrier in self.barriers else None).start()
                
                threading.Thread(target=complete_barrier, daemon=True).start()
            
            time.sleep(2)  # Update every 2 seconds

    def get_dashboard_state(self):
        """Get current state for dashboard API"""
        return {
            "coordinator": {
                "connected": True,
                "address": "localhost:50052",
                "uptime": int(time.time() - self.start_time),
                "version": "0.1.0"
            },
            "workers": [
                {
                    "id": w["id"],
                    "ip": w["hostname"].split('.')[0],
                    "port": 50052,
                    "status": w["status"],
                    "gpu_count": w["gpu_count"],
                    "last_heartbeat": int(w["last_heartbeat"] * 1000),
                    "assigned_shards": random.randint(8, 16),
                    "current_epoch": w["current_epoch"],
                    "current_step": w["current_step"],
                    "current_task": w["current_task"]
                }
                for w in self.workers.values()
            ],
            "datasets": [
                {
                    "id": d["id"],
                    "name": d["name"],
                    "total_samples": d["total_samples"],
                    "shard_size": 10000,
                    "shard_count": d["shard_count"],
                    "format": d["format"],
                    "shuffle": True,
                    "registered_at": int((time.time() - 3600) * 1000)  # 1 hour ago
                }
                for d in self.datasets.values()
            ],
            "checkpoints": [
                {
                    "id": c["id"],
                    "step": c["step"],
                    "epoch": c["epoch"],
                    "size": c["size"],
                    "path": f"/checkpoints/{c['id']}.pt",
                    "created_at": c["created_at"],
                    "worker_id": c["worker_id"],
                    "status": c["status"]
                }
                for c in self.checkpoints
            ],
            "barriers": [
                {
                    "id": b["id"],
                    "name": b["name"],
                    "arrived": b["arrived"],
                    "total": b["total"],
                    "status": b["status"],
                    "created_at": b["created_at"]
                }
                for b in self.barriers
            ],
            "metrics": self.metrics
        }

# Global demo instance
demo = TrainingDemo()

def start_demo():
    """Start the training simulation"""
    print("🚀 Starting Distributed Training Runtime Demo")
    print("=" * 50)
    print("📊 Simulating active training with:")
    print(f"  • {len(demo.workers)} workers (2 GPU + 1 CPU)")
    print(f"  • {len(demo.datasets)} datasets")
    print("  • Real-time progress updates")
    print("  • Checkpoint creation")
    print("  • Barrier synchronization")
    print()
    print("🌐 Dashboard: http://localhost:3000")
    print("🔧 API: http://localhost:51052/api")
    print()
    print("Press Ctrl+C to stop the demo")
    
    # Start simulation in background
    simulation_thread = threading.Thread(target=demo.simulate_training_progress, daemon=True)
    simulation_thread.start()
    
    try:
        while True:
            # Print current status
            active_workers = sum(1 for w in demo.workers.values() if w["status"] == "training")
            avg_step = sum(w["current_step"] for w in demo.workers.values()) // len(demo.workers)
            avg_loss = sum(w.get("loss", 0) for w in demo.workers.values() if "loss" in w) / max(1, sum(1 for w in demo.workers.values() if "loss" in w))
            
            print(f"\r⚡ Active: {active_workers} workers | Step: {avg_step} | Loss: {avg_loss:.3f} | Checkpoints: {len(demo.checkpoints)}", end="", flush=True)
            time.sleep(5)
            
    except KeyboardInterrupt:
        demo.training_active = False
        print("\n\n✅ Demo stopped")

if __name__ == "__main__":
    start_demo()