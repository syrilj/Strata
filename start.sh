#!/bin/bash
# Quick start script for a local Strata cluster

set -euo pipefail

DEMO_MODE="${DEMO_MODE:-false}"

echo "🚀 Starting Strata (DEMO_MODE=${DEMO_MODE})"
echo ""
echo "This will start:"
echo "  1. Coordinator (port 50051 gRPC, 51051 HTTP)"
echo "  2. Dashboard (port 3000)"
echo ""
echo "Press Ctrl+C to stop all services"
echo ""

COORDINATOR_PID=""
DASHBOARD_PID=""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "🛑 Stopping services..."
    [ -n "$COORDINATOR_PID" ] && kill "$COORDINATOR_PID" 2>/dev/null || true
    [ -n "$DASHBOARD_PID" ] && kill "$DASHBOARD_PID" 2>/dev/null || true
    exit 0
}

trap cleanup INT TERM

# Start coordinator
echo "📡 Starting coordinator..."
DEMO_MODE="$DEMO_MODE" cargo run --release --bin coordinator -- 0.0.0.0:50051 > /tmp/coordinator.log 2>&1 &
COORDINATOR_PID=$!

# Wait for coordinator to start
echo "⏳ Waiting for coordinator to initialize..."
for _ in $(seq 1 30); do
    if curl -sf http://localhost:51051/api/health > /dev/null 2>&1; then
        break
    fi
    if ! kill -0 "$COORDINATOR_PID" 2>/dev/null; then
        echo "❌ Coordinator failed to start. Check /tmp/coordinator.log"
        exit 1
    fi
    sleep 1
done

# Test coordinator health
if curl -sf http://localhost:51051/api/health > /dev/null 2>&1; then
    echo "✅ Coordinator is running"
else
    echo "⚠️  Coordinator started but health check failed"
fi

# Install dashboard dependencies if missing
if [ ! -d "dashboard/node_modules" ]; then
    echo "📦 Installing dashboard dependencies..."
    npm --prefix dashboard install
fi

# Start dashboard
echo "🎨 Starting dashboard..."
npm --prefix dashboard run dev > /tmp/dashboard.log 2>&1 &
DASHBOARD_PID=$!

echo ""
echo "✅ All services started!"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Dashboard:    http://localhost:3000"
echo "  API:          http://localhost:51051/api/health"
echo "  Coordinator:  localhost:50051 (gRPC)"
echo "  Mode:         $([ "$DEMO_MODE" = "true" ] && echo "demo" || echo "live")"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "Logs:"
echo "  Coordinator: /tmp/coordinator.log"
echo "  Dashboard:   /tmp/dashboard.log"
echo ""
echo "Press Ctrl+C to stop all services"
echo ""

# Wait for user interrupt
wait
