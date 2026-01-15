#!/bin/bash
# Quick start script for demo mode

echo "🚀 Starting Distributed Training Runtime Demo"
echo ""
echo "This will start:"
echo "  1. Coordinator with demo data (port 50051 gRPC, 51051 HTTP)"
echo "  2. Dashboard (port 3000)"
echo ""
echo "Press Ctrl+C to stop all services"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "🛑 Stopping services..."
    kill $COORDINATOR_PID $DASHBOARD_PID 2>/dev/null
    exit 0
}

trap cleanup INT TERM

# Start coordinator in demo mode
echo "📡 Starting coordinator..."
DEMO_MODE=true cargo run --release --bin coordinator -- 0.0.0.0:50051 > /tmp/coordinator.log 2>&1 &
COORDINATOR_PID=$!

# Wait for coordinator to start
echo "⏳ Waiting for coordinator to initialize..."
sleep 3

# Check if coordinator is running
if ! kill -0 $COORDINATOR_PID 2>/dev/null; then
    echo "❌ Coordinator failed to start. Check /tmp/coordinator.log"
    exit 1
fi

# Test coordinator health
if curl -s http://localhost:51051/api/health > /dev/null 2>&1; then
    echo "✅ Coordinator is running"
else
    echo "⚠️  Coordinator started but health check failed"
fi

# Start dashboard
echo "🎨 Starting dashboard..."
cd dashboard && npm run dev > /tmp/dashboard.log 2>&1 &
DASHBOARD_PID=$!
cd ..

echo ""
echo "✅ All services started!"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Dashboard:    http://localhost:3000"
echo "  API:          http://localhost:51051/api/health"
echo "  Coordinator:  localhost:50051 (gRPC)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "📊 Demo includes:"
echo "  • 3 simulated workers (2 GPU, 1 CPU)"
echo "  • Active training task with progress"
echo "  • Real-time metrics and logs"
echo "  • Sample training data preview"
echo ""
echo "Logs:"
echo "  Coordinator: /tmp/coordinator.log"
echo "  Dashboard:   /tmp/dashboard.log"
echo ""
echo "Press Ctrl+C to stop all services"
echo ""

# Wait for user interrupt
wait
