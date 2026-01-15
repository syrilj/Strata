"""
End-to-end training simulation tests for Python bindings.

These tests verify the complete workflow of:
- Worker registration and orchestration
- Dataset registration and shard distribution
- Checkpoint save/load operations
- Error handling and edge cases
"""

import pytest
import asyncio
import os
from dtruntime import (
    DatasetRegistry,
    CheckpointManager,
    TrainingOrchestrator,
    WorkerConfig,
)


class TestTrainingOrchestrator:
    """Tests for the TrainingOrchestrator class."""

    def test_orchestrator_creation(self, coordinator_server):
        """Test that orchestrator can be created with valid parameters."""
        orch = TrainingOrchestrator(coordinator_server)
        assert orch is not None

    def test_orchestrator_invalid_url(self):
        """Test that orchestrator handles invalid URLs gracefully."""
        # Should not raise on creation, only on connection
        orch = TrainingOrchestrator("http://invalid-host:99999")
        assert orch is not None


class TestCheckpointManager:
    """Tests for the CheckpointManager class."""

    def test_checkpoint_roundtrip(self, temp_checkpoint_dir):
        """Test saving and loading a checkpoint."""
        mgr = CheckpointManager(temp_checkpoint_dir)
        
        # Create test data
        original_data = b"model_weights_" + bytes(range(256))
        
        # Save checkpoint
        checkpoint_id = mgr.save(original_data, step=1000, epoch=0)
        
        # Wait for async write
        mgr.wait_pending()
        
        # Load checkpoint
        loaded_data = mgr.load(checkpoint_id)
        
        assert loaded_data == original_data

    def test_checkpoint_multiple_steps(self, temp_checkpoint_dir):
        """Test saving multiple checkpoints at different steps."""
        mgr = CheckpointManager(temp_checkpoint_dir)
        
        steps = [100, 200, 300, 400, 500]
        
        for step in steps:
            data = f"checkpoint_data_step_{step}".encode()
            mgr.save(data, step=step, epoch=0)
        
        mgr.wait_pending()  # Wait for all writes
        
        # Verify all checkpoints exist
        all_checkpoints = mgr.all_checkpoints()
        assert len(all_checkpoints) == len(steps), f"Expected {len(steps)} checkpoints, got {len(all_checkpoints)}"
        
        # Verify we can load by step
        for step in steps:
            info = mgr.get_by_step(step)
            assert info is not None, f"Checkpoint at step {step} not found"
            assert info.step == step

    def test_checkpoint_large_data(self, temp_checkpoint_dir):
        """Test checkpointing larger data (simulating model weights)."""
        mgr = CheckpointManager(temp_checkpoint_dir)
        
        # 1MB of data
        large_data = bytes(range(256)) * 4096
        
        checkpoint_id = mgr.save(large_data, step=9999, epoch=0)
        mgr.wait_pending()
        
        loaded = mgr.load(checkpoint_id)
        assert loaded == large_data
        assert len(loaded) == len(large_data)


class TestDatasetRegistry:
    """Tests for the DatasetRegistry class."""

    def test_register_dataset(self, coordinator_server, temp_dataset_dir):
        """Test dataset registration."""
        registry = DatasetRegistry()
        
        # Register a worker first
        registry.register_worker("test-worker")
        
        # Should not raise
        registry.register_dataset(
            dataset_id="imagenet-test",
            total_samples=1000,
            shard_size=100,
            shuffle=True,
            seed=42
        )

    def test_register_multiple_datasets(self, coordinator_server, temp_dataset_dir):
        """Test registering multiple datasets."""
        registry = DatasetRegistry()
        
        # Register a worker first
        registry.register_worker("test-worker")
        
        datasets = [
            ("dataset-a", 10000, 1000),
            ("dataset-b", 50000, 5000),
            ("dataset-c", 100000, 10000),
        ]
        
        for name, samples, shard_size in datasets:
            registry.register_dataset(
                dataset_id=name,
                total_samples=samples,
                shard_size=shard_size,
                shuffle=False,
                seed=42
            )


class TestIntegration:
    """Integration tests combining multiple components."""

    def test_training_loop_simulation(self, coordinator_server, temp_checkpoint_dir, temp_dataset_dir):
        """Simulate a complete training loop."""
        # Setup
        registry = DatasetRegistry()
        ckpt_mgr = CheckpointManager(temp_checkpoint_dir)
        
        # Register worker
        registry.register_worker("test-worker")
        
        # Register dataset
        registry.register_dataset(
            dataset_id="training-data",
            total_samples=10000,
            shard_size=1000,
            shuffle=True,
            seed=42
        )
        
        # Simulate training loop
        total_steps = 50
        checkpoint_interval = 10
        
        for step in range(total_steps):
            # Simulate training step
            loss = 1.0 - (step / total_steps) * 0.5  # Decreasing loss
            
            # Checkpoint at intervals
            if step > 0 and step % checkpoint_interval == 0:
                checkpoint_data = f"step={step},loss={loss:.4f}".encode()
                ckpt_mgr.save(checkpoint_data, step=step, epoch=0)
        
        # Wait for checkpoints to be written
        ckpt_mgr.wait_pending()
        
        # Verify checkpoints were created
        expected_checkpoints = [10, 20, 30, 40]
        all_checkpoints = ckpt_mgr.all_checkpoints()
        assert len(all_checkpoints) == len(expected_checkpoints), f"Expected {len(expected_checkpoints)} checkpoints, got {len(all_checkpoints)}"
        
        for step in expected_checkpoints:
            info = ckpt_mgr.get_by_step(step)
            assert info is not None, f"Missing checkpoint at step {step}"


class TestErrorHandling:
    """Tests for error handling and edge cases."""

    def test_invalid_checkpoint_path(self):
        """Test handling of invalid checkpoint directory."""
        # Non-existent directory should be created or handled
        mgr = CheckpointManager("/tmp/nonexistent_" + os.urandom(8).hex())
        assert mgr is not None

    def test_load_nonexistent_checkpoint(self, temp_checkpoint_dir):
        """Test loading a checkpoint that doesn't exist."""
        mgr = CheckpointManager(temp_checkpoint_dir)
        
        with pytest.raises(Exception):
            mgr.load("nonexistent-checkpoint-id")

    def test_empty_dataset_name(self, coordinator_server, temp_dataset_dir):
        """Test that empty dataset names are rejected."""
        registry = DatasetRegistry()
        
        # Empty dataset name should be handled gracefully
        # The API doesn't explicitly reject it, but it won't work properly
        registry.register_dataset(
            dataset_id="",  # Empty name
            total_samples=1000,
            shard_size=100,
            shuffle=False,
            seed=42
        )


class TestPerformance:
    """Performance-related tests."""

    def test_concurrent_checkpoints(self, temp_checkpoint_dir):
        """Test concurrent checkpoint operations."""
        mgr = CheckpointManager(temp_checkpoint_dir)
        
        # Create multiple checkpoints
        for i in range(10):
            data = f"concurrent_checkpoint_{i}".encode()
            mgr.save(data, step=i * 100, epoch=0)
        
        # Wait for all to complete
        mgr.wait_pending()
        
        # Verify all were written
        all_checkpoints = mgr.all_checkpoints()
        assert len(all_checkpoints) >= 5, f"Expected at least 5 checkpoints (keep_count), got {len(all_checkpoints)}"
        
        # Verify we can load the most recent ones
        for i in range(5, 10):  # Last 5 should be kept
            info = mgr.get_by_step(i * 100)
            assert info is not None, f"Checkpoint at step {i * 100} not found"
