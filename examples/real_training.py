#!/usr/bin/env python3
"""
Real distributed training example using CIFAR-10 (with PyTorch) or synthetic
dataset (pure Python / numpy fallback when torch is not installed), wired to
dtruntime.TrainingOrchestrator and dtruntime.CheckpointManager.
"""

import argparse
import asyncio
import json
import math
import random
import sys
from datetime import datetime
from pathlib import Path

from dtruntime import CheckpointManager, TrainingOrchestrator

try:
    import torch
    import torch.nn as nn
    import torch.optim as optim
    import torchvision
    import torchvision.transforms as transforms

    HAS_TORCH = True
except ImportError:
    HAS_TORCH = False

try:
    import numpy as np

    HAS_NUMPY = True
except ImportError:
    HAS_NUMPY = False


if HAS_TORCH:

    class SimpleCNN(nn.Module):
        """Simple CNN for CIFAR-10 classification."""

        def __init__(self):
            super().__init__()
            self.conv1 = nn.Conv2d(3, 32, 3, padding=1)
            self.conv2 = nn.Conv2d(32, 64, 3, padding=1)
            self.pool = nn.MaxPool2d(2, 2)
            self.fc1 = nn.Linear(64 * 8 * 8, 512)
            self.fc2 = nn.Linear(512, 10)
            self.relu = nn.ReLU()
            self.dropout = nn.Dropout(0.5)

        def forward(self, x):
            x = self.pool(self.relu(self.conv1(x)))
            x = self.pool(self.relu(self.conv2(x)))
            x = x.view(-1, 64 * 8 * 8)
            x = self.dropout(self.relu(self.fc1(x)))
            return self.fc2(x)


