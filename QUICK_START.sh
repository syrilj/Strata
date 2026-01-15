#!/bin/bash
# Quick start script to verify the system works

set -e

echo "🚀 Distributed Training Runtime - Quick Test"
echo "============================================"
echo ""

# Check if coordinator is running
if lsof -i :51051 >/dev/null 2>&1; then
    echo "✅ Coordinator is running"
else
    echo "❌ Coordinator not running. Starting it..."
    echo "   Run: DEMO_MODE=true cargo run --bin coordinator -- 0.0.0.0:50051"
    exit 1
fi

# Check if dashboard is running
if lsof -i :3000 >/dev/null 2>&1; then
    echo "✅ Dashboard is running"
else
    echo "❌ Dashboard not running. Starting it..."
    echo "   Run: cd dashboard && npm run dev"
    exit 1
fi

echo ""
echo "Testing API endpoints..."
echo ""

# Test health
echo "1. Health Check:"
curl -s http://localhost:51051/api/health | jq '.'
echo ""

# Test workers
echo "2. Workers:"
WORKERS=$(curl -s http://localhost:51051/api/workers | jq 'length')
echo "   Found $WORKERS workers"
echo ""

# Test tasks
echo "3. Tasks:"
TASKS=$(curl -s http://localhost:51051/api/tasks | jq 'length')
echo "   Found $TASKS active tasks"
echo ""

# Test datasets
echo "4. Datasets:"
DATASETS=$(curl -s http://localhost:51051/api/datasets | jq 'length')
echo "   Found $DATASETS datasets"
echo ""

# Test logs
echo "5. System Logs:"
LOGS=$(curl -s http://localhost:51051/api/logs | jq 'length')
echo "   Found $LOGS log entries"
echo ""

echo "============================================"
echo "✅ All tests passed!"
echo ""
echo "🌐 Dashboard: http://localhost:3000"
echo "🔧 API: http://localhost:51051/api"
echo ""
echo "Try these commands:"
echo "  curl http://localhost:51051/api/dashboard | jq '.'"
echo "  curl http://localhost:51051/api/workers | jq '.'"
echo "  curl http://localhost:51051/api/tasks | jq '.'"
echo ""
