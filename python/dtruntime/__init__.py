"""
Distributed Training Runtime - High-performance data and checkpoint coordination

This package provides Python bindings for the Rust distributed training runtime,
enabling efficient coordination of data loading, checkpointing, and synchronization
across distributed training workers.

Example:
    >>> from dtruntime import DatasetRegistry, CheckpointManager, TrainingOrchestrator
    >>> 
    >>> # Local mode: direct shard management
    >>> registry = DatasetRegistry()
    >>> registry.register_worker("worker-0")
    >>> registry.register_dataset("imagenet", total_samples=1281167, shard_size=10000)
    >>> shards = registry.get_shards("imagenet", "worker-0", epoch=0)
    >>> 
    >>> # Distributed mode: connect to coordinator
    >>> orch = TrainingOrchestrator("http://localhost:50051")
    >>> config = orch.register_worker("worker-0", "localhost", 50052, gpu_count=8)
    >>> print(f"Rank {config.rank} of {config.world_size}")
"""

from ._core import (
    # Dataset sharding
    DatasetRegistry,
    ShardInfo,
    # Checkpoint management  
    CheckpointManager,
    CheckpointInfo,
    # Distributed orchestration
    TrainingOrchestrator,
    WorkerConfig,
)

__all__ = [
    # Dataset sharding
    "DatasetRegistry",
    "ShardInfo",
    # Checkpoint management
    "CheckpointManager", 
    "CheckpointInfo",
    # Distributed orchestration
    "TrainingOrchestrator",
    "WorkerConfig",
]

__version__ = "0.1.0"
