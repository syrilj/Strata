#!/bin/bash
# Strata local launcher
# Starts the coordinator (50051 gRPC / 51051 HTTP) + real workers + dashboard (3000)
# Pass --demo to run in seeded demo mode (DEMO_MODE=true) without local workers.

set -euo pipefail

DEMO_FLAG=false
for arg in "$@"; do
    case "$arg" in
        --demo)
            DEMO_FLAG=true
            ;;
        -h|--help)
            echo "Usage: $0 [--demo]"
            echo "  (default)  Live mode (DEMO_MODE=false) with real Python workers + dashboard"
            echo "  --demo     Seeded demo mode (DEMO_MODE=true) without workers"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            exit 1
            ;;
    esac
done

DEMO_MODE_VAL="${DEMO_MODE:-false}"
if [ "$DEMO_FLAG" = true ]; then
    DEMO_MODE_VAL="true"
fi

echo "🚀 Strata - Local Cluster (DEMO_MODE=${DEMO_MODE_VAL})"
echo "============================================"
echo ""

COORDINATOR_PID=""
DASHBOARD_PID=""
WORKER_PIDS=()

cleanup() {
    echo ""
    echo "🛑 Stopping Strata..."
    for pid in "${WORKER_PIDS[@]:-}"; do
        kill "$pid" 2>/dev/null || true
    done
    if [ -n "$COORDINATOR_PID" ]; then
        kill "$COORDINATOR_PID" 2>/dev/null || true
    fi
    if [ -n "$DASHBOARD_PID" ]; then
        kill "$DASHBOARD_PID" 2>/dev/null || true
    fi
    pkill -f "coordinator.*50051" 2>/dev/null || true
    pkill -f "scripts/real_worker.py" 2>/dev/null || true
    pkill -f "scripts/simulated_worker.py" 2>/dev/null || true
    echo "✅ Stopped"
    exit 0
}

trap cleanup SIGINT SIGTERM

# Stop existing coordinator if running on 50051
if lsof -Pi :50051 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "⚠️  Coordinator already running on port 50051"
    echo "   Stopping existing coordinator..."
    pkill -f "coordinator.*50051" || true
    sleep 2
fi

# Start dashboard if not already running on 3000
if lsof -Pi :3000 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "⚠️  Dashboard already running on port 3000 (http://localhost:3000)"
else
    echo "🔧 Starting dashboard..."
    if [ ! -d "dashboard/node_modules" ]; then
        npm --prefix dashboard install --silent
    fi
    npm --prefix dashboard run dev >/tmp/strata-dashboard.log 2>&1 &
    DASHBOARD_PID=$!
    echo "   Dashboard starting in background..."
fi

echo ""
echo "🎯 Starting coordinator on 0.0.0.0:50051 (gRPC) and 0.0.0.0:51051 (HTTP)..."
DEMO_MODE="$DEMO_MODE_VAL" \
GRPC_ADDR="0.0.0.0:50051" \
HTTP_ADDR="0.0.0.0:51051" \
cargo run -p coordinator --bin coordinator -- 0.0.0.0:50051 >/tmp/strata-coordinator.log 2>&1 &
COORDINATOR_PID=$!

# Wait for coordinator health
echo "⏳ Waiting for coordinator to initialize..."
for _ in $(seq 1 30); do
    if curl -sf http://localhost:51051/api/health >/dev/null 2>&1; then
        break
    fi
    sleep 1
done

if curl -sf http://localhost:51051/api/health >/dev/null 2>&1; then
    echo "✅ Coordinator API ready"
else
    echo "❌ Coordinator failed to start. Check /tmp/strata-coordinator.log"
    exit 1
fi

if [ "$DEMO_MODE_VAL" != "true" ]; then
    echo "🤖 Starting real Python workers..."
    if python3 -c "import dtruntime" >/dev/null 2>&1; then
        for i in 1 2; do
            python3 scripts/real_worker.py \
                --worker-id "gpu-node-${i}" \
                --coordinator "http://127.0.0.1:50051" \
                --gpu-count 4 \
                --port "$((50051 + i))" \
                >/tmp/strata-worker-${i}.log 2>&1 &
            WORKER_PIDS+=($!)
        done
    else
        echo "   dtruntime native module not installed; falling back to scripts/simulated_worker.py"
        for i in 1 2; do
            COORDINATOR_URL="127.0.0.1:50051" \
            WORKER_ID="gpu-node-${i}" \
            GPU_COUNT=4 \
            python3 scripts/simulated_worker.py >/tmp/strata-worker-${i}.log 2>&1 &
            WORKER_PIDS+=($!)
        done
    fi
fi

if curl -sf http://localhost:3000 >/dev/null 2>&1; then
    echo "✅ Dashboard ready"
else
    echo "⏳ Dashboard still starting..."
fi

echo ""
echo "🌐 Strata is now running!"
echo "   Dashboard: http://localhost:3000"
echo "   HTTP API:  http://localhost:51051/api"
echo "   gRPC:      localhost:50051"
echo "   Mode:      $([ "$DEMO_MODE_VAL" = "true" ] && echo "demo" || echo "live")"
echo ""
echo "Press Ctrl+C to stop all services"

while true; do
    if curl -sf http://localhost:51051/api/metrics >/dev/null 2>&1; then
        METRICS=$(curl -sf http://localhost:51051/api/metrics)
        WORKERS=$(echo "$METRICS" | jq -r '.active_workers // 0')
        RPS=$(echo "$METRICS" | jq -r '.coordinator_rps // 0')
        THROUGHPUT=$(echo "$METRICS" | jq -r '.checkpoint_throughput // 0')
        printf "\r⚡ Active Workers: %s | RPS: %s | Checkpoint Throughput: %s MB/s" "$WORKERS" "$RPS" "$THROUGHPUT"
    else
        printf "\r⏳ Waiting for coordinator metrics..."
    fi
    sleep 2
done