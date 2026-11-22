#!/usr/bin/env python3
"""
Simple real distributed training example with synthetic data, using
dtruntime.TrainingOrchestrator and dtruntime.CheckpointManager (with pure Python /
numpy fallback when PyTorch is not installed).
"""

import argparse
import asyncio
import json
import math
import random
from datetime import datetime
from pathlib import Path

from dtruntime import CheckpointManager, TrainingOrchestrator

try:
    import torch
    import torch.nn as nn
    import torch.optim as optim

    HAS_TORCH = True
except ImportError:
    HAS_TORCH = False

try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


if HAS_TORCH:

    class SimpleNN(nn.Module):
        def __init__(self, input_size=64, hidden_size=32, num_classes=4):
            super().__init__()
            self.fc1 = nn.Linear(input_size, hidden_size)
            self.fc2 = nn.Linear(hidden_size, num_classes)
            self.relu = nn.ReLU()

        def forward(self, x):
            return self.fc2(self.relu(self.fc1(x)))


class RealTrainingWorker:
    def __init__(
        self,
        worker_id: str,
        coordinator_url: str,
        checkpoint_dir: str = "./checkpoints",
    ):
        self.worker_id = worker_id
        self.coordinator_url = coordinator_url
        self.orchestrator = TrainingOrchestrator(
            coordinator_url=coordinator_url,
            worker_id=worker_id,
        )
        self.ckpt_dir = Path(checkpoint_dir)
        self.ckpt_dir.mkdir(parents=True, exist_ok=True)
        self.checkpoint_manager = CheckpointManager(str(self.ckpt_dir), keep_count=5)

        self.current_epoch = 0
        self.current_step = 0
        self.training_loss = []
        self.model = None
        self.optimizer = None
        self.criterion = None
        self.train_loader = None
        self.test_loader = None
        self.fallback_data = None
        self.fallback_weights = None

    def connect(self):
        cfg = self.orchestrator.register_worker(
            worker_id=self.worker_id,
            hostname="127.0.0.1",
            port=50052,
        )
        print(f"[{self.worker_id}] Connected as rank {cfg.rank}/{cfg.world_size}")
        return cfg

    def generate_synthetic_dataset(self, num_samples: int = 800):
        self.orchestrator.register_dataset(
            dataset_id="synthetic-classification",
            path="./data",
            total_samples=num_samples,
            shard_size=200,
            format="synthetic",
            shuffle=True,
            seed=42,
        )
        shard = self.orchestrator.get_shard("synthetic-classification", epoch=0)
        shard_samples = max(32, shard.end_index - shard.start_index)

        if HAS_TORCH:
            x = torch.randn(shard_samples, 64)
            y = torch.randint(0, 4, (shard_samples,))
            for c in range(4):
                x[y == c] += (c - 1.5) * 0.6
            split = int(0.8 * shard_samples)
            self.train_loader = torch.utils.data.DataLoader(
                torch.utils.data.TensorDataset(x[:split], y[:split]),
                batch_size=32,
                shuffle=True,
            )
            self.test_loader = torch.utils.data.DataLoader(
                torch.utils.data.TensorDataset(x[split:], y[split:]),
                batch_size=32,
                shuffle=False,
            )
            self.model = SimpleNN()
            self.optimizer = optim.Adam(self.model.parameters(), lr=0.01)
            self.criterion = nn.CrossEntropyLoss()
        elif HAS_NUMPY:
            rng = np.random.default_rng(42)
            x = rng.normal(0.0, 1.0, size=(shard_samples, 16))
            w_true = np.linspace(-1.0, 1.0, 16)
            y = (x @ w_true > 0).astype(np.float64)
            self.fallback_data = (x, y)
            self.fallback_weights = np.zeros(16, dtype=np.float64)
        else:
            rng = random.Random(42)
            x = [[rng.gauss(0.0, 1.0) for _ in range(16)] for _ in range(shard_samples)]
            w_true = [(-1.0 + 2.0 * i / 15.0) for i in range(16)]
            y = [1.0 if sum(a * b for a, b in zip(r, w_true)) > 0 else 0.0 for r in x]
            self.fallback_data = (x, y)
            self.fallback_weights = [0.0] * 16

    async def save_checkpoint(self, accuracy: float, loss: float):
        if HAS_TORCH and self.model is not None:
            import io

            buf = io.BytesIO()
            torch.save(
                {
                    "epoch": self.current_epoch,
                    "step": self.current_step,
                    "model_state_dict": self.model.state_dict(),
                    "accuracy": accuracy,
                    "loss": loss,
                },
                buf,
            )
            payload = buf.getvalue()
        else:
            weights = (
                self.fallback_weights.tolist()
                if HAS_NUMPY and isinstance(self.fallback_weights, np.ndarray)
                else list(self.fallback_weights)
            )
            payload = json.dumps(
                {
                    "epoch": self.current_epoch,
                    "step": self.current_step,
                    "accuracy": accuracy,
                    "loss": loss,
                    "weights": weights,
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
        path = info.path if info is not None else str(self.ckpt_dir / f"{ckpt_id}.ckpt")
        self.orchestrator.notify_checkpoint(
            checkpoint_id=ckpt_id,
            step=self.current_step,
            epoch=self.current_epoch,
            storage_path=path,
            size_bytes=len(payload),
            checkpoint_type="full",
        )
        print(f"[{self.worker_id}] Checkpoint {ckpt_id} saved ({len(payload)} bytes)")

    async def train_epoch(self):
        if HAS_TORCH:
            self.model.train()
            total_loss = 0.0
            for data, target in self.train_loader:
                self.optimizer.zero_grad()
                loss = self.criterion(self.model(data), target)
                loss.backward()
                self.optimizer.step()
                total_loss += loss.item()
                self.current_step += 1
            avg_loss = total_loss / max(1, len(self.train_loader))
            self.model.eval()
            correct, total = 0, 0
            with torch.no_grad():
                for data, target in self.test_loader:
                    pred = self.model(data).argmax(dim=1)
                    correct += (pred == target).sum().item()
                    total += target.size(0)
            accuracy = 100.0 * correct / max(1, total)
        elif HAS_NUMPY:
            x, y = self.fallback_data
            logits = np.clip(x @ self.fallback_weights, -20.0, 20.0)
            probs = 1.0 / (1.0 + np.exp(-logits))
            grad = (x.T @ (probs - y)) / len(y)
            self.fallback_weights -= 0.1 * grad
            self.current_step += len(y) // 32
            avg_loss = float(
                -np.mean(y * np.log(probs + 1e-8) + (1.0 - y) * np.log(1.0 - probs + 1e-8))
            )
            accuracy = float(((x @ self.fallback_weights > 0) == y).mean() * 100.0)
        else:
            x, y = self.fallback_data
            total_loss = 0.0
            correct = 0
            for row, label in zip(x, y):
                z = max(-20.0, min(20.0, sum(a * b for a, b in zip(row, self.fallback_weights))))
                prob = 1.0 / (1.0 + math.exp(-z))
                total_loss += -(label * math.log(prob + 1e-8) + (1.0 - label) * math.log(1.0 - prob + 1e-8))
                err = prob - label
                for j in range(len(self.fallback_weights)):
                    self.fallback_weights[j] -= 0.05 * err * row[j]
                if (prob >= 0.5) == (label >= 0.5):
                    correct += 1
                self.current_step += 1
            avg_loss = total_loss / max(1, len(y))
            accuracy = 100.0 * correct / max(1, len(y))

        self.current_epoch += 1
        self.training_loss.append(avg_loss)
        await self.orchestrator.heartbeat_async(
            current_step=self.current_step,
            current_epoch=self.current_epoch,
            state="training",
            current_task=f"epoch_{self.current_epoch}",
        )
        await self.save_checkpoint(accuracy, avg_loss)
        return accuracy, avg_loss

    async def start_real_training(self, num_epochs: int = 3):
        try:
            self.connect()
            self.generate_synthetic_dataset()
            for epoch in range(num_epochs):
                acc, loss = await self.train_epoch()
                print(
                    f"[{self.worker_id}] Epoch {epoch + 1}/{num_epochs}: "
                    f"loss={loss:.4f}, acc={acc:.2f}%"
                )
                await self.orchestrator.wait_barrier_async(
                    f"simple-epoch-{epoch}", step=self.current_step
                )
            await self.orchestrator.heartbeat_async(
                current_step=self.current_step,
                current_epoch=self.current_epoch,
                state="idle",
                current_task="complete",
            )
        finally:
            try:
                self.orchestrator.deregister()
            except Exception:
                pass


async def main():
    parser = argparse.ArgumentParser(description="Real Distributed Training Simple Demo")
    parser.add_argument("--worker-id", default="real-worker-01", help="Worker ID")
    parser.add_argument("--coordinator", default="http://localhost:50051", help="Coordinator URL")
    parser.add_argument("--epochs", type=int, default=3, help="Number of epochs")
    parser.add_argument("--checkpoint-dir", default="./checkpoints", help="Checkpoint directory")
    args = parser.parse_args()

    worker = RealTrainingWorker(args.worker_id, args.coordinator, args.checkpoint_dir)
    await worker.start_real_training(args.epochs)


if __name__ == "__main__":
    asyncio.run(main())