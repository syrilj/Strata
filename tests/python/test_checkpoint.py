import pytest

from dtruntime import CheckpointManager


@pytest.mark.asyncio
async def test_async_checkpoint_and_load_by_step_and_id(temp_checkpoint_dir):
    mgr = CheckpointManager(temp_checkpoint_dir)
    data = bytes([1, 2, 3, 4, 5, 6])

    ckpt_id = await mgr.save_async(data, step=100, epoch=2, metadata={"tag": "async"})
    assert isinstance(ckpt_id, str)
    assert len(ckpt_id) > 0

    assert mgr.list_checkpoints() == [100]

    loaded_by_id = await mgr.load_async(ckpt_id)
    assert loaded_by_id == data

    loaded_by_step = await mgr.load_async(100)
    assert loaded_by_step == data


def test_sync_checkpoint_list_delete_and_reload(temp_checkpoint_dir):
    mgr = CheckpointManager(temp_checkpoint_dir, keep_count=5)

    data1 = b"weights-step-10"
    data2 = b"weights-step-20"
    id1 = mgr.save(data1, step=10, epoch=0)
    id2 = mgr.save(data2, step=20, epoch=1)
    mgr.wait_pending()

    assert mgr.list_checkpoints() == [10, 20]
    assert mgr.load(id1) == data1
    assert mgr.load(20) == data2
    assert mgr.latest().checkpoint_id == id2

    assert mgr.delete(id1) is True
    assert mgr.delete(id1) is False
    assert mgr.list_checkpoints() == [20]

    with pytest.raises(Exception):
        mgr.load(id1)
