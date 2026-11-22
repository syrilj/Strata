from dtruntime import DatasetRegistry


def test_local_dataset_registry(temp_dataset_dir):
    registry = DatasetRegistry()
    registry.register_worker("worker-0")
    registry.register_worker("worker-1")

    registry.register(
        "local-ds",
        temp_dataset_dir,
        format="parquet",
        total_samples=1000,
        shard_size=100,
        shuffle=True,
        seed=42,
    )

    shards = registry.get_shards("local-ds", "worker-0", epoch=0)
    assert len(shards) > 0
    assert shards[0].dataset_id == "local-ds"
    assert shards[0].num_samples > 0

    shard = registry.get_shard("local-ds", worker_id="worker-1", epoch=0)
    assert shard.dataset_id == "local-ds"
    assert shard.end_index > shard.start_index


def test_register_dataset_coordinator(coordinator_server, temp_dataset_dir):
    registry = DatasetRegistry(coordinator_server)
    registry.register(
        "test-dataset-py",
        temp_dataset_dir,
        format="parquet",
        total_samples=5000,
        shard_size=500,
    )


def test_get_shard_coordinator(coordinator_server, temp_dataset_dir):
    registry = DatasetRegistry(coordinator_server)
    registry.register(
        "test-dataset-py-2",
        temp_dataset_dir,
        format="parquet",
        total_samples=5000,
        shard_size=500,
    )

    try:
        shard = registry.get_shard("test-dataset-py-2", worker_rank=0, epoch=0)
        assert shard is not None
        assert shard.dataset_id == "test-dataset-py-2"
        assert shard.total_shards == 10
        assert shard.end_index > shard.start_index
        assert len(shard.file_paths) > 0
    finally:
        registry.remove_worker("worker-0")
