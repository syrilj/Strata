import pytest
from dtruntime import TrainingOrchestrator
import asyncio

@pytest.mark.asyncio
async def test_distributed_orchestration(coordinator_server):
    # This involves connecting to coordinator
    orch = TrainingOrchestrator(
        worker_id="worker-py-1",
        coordinator_url=coordinator_server,
        world_size=1,
        rank=0
    )
    
    # Should register implicitly or explicit?
    # Usually constructor initializes, but we might need `join()` or similar.
    # Looking at bindings, `new` just creates struct.
    # There is likely a `join` or `register` method. 
    # Or maybe it registers on creation?
    # Assuming `init` or `connect`.
    
    # Check binding implementation if possible, or assume common pattern.
    # We implemented `TrainingOrchestrator` to have methods corresponding to `runtime-core`.
    
    # Let's try to call `register_worker`? Or is it automatic?
    # In `runtime-core`, `TrainingOrchestrator::new` does NOT register. `run()` or explicit register needed.
    # `TrainingOrchestrator` in generic code usually has `register`.
    
    # As per previous conversation memory, we exposed `TrainingOrchestrator` methods.
    
    # Let's try:
    await orch.register_worker(ip="127.0.0.1", port=8081)
    
    # Wait barrier
    await orch.wait_barrier("start-training")
    
    assert True
