#!/usr/bin/env python3
"""Strata coordinator API load tester (senior-backend: api_load_tester).

Hammers the HTTP API with concurrent load and reports p50/p99 latencies.
Requires the coordinator running: DEMO_MODE=true cargo run --bin coordinator

Usage:
    python scripts/api_load_test.py --base-url http://localhost:51051 --requests 200 --concurrency 20
"""

from __future__ import annotations

import argparse
import statistics
import time
import urllib.request
import json
from concurrent.futures import ThreadPoolExecutor


def get(path: str, base_url: str, timeout: float = 10.0) -> tuple[int, float]:
    url = f"{base_url.rstrip('/')}{path}"
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(url, timeout=timeout) as resp:
            resp.read()
            return resp.status, (time.perf_counter() - start) * 1000.0
    except Exception as exc:  # noqa: BLE001 — report as failure sample
        return 0, (time.perf_counter() - start) * 1000.0


def percentile(samples: list[float], pct: float) -> float:
    if not samples:
        return 0.0
    ordered = sorted(samples)
    idx = min(int(len(ordered) * pct / 100.0), len(ordered) - 1)
    return ordered[idx]


def main() -> int:
    parser = argparse.ArgumentParser(description="Strata API load tester")
    parser.add_argument("--base-url", default="http://localhost:51051")
    parser.add_argument("--requests", type=int, default=200)
    parser.add_argument("--concurrency", type=int, default=20)
    parser.add_argument("--endpoint", default="/api/dashboard")
    parser.add_argument("--max-p99-ms", type=float, default=1500.0)
    args = parser.parse_args()

    latencies: list[float] = []
    statuses: list[int] = []
    start = time.perf_counter()
    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futures = [pool.submit(get, args.endpoint, args.base_url) for _ in range(args.requests)]
        for fut in futures:
            status, ms = fut.result()
            statuses.append(status)
            latencies.append(ms)
    elapsed = time.perf_counter() - start

    ok = sum(1 for s in statuses if s == 200)
    p50 = percentile(latencies, 50)
    p99 = percentile(latencies, 99)
    rps = args.requests / elapsed if elapsed > 0 else 0.0

    print("=" * 52)
    print("STRATA API LOAD TEST")
    print("=" * 52)
    print(f"Endpoint   : {args.endpoint}")
    print(f"Requests   : {args.requests} @ concurrency {args.concurrency}")
    print(f"Success    : {ok}/{args.requests}")
    print(f"Throughput : {rps:.1f} rps in {elapsed:.2f}s")
    print(f"Latency p50: {p50:.1f} ms | p99: {p99:.1f} ms | max: {max(latencies):.1f} ms")
    print("=" * 52)

    if ok < args.requests:
        print(f"FAIL: {args.requests - ok} non-200 responses")
        return 1
    if p99 > args.max_p99_ms:
        print(f"FAIL: p99 {p99:.1f}ms exceeds budget {args.max_p99_ms:.1f}ms")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
