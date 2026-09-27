#!/usr/bin/env python3
"""Strata mock coordinator API — lets the dashboard run Live without Rust.

Serves http://localhost:51051/api/* with demo data matching ApiDashboardState.
Usage: python3 scripts/mock_api.py [--port 51051]
"""

from __future__ import annotations

import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, HTTPServer

START = time.time()


def dashboard_state() -> dict:
    now_ms = int(time.time() * 1000)
    uptime = int(time.time() - START)
    step = uptime // 3 or 1
    return {
        "coordinator": {
            "connected": True,
            "address": "localhost:50051",
            "uptime": uptime,
            "version": "0.1.0",
        },
        "workers": [
            {
                "id": "gpu-worker-01", "ip": "10.0.0.1", "port": 50052,
                "status": "active", "gpu_count": 8, "last_heartbeat": now_ms,
                "assigned_shards": 12, "current_epoch": step // 500 + 1,
                "current_step": step % 500, "current_task": "forward_pass",
            },
            {
                "id": "gpu-worker-02", "ip": "10.0.0.2", "port": 50052,
                "status": "active", "gpu_count": 8, "last_heartbeat": now_ms,
                "assigned_shards": 12, "current_epoch": step // 500 + 1,
                "current_step": max(step % 500 - 2, 0), "current_task": "backward_pass",
            },
        ],
        "datasets": [
            {
                "id": "imagenet-train", "name": "ImageNet Training Set",
                "total_samples": 1281167, "shard_size": 10000, "shard_count": 128,
                "format": "tfrecord", "shuffle": True, "registered_at": now_ms - 3600000,
            }
        ],
        "checkpoints": [
            {
                "id": "checkpoint_epoch_1", "step": 500, "epoch": 1,
                "size": 650 * 1024 * 1024, "path": "/checkpoints/epoch_1.pt",
                "created_at": now_ms - 60000, "worker_id": "gpu-worker-01",
                "status": "completed",
            }
        ],
        "barriers": [],
        "metrics": {
            "checkpoint_throughput": 480, "coordinator_rps": 10200,
            "active_workers": 2, "total_workers": 2,
            "barrier_latency_p99": 42, "shard_assignment_time": 8,
        },
        "tasks": [
            {
                "id": "task_vision_training", "name": "Vision Model Training",
                "type": "image_classification", "status": "running",
                "worker_ids": ["gpu-worker-01", "gpu-worker-02"],
                "dataset_id": "imagenet-train", "started_at": now_ms - uptime * 1000,
                "completed_at": None, "progress": min(uptime % 300 * 100 // 300, 95),
                "logs": ["[start] Task started with 2 workers"],
            }
        ],
        "logs": [
            {
                "id": "log_1", "timestamp": now_ms - 5000, "level": "info",
                "message": "Mock coordinator serving demo data", "source": "coordinator",
                "task_id": None, "worker_id": None,
            }
        ],
    }


class Handler(BaseHTTPRequestHandler):
    def _send(self, obj: object, status: int = 200) -> None:
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self) -> None:  # CORS preflight
        self.send_response(200)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()

    def do_GET(self) -> None:
        state = dashboard_state()
        path = self.path.split("?")[0]
        if path == "/api/health":
            return self._send({"status": "ok"})
        if path == "/api/dashboard":
            return self._send(state)
        for key in ("status", "workers", "datasets", "checkpoints",
                    "barriers", "metrics", "tasks", "logs"):
            if path == f"/api/{key}":
                if key == "status":
                    return self._send(state["coordinator"])
                return self._send(state[key])
        return self._send({"error": "not found"}, 404)

    def do_POST(self) -> None:  # task create/stop stubs
        length = int(self.headers.get("Content-Length", 0))
        if length:
            self.rfile.read(length)
        if self.path == "/api/tasks":
            return self._send({"task_id": "task_mock_1"})
        if self.path.endswith("/stop"):
            return self._send({"success": True})
        return self._send({"error": "not found"}, 404)

    def log_message(self, fmt: str, *args: object) -> None:
        print(f"[mock-api] {args[0]}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=51051)
    args = parser.parse_args()
    server = HTTPServer(("127.0.0.1", args.port), Handler)
    print(f"[mock-api] Strata mock coordinator on http://localhost:{args.port}/api")
    server.serve_forever()


if __name__ == "__main__":
    main()
