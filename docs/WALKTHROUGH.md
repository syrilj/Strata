# Strata Walkthrough

This document tours the Strata dashboard.

> **About the screenshots:** they show synthetic demo data: the same fixed cluster the coordinator serves with `DEMO_MODE=true` (`gpu-worker-01`, `gpu-worker-02`, `cpu-worker-01`, an "ImageNet Training Set" dataset, etc.) with generated metric values. None of the numbers visible in the screenshots are measurements. For measured numbers see [BENCHMARKS.md](../BENCHMARKS.md).

## Overview

The walkthrough runs a local cluster with:
- **Coordinator**: Rust-based gRPC service managing workers and state.
- **Workers**: in live mode, local Python worker processes (`scripts/real_worker.py`); in demo mode, a fixed synthetic worker list.
- **Dashboard**: React-based UI for real-time monitoring.

## 1. Dashboard Overview

The main dashboard provides a "cockpit" view of the entire cluster.

![Dashboard Overview](demo-images/dashboard_overview_1768519762262.png)

**Key Metrics:**
- **Active Workers**: Shows connected workers (e.g., `gpu-worker-01`, `cpu-worker-01`) and their status.
- **Checkpoint throughput**: checkpoint write rate reported by the coordinator (synthetic in demo mode).
- **Barrier sync p99**: barrier wait latency reported by the coordinator (synthetic in demo mode).
- **Training Progress**: Per-worker epoch and step counters.

## 2. Dataset Management

The runtime manages distributed datasets, handling sharding and assignment.

![Datasets Page](demo-images/datasets_page_1768519803685.png)

The **Datasets** page shows registered datasets (e.g., "ImageNet Training Set") and provides a live preview of the data being processed by the workers.

## 3. Active Tasks

Track ongoing training jobs and their progress.

![Tasks Page](demo-images/tasks_page_1768519812446.png)

Each bar represents a worker's progress through the current epoch, giving instant visual feedback on cluster utilization.

## 4. Activity & Performance

Monitor system performance over time.

![Activity Page](demo-images/activity_page_1768519821851.png)

This view charts key metrics like **Requests Per Second (RPS)** and **Throughput**, helping identify bottlenecks or performance regressions.

## 5. System Logs

Detailed event logs for debugging and auditing.

![Logs Page](demo-images/logs_page_1768519832293.png)

The logs capture coordinator events, worker registrations, barrier completions, and errors.

## 6. Settings

Connection details for the coordinator and API.

![Settings Page](demo-images/settings_page_1768519848985.png)

## How to Run It

To run this yourself:

1.  **Start the services**:
    ```bash
    ./run-local.sh
    ```

2.  **Or run manually**:
    ```bash
    # Terminal 1: Start Dashboard
    cd dashboard
    npm install
    npm run dev

    # Terminal 2: Start Coordinator (seeded development data)
    DEMO_MODE=true cargo run --bin coordinator -- 0.0.0.0:50052
    ```

3.  **Open Browser**:
    Navigate to `http://localhost:3000`.
