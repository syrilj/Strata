#!/bin/bash
# Distributed Training Runtime Demo Launcher
# Shows active training tasks for interviews and presentations

set -e

echo "🚀 Distributed Training Runtime - Live Demo"
echo "============================================"
echo ""
echo "This demo shows:"
echo "  ✅ Active GPU workers training a vision model"
echo "  ✅ Real-time progress tracking (steps, epochs, loss)"
echo "  ✅ Checkpoint creation and management"
echo "  ✅ Dataset sharding and distribution"
echo "  ✅ Barrier synchronization between workers"
echo "  ✅ Live system metrics and monitoring"
echo ""

# Check if coordinator is already running
if lsof -Pi :50052 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "⚠️  Coordinator already running on port 50052"
    echo "   Stopping existing coordinator..."
    pkill -f "coordinator.*50052" || true
    sleep 2
fi

# Check if dashboard is already running
if lsof -Pi :3000 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "⚠️  Dashboard already running on port 3000"
    echo "   You can access it at: http://localhost:3000"
else
    echo "🔧 Starting dashboard..."
    cd dashboard
    npm install --silent
    npm run dev &
    DASHBOARD_PID=$!
    cd ..
    echo "   Dashboard starting in background..."
fi

echo ""
echo "🎯 Starting coordinator in DEMO MODE..."
echo "   This will show simulated active training tasks"

# Start coordinator in demo mode
DEMO_MODE=true cargo run --bin coordinator -- 0.0.0.0:50052 &
COORDINATOR_PID=$!

# Optionally start demo workers (uncomment to run)
# echo "🤖 Starting demo workers..."
# python3 tests/demos/demo.py &
# DEMO_WORKERS_PID=$!

# Wait for services to start
echo ""
echo "⏳ Waiting for services to initialize..."
sleep 5

# Check if services are running
if curl -s http://localhost:51052/api/health >/dev/null 2>&1; then
    echo "✅ Coordinator API ready"
else
    echo "❌ Coordinator failed to start"
    exit 1
fi

if curl -s http://localhost:3000 >/dev/null 2>&1; then
    echo "✅ Dashboard ready"
else
    echo "⏳ Dashboard still starting..."
fi

echo ""
echo "🌐 Demo is now running!"
echo "   Dashboard: http://localhost:3000"
echo "   API: http://localhost:51052/api"
echo ""
echo "📊 You'll see:"
echo "   • 3 active workers (2 GPU + 1 CPU)"
echo "   • Real-time training progress"
echo "   • ImageNet and custom datasets"
echo "   • Automatic checkpoint creation"
echo "   • Live performance metrics"
echo ""
echo "Press Ctrl+C to stop the demo"

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "🛑 Stopping demo..."
    if [ ! -z "$COORDINATOR_PID" ]; then
        kill $COORDINATOR_PID 2>/dev/null || true
    fi
    if [ ! -z "$DASHBOARD_PID" ]; then
        kill $DASHBOARD_PID 2>/dev/null || true
    fi
    if [ ! -z "$DEMO_WORKERS_PID" ]; then
        kill $DEMO_WORKERS_PID 2>/dev/null || true
    fi
    pkill -f "coordinator.*50052" 2>/dev/null || true
    pkill -f "vite.*3000" 2>/dev/null || true
    pkill -f "tests/demos/demo.py" 2>/dev/null || true
    echo "✅ Demo stopped"
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM

# Keep script running and show live stats
while true; do
    if curl -s http://localhost:51052/api/metrics >/dev/null 2>&1; then
        METRICS=$(curl -s http://localhost:51052/api/metrics)
        WORKERS=$(echo $METRICS | jq -r '.active_workers // 0')
        RPS=$(echo $METRICS | jq -r '.coordinator_rps // 0')
        THROUGHPUT=$(echo $METRICS | jq -r '.checkpoint_throughput // 0')
        
        printf "\r⚡ Active Workers: %s | RPS: %s | Checkpoint Throughput: %s/min" "$WORKERS" "$RPS" "$THROUGHPUT"
    else
        printf "\r⏳ Starting services..."
    fi
    sleep 2
done