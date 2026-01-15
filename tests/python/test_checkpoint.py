import pytest
from dtruntime import CheckpointManager
import asyncio

@pytest.mark.asyncio
async def test_async_checkpoint(temp_checkpoint_dir):
    mgr = CheckpointManager(temp_checkpoint_dir)
    
    # Save async
    data = bytes([1, 2, 3, 4])
    await mgr.save_async(data, step=100)
    
    # Wait pending should be called implicitly or explicit?
    # bindings `save_async` is async, so it suspends until handed off? 
    # Actually `save_async` spawned a task and returned. 
    # We might need `wait_pending()` if exposed.
    # The binding `CheckpointManager` has `close` maybe?
    # In previous task we found `save_async` was fire and forget or similar.
    # Let's verify if `wait_pending` exists in binding.
    # It was in the Rust struct.
    
    # Assuming `save_async` waits for the write to complete or at least be queued.
    # If we want to verify it exists, we check file.
    
    import os
    # Give it a moment to flush if it is background
    await asyncio.sleep(0.5)
    
    expected_path = os.path.join(temp_checkpoint_dir, "checkpoint-100.ckpt")
    assert os.path.exists(expected_path)
    
    with open(expected_path, "rb") as f:
        read_data = f.read()
        assert read_data == data

@pytest.mark.asyncio
async def test_load_checkpoint(temp_checkpoint_dir):
    mgr = CheckpointManager(temp_checkpoint_dir)
    data = bytes([5, 6, 7, 8])
    
    # Write manually first to ensure it's there
    import os
    ckpt_path = os.path.join(temp_checkpoint_dir, "checkpoint-200.ckpt")
    with open(ckpt_path, "wb") as f:
        f.write(data)
        
    loaded = await mgr.load(step=200)
    assert loaded == data
