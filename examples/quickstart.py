#!/usr/bin/env python3
"""Minimal single-worker walkthrough of the Strata Python API.

Start a coordinator first:

    cargo run --release -p coordinator --bin coordinator -- 127.0.0.1:50051

Then run:

    python examples/quickstart.py
"""

import argparse
import tempfile

from dtruntime import CheckpointManager, TrainingOrchestrator


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--coordinator", default="http://127.0.0.1:50051")
    args = parser.parse_args()

    orch = TrainingOrchestrator(args.coordinator, worker_id="worker-0", world_size=1, rank=0)
    cfg = orch.register_worker(hostname="127.0.0.1", port=50060)
    print(f"registered {cfg.worker_id}")

    try:
        # Describe a dataset: 100k samples in shards of 10k -> 10 shards.
        orch.register_dataset(
            dataset_id="train",
            path="/tmp/strata-quickstart-data",
            total_samples=100_000,
            shard_size=10_000,
            format="parquet",
            shuffle=True,
            seed=42,
        )
        shard = orch.get_shard("train", epoch=0)
        print(
            f"epoch 0, first assigned shard: samples [{shard.start_index}, {shard.end_index}) "
            f"(dataset has {shard.total_shards} shards)"
        )

        ckpt_dir = tempfile.mkdtemp(prefix="strata-ckpt-")
        ckpts = CheckpointManager(ckpt_dir)

        for step in range(1, 31):
            # ... a real training step would go here ...
            if step % 10 == 0:
                state = f"model-state-at-step-{step}".encode()
                ckpt_id = ckpts.save(state, step=step, epoch=0)  # queues a background write
                ckpts.wait_pending()  # block until it is on disk
                info = ckpts.get_by_step(step)
                orch.notify_checkpoint(
                    checkpoint_id=ckpt_id,
                    step=step,
                    epoch=0,
                    storage_path=info.path,
                    size_bytes=len(state),
                )
                orch.heartbeat(current_step=step, current_epoch=0, state="training")
                print(f"step {step}: checkpoint {ckpt_id} -> {info.path}")

        latest = orch.get_latest_checkpoint()
        restored = ckpts.load(latest.step)
        print(f"coordinator's latest checkpoint: step {latest.step}, payload {restored!r}")

        barrier = orch.barrier("quickstart-done", step=30)
        print(f"barrier released: {barrier.released}")
    finally:
        orch.deregister()
        print("deregistered")


if __name__ == "__main__":
    main()
