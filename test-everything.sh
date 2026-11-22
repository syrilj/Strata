#!/bin/bash
# Comprehensive test runner for Strata (Rust, Python bindings, benchmarks, dashboard)

set -euo pipefail

echo "════════════════════════════════════════════════════════════════"
echo "  Testing Strata (Distributed Training Runtime)"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "Step 1: Checking Rust workspace compilation..."
echo "────────────────────────────────────────────────────────────────"
cargo check --workspace --all-targets
echo "✅ All Rust crates compile successfully"
echo ""

echo "Step 2: Running Rust workspace tests..."
echo "────────────────────────────────────────────────────────────────"
cargo test --workspace
echo "✅ Rust workspace tests passed"
echo ""

echo "Step 3: Checking benchmarks..."
echo "────────────────────────────────────────────────────────────────"
cargo check -p benchmarks --benches
echo "✅ All benchmarks compile successfully"
echo ""

echo "Step 4: Building Python bindings (dtruntime-python)..."
echo "────────────────────────────────────────────────────────────────"
cargo build -p dtruntime-python --release
if command -v maturin >/dev/null 2>&1; then
    maturin develop
else
    pip install -e . --quiet
fi
echo "✅ Python bindings built and installed"
echo ""

echo "Step 5: Running Python binding & pytest suites..."
echo "────────────────────────────────────────────────────────────────"
python3 -c "from dtruntime import DatasetRegistry, CheckpointManager, TrainingOrchestrator; print('✅ Python imports successful')"
python3 test-python-bindings.py
pytest tests/python/ -v
echo "✅ Python tests passed"
echo ""

echo "Step 6: Running Dashboard lint, unit tests, and build..."
echo "────────────────────────────────────────────────────────────────"
if [ ! -d "dashboard/node_modules" ]; then
    npm --prefix dashboard ci
fi
npm --prefix dashboard run lint
npm --prefix dashboard test
npm --prefix dashboard run build
echo "✅ Dashboard checks passed"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "  ALL TESTS PASSED"
echo "════════════════════════════════════════════════════════════════"
