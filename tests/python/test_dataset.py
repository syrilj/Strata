import pytest
from dtruntime import DatasetRegistry

def test_register_dataset(coordinator_server, temp_dataset_dir):
    registry = DatasetRegistry(coordinator_server)
    registry.register("test-dataset-py", temp_dataset_dir, "parquet")

def test_get_shard(coordinator_server, temp_dataset_dir):
    registry = DatasetRegistry(coordinator_server)
    registry.register("test-dataset-py-2", temp_dataset_dir, "parquet")
    
    # We need a registered worker for get_shard to work?
    # DatasetRegistry interacts with coordinator. The coordinator requires "worker_id" to assign shards.
    # The `get_shard` method in Python binding takes (dataset_name, worker_id, epoch).
    # But for it to return a shard, the dataset must be registered.
    # Does the worker need to be registered first? 
    # Logic in coordinator: get_data_shard doesn't explicitly check if worker is registered for *shard assignment* 
    # but strictly speaking usually it should.
    # Let's assume it works without explicit worker registration if the logic permits, 
    # or we might need to register a worker via gRPC client or via Python if exposed.
    # Currently Python bindings don't expose Worker registration (it's in `TrainingOrchestrator` implicitly?).
    # `TrainingOrchestrator` calls `register_worker`.
    
    shard = registry.get_shard("test-dataset-py-2", worker_rank=0, epoch=0)
    assert shard is not None
    assert len(shard) > 0 # Should have file paths
