#!/bin/bash
# Start coordinator and dashboard reverse-proxy server

set -euo pipefail

echo "Starting Distributed Training Runtime..."
echo "========================================="

export GRPC_ADDR="${GRPC_ADDR:-0.0.0.0:50051}"
export HTTP_ADDR="${HTTP_ADDR:-0.0.0.0:51051}"

HTTP_PORT="${HTTP_ADDR##*:}"
export COORDINATOR_API_URL="${COORDINATOR_API_URL:-http://127.0.0.1:${HTTP_PORT}}"
export HOST="${HOST:-0.0.0.0}"
export PORT="${PORT:-3000}"
export NO_BROWSER="true"

# Start the dashboard SPA + /api reverse-proxy server in background
echo "Starting dashboard on ${HOST}:${PORT} (proxying /api/* -> ${COORDINATOR_API_URL})..."
python3 /app/dashboard/serve.py &
DASHBOARD_PID=$!

cleanup() {
    kill "$DASHBOARD_PID" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Start the coordinator (this blocks)
echo "Starting coordinator on ${GRPC_ADDR} (gRPC) and ${HTTP_ADDR} (HTTP)..."
cd /app
exec /app/coordinator "$GRPC_ADDR"
