import pytest
import subprocess
import time
import os
import signal
import sys
import uuid
import socket

def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

@pytest.fixture(scope="session")
def coordinator_server():
    """Starts the coordinator server as a subprocess and returns its address."""
    port = find_free_port()
    addr = f"127.0.0.1:{port}"
    
    print(f"Starting coordinator on {addr}")
    
    # Run cargo run -p coordinator --bin coordinator -- <addr>
    # Note: This assumes cargo is in path and we are in root
    proc = subprocess.Popen(
        ["cargo", "run", "-p", "coordinator", "--bin", "coordinator", "--", addr],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        cwd=os.getcwd()
    )
    
    # Wait for port to be open
    start_time = time.time()
    connected = False
    while time.time() - start_time < 30:
        if proc.poll() is not None:
             stdout, stderr = proc.communicate()
             raise RuntimeError(f"Coordinator failed to start. Exit code: {proc.returncode}\nStdout: {stdout}\nStderr: {stderr}")
        try:
             with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                  connected = True
                  break
        except (ConnectionRefusedError, socket.timeout, OSError):
             time.sleep(0.1)
    
    if not connected:
        proc.terminate()
        stdout, stderr = proc.communicate()
        raise RuntimeError(f"Coordinator timed out starting. \nStdout: {stdout}\nStderr: {stderr}")

    yield f"http://{addr}"
    
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()

@pytest.fixture
def temp_checkpoint_dir(tmp_path):
    d = tmp_path / "checkpoints"
    d.mkdir()
    return str(d)

@pytest.fixture
def temp_dataset_dir(tmp_path):
    d = tmp_path / "dataset"
    d.mkdir()
    # Create dummy files
    for i in range(5):
        (d / f"part-{i}.parquet").write_text("dummy data")
    return str(d)
