#!/bin/bash
# Start coordinator and dashboard server

echo "Starting Distributed Training Runtime..."
echo "========================================="

# Start the dashboard static file server in background
echo "Starting dashboard on port 3000..."
cd /app/dashboard
python3 -m http.server 3000 &
DASHBOARD_PID=$!

# Start the coordinator (this blocks)
echo "Starting coordinator on ports 50051 (gRPC) and 51051 (HTTP)..."
cd /app
exec /app/coordinator 0.0.0.0:50051
