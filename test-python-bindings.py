#!/usr/bin/env python3
"""
Quick test to verify Python bindings work correctly
"""

import asyncio
import sys
import tempfile


def test_imports():
    """Test that all modules and classes can be imported"""
    print("Testing imports...")
    try:
        from dtruntime import (
            BarrierResult,
            CheckpointInfo,
            CheckpointManager,
            CoordinatorCheckpointInfo,
            CoordinatorShardInfo,
            DatasetRegistry,
            ShardInfo,
            TrainingOrchestrator,
            WorkerConfig,
            coordinator_pb2,
            coordinator_pb2_grpc,
        )

        assert coordinator_pb2 is not None
        assert coordinator_pb2_grpc is not None
        assert all(
            cls is not None
            for cls in (
                BarrierResult,
                CheckpointInfo,
                CheckpointManager,
                CoordinatorCheckpointInfo,
                CoordinatorShardInfo,
                DatasetRegistry,
                ShardInfo,
                TrainingOrchestrator,
                WorkerConfig,
            )
        )
        print("All imports successful")
        return True
    except ImportError as e:
        print(f"Import failed: {e}")
        return False


def test_checkpoint_manager():
    """Test CheckpointManager sync + async roundtrip"""
    print("\nTesting CheckpointManager...")
    try:
        from dtruntime import CheckpointManager

        with tempfile.TemporaryDirectory() as tmpdir:
            mgr = CheckpointManager(tmpdir)
            ckpt_id = mgr.save(b"sync-bytes", step=10, epoch=0)
            mgr.wait_pending()
            assert mgr.load(ckpt_id) == b"sync-bytes"
            assert mgr.load(10) == b"sync-bytes"
            assert mgr.list_checkpoints() == [10]

            async def _run_async():
                id2 = await mgr.save_async(b"async-bytes", step=20, epoch=1)
                assert await mgr.load_async(id2) == b"async-bytes"
                assert await mgr.load_async(20) == b"async-bytes"

            asyncio.run(_run_async())
            assert mgr.list_checkpoints() == [10, 20]
            assert mgr.delete(ckpt_id) is True
            assert mgr.list_checkpoints() == [20]
            print("CheckpointManager sync & async tests passed")
            return True
    except Exception as e:
        print(f"CheckpointManager test failed: {e}")
        return False


def test_dataset_registry():
    """Test DatasetRegistry local and coordinator-configured creation"""
    print("\nTesting DatasetRegistry...")
    try:
        from dtruntime import DatasetRegistry

        local_reg = DatasetRegistry()
        local_reg.register_worker("worker-0")
        local_reg.register("ds", "/tmp/data", total_samples=1000, shard_size=250)
        shard = local_reg.get_shard("ds", worker_id="worker-0", epoch=0)
        assert shard.total_shards == 4

        dist_reg = DatasetRegistry("http://localhost:50051")
        assert dist_reg.coordinator_url == "http://localhost:50051"
        print("DatasetRegistry tests passed")
        return True
    except Exception as e:
        print(f"DatasetRegistry test failed: {e}")
        return False


def test_orchestrator():
    """Test TrainingOrchestrator creation and URL normalization"""
    print("\nTesting TrainingOrchestrator...")
    try:
        from dtruntime import TrainingOrchestrator

        orch = TrainingOrchestrator(
            worker_id="test-worker",
            coordinator_url="localhost:50051",
            world_size=1,
            rank=0,
        )
        assert orch.coordinator_url == "http://localhost:50051"
        assert orch.worker_id == "test-worker"
        assert orch.world_size == 1
        assert orch.rank == 0
        print("TrainingOrchestrator tests passed")
        return True
    except Exception as e:
        print(f"TrainingOrchestrator test failed: {e}")
        return False


def main():
    print("=" * 60)
    print("  Python Bindings Quick Test")
    print("=" * 60)

    results = [
        ("Imports", test_imports()),
        ("CheckpointManager", test_checkpoint_manager()),
        ("DatasetRegistry", test_dataset_registry()),
        ("TrainingOrchestrator", test_orchestrator()),
    ]

    print("\n" + "=" * 60)
    print("  RESULTS")
    print("=" * 60)

    for name, passed in results:
        status = "PASS" if passed else "FAIL"
        print(f"{status:10} {name}")

    if all(r[1] for r in results):
        print("\nAll Python binding tests passed!")
        return 0
    print("\nSome tests failed.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
