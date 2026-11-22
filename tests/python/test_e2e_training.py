"""
End-to-end training simulation tests for Python bindings.
"""

import os
import pytest

from dtruntime import (
    BarrierResult,
    CheckpointManager,
    CoordinatorCheckpointInfo,
    CoordinatorShardInfo,
    DatasetRegistry,
    TrainingOrchestrator,
)


class TestTrainingOrchestrator:
    def test_orchestrator_creation_and_url_normalization(self, coordinator_server):
        orch = TrainingOrchestrator(coordinator_server, worker_id="w-norm", world_size=2, rank=0)
        assert orch.coordinator_url == coordinator_server
        assert orch.worker_id == "w-norm"
        assert orch.world_size == 2
        assert orch.rank == 0

        orch_no_scheme = TrainingOrchestrator("127.0.0.1:50051")
        assert orch_no_scheme.coordinator_url == "http://127.0.0.1:50051"

    def test_orchestrator_invalid_url_connect_fails(self):
        orch = TrainingOrchestrator("http://127.0.0.1:1")
        with pytest.raises(Exception):
            orch.connect()


class TestCheckpointManager:
    def test_checkpoint_roundtrip(self, temp_checkpoint_dir):
        mgr = CheckpointManager(temp_checkpoint_dir)
        original_data = b"model_weights_" + bytes(range(256))

        checkpoint_id = mgr.save(original_data, step=1000, epoch=0)
        mgr.wait_pending()

        assert mgr.load(checkpoint_id) == original_data
        assert mgr.load(1000) == original_data

    def test_checkpoint_multiple_steps(self, temp_checkpoint_dir):
        mgr = CheckpointManager(temp_checkpoint_dir)
        steps = [100, 200, 300, 400, 500]

        for step in steps:
            mgr.save(f"checkpoint_data_step_{step}".encode(), step=step, epoch=0)

        mgr.wait_pending()

        assert mgr.list_checkpoints() == steps
        all_checkpoints = mgr.all_checkpoints()
        assert len(all_checkpoints) == len(steps)

        for step in steps:
            info = mgr.get_by_step(step)
            assert info is not None
            assert info.step == step

    def test_checkpoint_large_data(self, temp_checkpoint_dir):
        mgr = CheckpointManager(temp_checkpoint_dir)
        large_data = bytes(range(256)) * 4096

        checkpoint_id = mgr.save(large_data, step=9999, epoch=0)
        mgr.wait_pending()

        loaded = mgr.load(checkpoint_id)
        assert loaded == large_data
        assert len(loaded) == len(large_data)


class TestDatasetRegistry:
    def test_register_dataset_local(self):
        registry = DatasetRegistry()
        registry.register_worker("test-worker")
        registry.register_dataset(
            dataset_id="imagenet-test",
            total_samples=1000,
            shard_size=100,
            shuffle=True,
            seed=42,
        )
        shards = registry.get_shards("imagenet-test", "test-worker", epoch=0)
        assert len(shards) == 10

    def test_register_multiple_datasets_coordinator(self, coordinator_server, temp_dataset_dir):
        registry = DatasetRegistry(coordinator_server)
        for name, samples, shard_size in [
            ("dataset-a", 10000, 1000),
            ("dataset-b", 50000, 5000),
            ("dataset-c", 100000, 10000),
        ]:
            registry.register_dataset(
                dataset_id=name,
                total_samples=samples,
                shard_size=shard_size,
                shuffle=False,
                seed=42,
                path=temp_dataset_dir,
            )


class TestIntegration:
    def test_training_loop_e2e_with_coordinator(
        self, coordinator_server, temp_checkpoint_dir, temp_dataset_dir
    ):
        orch = TrainingOrchestrator(coordinator_server, worker_id="e2e-worker-0", world_size=1, rank=0)
        ckpt_mgr = CheckpointManager(temp_checkpoint_dir)

        cfg = orch.register_worker(hostname="127.0.0.1", port=50055, gpu_count=1)
        assert cfg.worker_id == "e2e-worker-0"

        try:
            assert orch.register_dataset(
                dataset_id="training-data",
                path=temp_dataset_dir,
                total_samples=10000,
                shard_size=1000,
                format="parquet",
                shuffle=True,
                seed=42,
            )

            shard = orch.get_shard("training-data", epoch=0)
            assert isinstance(shard, CoordinatorShardInfo)
            assert shard.total_shards == 10

            for step in range(1, 41):
                if step % 10 == 0:
                    payload = f"step={step},loss={1.0 - step / 50:.4f}".encode()
                    ckpt_id = ckpt_mgr.save(payload, step=step, epoch=0)
                    ckpt_mgr.wait_pending()
                    info = ckpt_mgr.get_by_step(step)
                    assert info is not None
                    assert orch.notify_checkpoint(
                        checkpoint_id=ckpt_id,
                        step=step,
                        epoch=0,
                        storage_path=info.path,
                        size_bytes=len(payload),
                    )
                    assert orch.heartbeat(current_step=step, current_epoch=0, state="training")

            latest = orch.get_latest_checkpoint()
            assert isinstance(latest, CoordinatorCheckpointInfo)
            assert latest.step == 40

            barrier = orch.barrier("e2e-done", step=40)
            assert isinstance(barrier, BarrierResult)
            assert barrier.released is True
        finally:
            orch.deregister()


class TestErrorHandling:
    def test_invalid_checkpoint_path(self):
        mgr = CheckpointManager("/tmp/nonexistent_" + os.urandom(8).hex())
        assert mgr is not None

    def test_load_nonexistent_checkpoint(self, temp_checkpoint_dir):
        mgr = CheckpointManager(temp_checkpoint_dir)
        with pytest.raises(Exception):
            mgr.load("nonexistent-checkpoint-id")

    def test_zero_shard_size_rejected(self):
        registry = DatasetRegistry()
        with pytest.raises(ValueError):
            registry.register_dataset("bad-ds", total_samples=100, shard_size=0)


class TestPerformance:
    def test_concurrent_checkpoints(self, temp_checkpoint_dir):
        mgr = CheckpointManager(temp_checkpoint_dir, keep_count=5)

        for i in range(10):
            mgr.save(f"concurrent_checkpoint_{i}".encode(), step=i * 100, epoch=0)

        mgr.wait_pending()
        assert len(mgr.all_checkpoints()) >= 5

        for i in range(5, 10):
            info = mgr.get_by_step(i * 100)
            assert info is not None