class RealDistributedTrainer:
    def __init__(
        self,
        worker_id: str,
        coordinator_url: str,
        rank: int = 0,
        world_size: int = 1,
        checkpoint_dir: str = "./checkpoints",
    ):
        self.worker_id = worker_id
        self.coordinator_url = coordinator_url
        self.rank = rank
        self.world_size = world_size
        self.current_epoch = 0
        self.current_step = 0
        self.best_accuracy = 0.0

        self.orchestrator = TrainingOrchestrator(
            coordinator_url=coordinator_url,
            worker_id=worker_id,
            world_size=world_size,
            rank=rank,
        )
        self.ckpt_dir = Path(checkpoint_dir)
        self.ckpt_dir.mkdir(parents=True, exist_ok=True)
        self.checkpoint_manager = CheckpointManager(str(self.ckpt_dir), keep_count=5)

        if HAS_TORCH:
            self.device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
        else:
            self.device = "cpu-fallback"

        self.model = None
        self.optimizer = None
        self.criterion = None
        self.trainloader = None
        self.testloader = None
        self.fallback_weights = None
        self.fallback_dataset = None

    def connect_and_register(self):
        config = self.orchestrator.register_worker(
            worker_id=self.worker_id,
            hostname="127.0.0.1",
            port=50052 + self.rank,
            gpu_count=1 if (HAS_TORCH and torch.cuda.is_available()) else 0,
        )
        self.rank = config.rank
        self.world_size = config.world_size
        print(
            f"[{self.worker_id}] Registered with coordinator as rank "
            f"{config.rank}/{config.world_size}"
        )
        return config

    def download_and_prepare_data(self):
        if HAS_TORCH:
            data_dir = Path("./data")
            data_dir.mkdir(exist_ok=True)

            transform_train = transforms.Compose(
                [
                    transforms.RandomCrop(32, padding=4),
                    transforms.RandomHorizontalFlip(),
                    transforms.ToTensor(),
                    transforms.Normalize((0.4914, 0.4822, 0.4465), (0.2023, 0.1994, 0.2010)),
                ]
            )
            transform_test = transforms.Compose(
                [
                    transforms.ToTensor(),
                    transforms.Normalize((0.4914, 0.4822, 0.4465), (0.2023, 0.1994, 0.2010)),
                ]
            )

            trainset = torchvision.datasets.CIFAR10(
                root=str(data_dir), train=True, download=True, transform=transform_train
            )
            testset = torchvision.datasets.CIFAR10(
                root=str(data_dir), train=False, download=True, transform=transform_test
            )

            total_samples = len(trainset)
            self.orchestrator.register_dataset(
                dataset_id="cifar10-train",
                path=str(data_dir),
                total_samples=total_samples,
                shard_size=max(1, total_samples // max(1, self.world_size)),
                format="torchvision",
                shuffle=True,
                seed=42,
            )
            shard = self.orchestrator.get_shard("cifar10-train", epoch=0)
            indices = list(range(shard.start_index, min(shard.end_index, total_samples)))
            if indices:
                trainset = torch.utils.data.Subset(trainset, indices)

            self.trainloader = torch.utils.data.DataLoader(
                trainset, batch_size=32, shuffle=True, num_workers=0
            )
            self.testloader = torch.utils.data.DataLoader(
                testset, batch_size=32, shuffle=False, num_workers=0
            )
            print(f"[{self.worker_id}] Prepared CIFAR-10 shard: {len(trainset)} samples")
        else:
            total_samples = 1000
            self.orchestrator.register_dataset(
                dataset_id="cifar10-synthetic",
                path="./data",
                total_samples=total_samples,
                shard_size=max(1, total_samples // max(1, self.world_size)),
                format="synthetic",
                shuffle=True,
                seed=42,
            )
            shard = self.orchestrator.get_shard("cifar10-synthetic", epoch=0)
            num_samples = max(10, shard.end_index - shard.start_index)
            rng = random.Random(42 + self.rank)
            if HAS_NUMPY:
                np_rng = np.random.default_rng(42 + self.rank)
                x = np_rng.normal(0.0, 1.0, size=(num_samples, 16))
                true_w = np.linspace(-1.0, 1.0, 16)
                y = (x @ true_w > 0).astype(np.float64)
                self.fallback_dataset = (x, y)
                self.fallback_weights = np.zeros(16, dtype=np.float64)
            else:
                x = [[rng.gauss(0.0, 1.0) for _ in range(16)] for _ in range(num_samples)]
                true_w = [(-1.0 + 2.0 * i / 15.0) for i in range(16)]
                y = [1.0 if sum(a * b for a, b in zip(row, true_w)) > 0 else 0.0 for row in x]
                self.fallback_dataset = (x, y)
                self.fallback_weights = [0.0] * 16
            print(
                f"[{self.worker_id}] PyTorch not installed; prepared synthetic shard "
                f"[{shard.start_index}, {shard.end_index}) ({num_samples} samples)"
            )

    def initialize_model(self):
        if HAS_TORCH:
            self.model = SimpleCNN().to(self.device)
            self.optimizer = optim.Adam(self.model.parameters(), lr=0.001)
            self.criterion = nn.CrossEntropyLoss()

    async def send_heartbeat(self, state: str = "training", task: str = ""):
        await self.orchestrator.heartbeat_async(
            current_step=self.current_step,
            current_epoch=self.current_epoch,
            state=state,
            current_task=task,
        )

    async def save_and_notify_checkpoint(self, accuracy: float):
        if HAS_TORCH and self.model is not None:
            import io

            buf = io.BytesIO()
            torch.save(
                {
                    "epoch": self.current_epoch,
                    "step": self.current_step,
                    "model_state_dict": self.model.state_dict(),
                    "optimizer_state_dict": self.optimizer.state_dict(),
                    "accuracy": accuracy,
                },
                buf,
            )
            payload = buf.getvalue()
        else:
            weights_list = (
                self.fallback_weights.tolist()
                if HAS_NUMPY and isinstance(self.fallback_weights, np.ndarray)
                else list(self.fallback_weights)
            )
            payload = json.dumps(
                {
                    "epoch": self.current_epoch,
                    "step": self.current_step,
                    "accuracy": accuracy,
                    "weights": weights_list,
                    "worker_id": self.worker_id,
                    "timestamp": datetime.now().isoformat(),
                }
            ).encode("utf-8")

        ckpt_id = await self.checkpoint_manager.save_async(
            payload,
            step=self.current_step,
            epoch=self.current_epoch,
            metadata={"worker_id": self.worker_id, "accuracy": f"{accuracy:.4f}"},
        )
        info = self.checkpoint_manager.get_by_step(self.current_step)
        storage_path = info.path if info is not None else str(self.ckpt_dir / f"{ckpt_id}.ckpt")
        self.orchestrator.notify_checkpoint(
            checkpoint_id=ckpt_id,
            step=self.current_step,
            epoch=self.current_epoch,
            storage_path=storage_path,
            size_bytes=len(payload),
            checkpoint_type="full",
            metadata={"accuracy": f"{accuracy:.4f}"},
        )
        print(f"[{self.worker_id}] Saved & notified checkpoint {ckpt_id} ({len(payload)} bytes)")

    def _train_fallback_epoch(self) -> float:
        x, y = self.fallback_dataset
        lr = 0.05
        if HAS_NUMPY:
            n = len(y)
            for idx in range(0, n, 32):
                xb = x[idx : idx + 32]
                yb = y[idx : idx + 32]
                logits = np.clip(xb @ self.fallback_weights, -20.0, 20.0)
                probs = 1.0 / (1.0 + np.exp(-logits))
                grad = (xb.T @ (probs - yb)) / len(yb)
                self.fallback_weights -= lr * grad
                self.current_step += 1
            preds = (x @ self.fallback_weights > 0).astype(np.float64)
            return float((preds == y).mean() * 100.0)
        else:
            correct = 0
            for row, label in zip(x, y):
                z = max(-20.0, min(20.0, sum(a * b for a, b in zip(row, self.fallback_weights))))
                prob = 1.0 / (1.0 + math.exp(-z))
                err = prob - label
                for j in range(len(self.fallback_weights)):
                    self.fallback_weights[j] -= lr * err * row[j]
                if (prob >= 0.5) == (label >= 0.5):
                    correct += 1
                self.current_step += 1
            return 100.0 * correct / max(1, len(y))

    def evaluate_torch(self) -> float:
        self.model.eval()
        correct = 0
        total = 0
        with torch.no_grad():
            for images, labels in self.testloader:
                images, labels = images.to(self.device), labels.to(self.device)
                outputs = self.model(images)
                _, predicted = torch.max(outputs.data, 1)
                total += labels.size(0)
                correct += (predicted == labels).sum().item()
        return 100.0 * correct / max(1, total)

    async def train_epoch(self) -> float:
        if HAS_TORCH:
            self.model.train()
            for inputs, labels in self.trainloader:
                inputs, labels = inputs.to(self.device), labels.to(self.device)
                self.optimizer.zero_grad()
                outputs = self.model(inputs)
                loss = self.criterion(outputs, labels)
                loss.backward()
                self.optimizer.step()
                self.current_step += 1
                if self.current_step % 50 == 0:
                    await self.send_heartbeat(task=f"epoch_{self.current_epoch}")
            self.current_epoch += 1
            accuracy = self.evaluate_torch()
        else:
            accuracy = self._train_fallback_epoch()
            self.current_epoch += 1
            await self.send_heartbeat(task=f"epoch_{self.current_epoch}")

        print(f"[{self.worker_id}] Epoch {self.current_epoch} accuracy: {accuracy:.2f}%")
        if accuracy >= self.best_accuracy:
            self.best_accuracy = accuracy
            await self.save_and_notify_checkpoint(accuracy)
        return accuracy

    async def start_training(self, num_epochs: int = 3):
        try:
            self.connect_and_register()
            self.download_and_prepare_data()
            self.initialize_model()

            for epoch in range(num_epochs):
                await self.train_epoch()
                barrier = await self.orchestrator.wait_barrier_async(
                    f"epoch-{epoch}-sync", step=self.current_step
                )
                print(
                    f"[{self.worker_id}] Barrier epoch-{epoch}-sync released "
                    f"({barrier.arrival_order}/{barrier.participants})"
                )

            await self.send_heartbeat(state="idle", task="complete")
            print(f"[{self.worker_id}] Training complete! Best accuracy: {self.best_accuracy:.2f}%")
        finally:
            try:
                self.orchestrator.deregister()
            except Exception:
                pass


async def main():
    parser = argparse.ArgumentParser(description="Real Distributed Training Example")
    parser.add_argument("--worker-id", default="real-worker-01", help="Worker ID")
    parser.add_argument("--coordinator", default="http://localhost:50051", help="Coordinator URL")
    parser.add_argument("--rank", type=int, default=0, help="Worker rank")
    parser.add_argument("--world-size", type=int, default=1, help="Total number of workers")
    parser.add_argument("--epochs", type=int, default=2, help="Number of epochs")
    parser.add_argument("--checkpoint-dir", default="./checkpoints", help="Checkpoint directory")
    args = parser.parse_args()

    trainer = RealDistributedTrainer(
        worker_id=args.worker_id,
        coordinator_url=args.coordinator,
        rank=args.rank,
        world_size=args.world_size,
        checkpoint_dir=args.checkpoint_dir,
    )
    await trainer.start_training(args.epochs)


if __name__ == "__main__":
    asyncio.run(main())