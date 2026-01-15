#!/usr/bin/env python3
"""
Quick test to verify Python bindings work correctly
"""

import sys

def test_imports():
    """Test that all modules can be imported"""
    print("Testing imports...")
    try:
        from dtruntime import DatasetRegistry, CheckpointManager, TrainingOrchestrator
        print("✅ All imports successful")
        return True
    except ImportError as e:
        print(f"❌ Import failed: {e}")
        return False

def test_checkpoint_manager():
    """Test CheckpointManager creation"""
    print("\nTesting CheckpointManager...")
    try:
        from dtruntime import CheckpointManager
        import tempfile
        
        with tempfile.TemporaryDirectory() as tmpdir:
            mgr = CheckpointManager(tmpdir)
            print("✅ CheckpointManager created successfully")
            return True
    except Exception as e:
        print(f"❌ CheckpointManager test failed: {e}")
        return False

def test_dataset_registry():
    """Test DatasetRegistry creation"""
    print("\nTesting DatasetRegistry...")
    try:
        from dtruntime import DatasetRegistry
        
        # Note: This will fail to connect but tests the binding works
        registry = DatasetRegistry("http://localhost:50051")
        print("✅ DatasetRegistry created successfully")
        print("   (Connection will fail without coordinator running)")
        return True
    except Exception as e:
        print(f"❌ DatasetRegistry test failed: {e}")
        return False

def test_orchestrator():
    """Test TrainingOrchestrator creation"""
    print("\nTesting TrainingOrchestrator...")
    try:
        from dtruntime import TrainingOrchestrator
        
        orch = TrainingOrchestrator(
            worker_id="test-worker",
            coordinator_url="http://localhost:50051",
            world_size=1,
            rank=0
        )
        print("✅ TrainingOrchestrator created successfully")
        print("   (Connection will fail without coordinator running)")
        return True
    except Exception as e:
        print(f"❌ TrainingOrchestrator test failed: {e}")
        return False

def main():
    print("=" * 60)
    print("  Python Bindings Quick Test")
    print("=" * 60)
    
    results = []
    results.append(("Imports", test_imports()))
    results.append(("CheckpointManager", test_checkpoint_manager()))
    results.append(("DatasetRegistry", test_dataset_registry()))
    results.append(("TrainingOrchestrator", test_orchestrator()))
    
    print("\n" + "=" * 60)
    print("  RESULTS")
    print("=" * 60)
    
    for name, passed in results:
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{status:10} {name}")
    
    all_passed = all(r[1] for r in results)
    
    if all_passed:
        print("\n✅ All Python binding tests passed!")
        print("\nTo test with a real coordinator:")
        print("  1. Terminal 1: cargo run -p coordinator --bin coordinator -- 0.0.0.0:50051")
        print("  2. Terminal 2: pytest tests/python/ -v")
        return 0
    else:
        print("\n❌ Some tests failed. Check error messages above.")
        return 1

if __name__ == "__main__":
    sys.exit(main())
