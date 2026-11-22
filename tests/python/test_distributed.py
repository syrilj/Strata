import asyncio
import pytest

from dtruntime import BarrierResult, CoordinatorShardInfo, TrainingOrchestrator, WorkerConfig


@pytest.mark.asyncio
async def test_distributed_orchestration(coordinator_server):
    orch = TrainingOrchestrator(
        worker_id="worker-py-1",
        coordinator_url=coordinator_server,
        world_size=1,
        rank=0,
    )

    config = await orch.register_worker_async(ip="127.0.0.1", port=8081)
    assert isinstance(config, WorkerConfig)
    assert config.worker_id == "worker-py-1"
    assert config.world_size >= 1

    ack = await orch.heartbeat_async(current_step=1, current_epoch=0, state="training")
    assert ack is True

    assert orch.register_dataset(
        "dist-ds",
        "/tmp/data",
        total_samples=2000,
        shard_size=500,
        format="parquet",
    ) is True

    shard = orch.get_shard("dist-ds", epoch=0)
    assert isinstance(shard, CoordinatorShardInfo)
    assert shard.dataset_id == "dist-ds"
    assert shard.total_shards == 4

    barrier = await orch.wait_barrier_async("start-training", step=1)
    assert isinstance(barrier, BarrierResult)
    assert barrier.released is True
    assert barrier.participants >= 1

    orch.deregister()


@pytest.mark.asyncio
async def test_multi_worker_barrier_and_concurrent_heartbeat(coordinator_server):
    w1 = TrainingOrchestrator(coordinator_server, worker_id="barrier-w1")
    w2 = TrainingOrchestrator(coordinator_server, worker_id="barrier-w2")

    await w1.register_worker_async(ip="127.0.0.1", port=9001)
    await w2.register_worker_async(ip="127.0.0.1", port=9002)

    try:
        # Start w1 waiting at barrier; verify heartbeat on w1 does not deadlock while barrier waits
        barrier_f1 = asyncio.ensure_future(w1.wait_barrier_async("multi-sync", step=5))
        await asyncio.sleep(0.05)

        hb_ack = await w1.heartbeat_async(current_step=5, current_epoch=1, state="checkpointing")
        assert hb_ack is True

        barrier_f2 = asyncio.ensure_future(w2.wait_barrier_async("multi-sync", step=5))
        r1, r2 = await asyncio.gather(barrier_f1, barrier_f2)

        assert r1.released is True
        assert r2.released is True
        assert {r1.arrival_order, r2.arrival_order} == {1, 2}
    finally:
        for w in (w1, w2):
            try:
                w.deregister()
            except Exception:
                pass
