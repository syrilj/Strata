#!/bin/bash
# Comprehensive system verification script

set -euo pipefail

echo "════════════════════════════════════════════════════════════════"
echo "  Verifying Distributed Training Runtime System"
echo "════════════════════════════════════════════════════════════════"
echo ""

echo "✓ Step 1: Checking Rust compilation..."
cargo check --workspace --all-targets
echo "  ✅ All Rust crates compile"
echo ""

echo "✓ Step 2: Building Python bindings (dtruntime-python)..."
cargo build -p dtruntime-python --release
if command -v maturin >/dev/null 2>&1; then
    maturin develop
fi
echo "  ✅ Python bindings built"
echo ""

echo "✓ Step 3: Running Rust workspace tests..."
cargo test --workspace
echo "  ✅ Rust tests passed"
echo ""

echo "✓ Step 4: Checking benchmarks..."
cargo check -p benchmarks --benches
echo "  ✅ Benchmarks compile"
echo ""

echo "✓ Step 5: Checking coordinator binary..."
cargo build -p coordinator --bin coordinator
echo "  ✅ Coordinator binary builds"
echo ""

echo "✓ Step 6: Running Python binding & pytest checks..."
python3 test-python-bindings.py
pytest tests/python/ -v
echo "  ✅ Python tests passed"
echo ""

echo "✓ Step 7: Verifying dashboard..."
if [ ! -d "dashboard/node_modules" ]; then
    npm --prefix dashboard ci
fi
npm --prefix dashboard run lint
npm --prefix dashboard test
npm --prefix dashboard run build
echo "  ✅ Dashboard lint, tests, and build passed"
echo ""

echo "════════════════════════════════════════════════════════════════"
echo "  VERIFICATION SUMMARY"
echo "════════════════════════════════════════════════════════════════"
echo ""
echo "✅ Rust workspace compiles and tests pass"
echo "✅ Benchmarks compile"
echo "✅ Python bindings built and tested"
echo "✅ Coordinator binary builds"
echo "✅ Dashboard lint, tests, and build pass"
echo ""
echo "To run the system locally:"
echo "  ./run-local.sh          # Live mode (DEMO_MODE=false) with real workers"
echo "  ./run-local.sh --demo   # Seeded demo mode (DEMO_MODE=true)"
echo ""
echo "════════════════════════════════════════════════════════════════"
