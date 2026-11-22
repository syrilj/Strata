#!/usr/bin/env python3
"""
Real worker implementation that connects to the coordinator via
dtruntime.TrainingOrchestrator and persists checkpoints with
dtruntime.CheckpointManager.
"""

import argparse
import asyncio
import json
from datetime import datetime
from pathlib import Path

from dtruntime import CheckpointManager, TrainingOrchestrator


class RealWorker:
    def __init__(
        self,
        worker_id: str,
        coordinator_url: str,
        gpu_count: int = 0,
        port: int = 50052,
        checkpoint_dir: str = "/tmp/strata-checkpoints",
    ):
        self.worker_id = worker_id
        self.coordinator_url = coordinator_url
        self.gpu_count = gpu_count
        self.port = port
        self.current_epoch = 0
        self.current_step = 0
        self.is_training = False

        self.orchestrator = TrainingOrchestrator(
            coordinator_url=coordinator_url,
            worker_id=worker_id,
        )
        ckpt_path = Path(checkpoint_dir) / worker_id
        ckpt_path.mkdir(parents=True, exist_ok=True)
        self.checkpoint_manager = CheckpointManager(str(ckpt_path), keep_count=5)

    async def register_with_coordinator(self):
        """Register this worker with the coordinator via gRPC."""
        config = await self.orchestrator.register_worker_async(
            worker_id=self.worker_id,
            hostname="127.0.0.1",
            port=self.port,
            gpu_count=self.gpu_count,
        )
        print(
            f"[{self.worker_id}] Registered as rank {config.rank}/{config.world_size} "
            f"(heartbeat interval {config.heartbeat_interval_ms} ms)"
        )
        return config

    async def start_training_loop(self, max_steps: int = 0, step_delay: float = 0.5):
        """Run training steps, heartbeats, and checkpoints."""
        self.is_training = True
        try:
            while self.is_training:
                await self.training_step()
                await self.send_heartbeat()
                if max_steps > 0 and self.current_step >= max_steps:
                    break
                await asyncio.sleep(step_delay)
        finally:
            await self.shutdown()

    async def training_step(self):
        self.current_step += 1
        phases = ["data_loading", "forward_pass", "backward_pass", "gradient_sync"]
        phase = phases[self.current_step % len(phases)]
        print(f"[{self.worker_id}] Step {self.current_step}: {phase}")

        if self.current_step % 10 == 0:
            self.current_epoch += 1
            await self.create_checkpoint()

    async def create_checkpoint(self):
        payload = json.dumps(
            {
                "worker_id": self.worker_id,
                "epoch": self.current_epoch,
                "step": self.current_step,
                "timestamp": datetime.now().isoformat(),
            }
        ).encode("utf-8")

        ckpt_id = await self.checkpoint_manager.save_async(
            payload,
            step=self.current_step,
            epoch=self.current_epoch,
            metadata={"worker_id": self.worker_id},
        )
        info = self.checkpoint_manager.get_by_step(self.current_step)
        storage_path = info.path if info is not None else f"/tmp/{ckpt_id}.ckpt"
        self.orchestrator.notify_checkpoint(
            checkpoint_id=ckpt_id,
            step=self.current_step,
            epoch=self.current_epoch,
            storage_path=storage_path,
            size_bytes=len(payload),
            checkpoint_type="full",
        )
        print(f"[{self.worker_id}] Checkpoint saved & notified: {ckpt_id}")

    async def send_heartbeat(self):
        ack = await self.orchestrator.heartbeat_async(
            current_step=self.current_step,
            current_epoch=self.current_epoch,
            state="training",
            current_task=f"step_{self.current_step}",
        )
        if not ack:
            print(f"[{self.worker_id}] Warning: heartbeat not acknowledged")

    async def shutdown(self):
        if not self.is_training:
            return
        self.is_training = False
        try:
            await self.orchestrator.heartbeat_async(
                current_step=self.current_step,
                current_epoch=self.current_epoch,
                state="idle",
                current_task="shutdown",
            )
        except Exception:
            pass
        try:
            self.orchestrator.deregister()
        except Exception:
            pass
        print(f"[{self.worker_id}] Shutdown complete")


async def main():
    parser = argparse.ArgumentParser(description="Real distributed training worker")
    parser.add_argument("--worker-id", required=True, help="Unique worker ID")
    parser.add_argument("--coordinator", default="http://localhost:50051", help="Coordinator URL")
    parser.add_argument("--gpu-count", type=int, default=0, help="Number of GPUs")
    parser.add_argument("--port", type=int, default=50052, help="Worker port")
    parser.add_argument("--max-steps", type=int, default=0, help="Max steps (0 = run forever)")
    parser.add_argument("--step-delay", type=float, default=0.5, help="Delay between steps in seconds")
    args = parser.parse_args()

    worker = RealWorker(
        worker_id=args.worker_id,
        coordinator_url=args.coordinator,
        gpu_count=args.gpu_count,
        port=args.port,
    )

    try:
        await worker.register_with_coordinator()
        await worker.start_training_loop(max_steps=args.max_steps, step_delay=args.step_delay)
    except KeyboardInterrupt:
        await worker.shutdown()


if __name__ == "__main__":
    asyncio.run(main())