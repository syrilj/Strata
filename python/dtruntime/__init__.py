"""
Distributed Training Runtime - High-performance data and checkpoint coordination

This package provides Python bindings for the Rust distributed training runtime,
enabling efficient coordination of data loading, checkpointing, and synchronization
across distributed training workers.
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
    CoordinatorShardInfo,
    BarrierResult,
    CoordinatorCheckpointInfo,
)

try:
    from . import coordinator_pb2, coordinator_pb2_grpc
except ImportError:
    coordinator_pb2 = None
    coordinator_pb2_grpc = None

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
    "CoordinatorShardInfo",
    "BarrierResult",
    "CoordinatorCheckpointInfo",
    # Generated gRPC stubs
    "coordinator_pb2",
    "coordinator_pb2_grpc",
]

__version__ = "0.1.0"
