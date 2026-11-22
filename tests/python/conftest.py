import os
import socket
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path

import pytest


def find_free_port_pair():
    """Find a free gRPC port whose corresponding HTTP port (port + 1000) is also free."""
    for _ in range(50):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s_grpc:
            s_grpc.bind(("127.0.0.1", 0))
            grpc_port = s_grpc.getsockname()[1]
        if grpc_port + 1000 > 65535:
            continue
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s_http:
            try:
                s_http.bind(("127.0.0.1", grpc_port + 1000))
                return grpc_port, grpc_port + 1000
            except OSError:
                continue
    raise RuntimeError("Could not find a free (grpc_port, grpc_port + 1000) pair")


def _repo_root() -> Path:
    return Path(__file__).resolve().parent.parent.parent


def _ensure_coordinator_binary(repo_root: Path) -> Path:
    debug_bin = repo_root / "target" / "debug" / "coordinator"
    release_bin = repo_root / "target" / "release" / "coordinator"
    if debug_bin.exists():
        return debug_bin
    if release_bin.exists():
        return release_bin

    subprocess.run(
        ["cargo", "build", "-p", "coordinator", "--bin", "coordinator"],
        cwd=str(repo_root),
        check=True,
    )
    if debug_bin.exists():
        return debug_bin
    raise RuntimeError(f"Coordinator binary not found after build at {debug_bin}")


@pytest.fixture(scope="session")
def coordinator_server(tmp_path_factory):
    """Build (if needed) and start the coordinator binary, polling /api/health until ready."""
    repo_root = _repo_root()
    binary = _ensure_coordinator_binary(repo_root)
    grpc_port, http_port = find_free_port_pair()
    addr = f"127.0.0.1:{grpc_port}"
    http_addr = f"127.0.0.1:{http_port}"
    health_url = f"http://{http_addr}/api/health"
    ckpt_dir = tmp_path_factory.mktemp("coordinator-checkpoints")

    env = os.environ.copy()
    env["GRPC_ADDR"] = addr
    env["HTTP_ADDR"] = http_addr
    # The coordinator reads CHECKPOINT_DIR; without it, it writes to ./data in the repo.
    env["CHECKPOINT_DIR"] = str(ckpt_dir)
    env["DEMO_MODE"] = "false"

    proc = subprocess.Popen(
        [str(binary), addr],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        cwd=str(repo_root),
        env=env,
    )

    deadline = time.time() + 30.0
    ready = False
    while time.time() < deadline:
        if proc.poll() is not None:
            stdout, stderr = proc.communicate()
            raise RuntimeError(
                f"Coordinator exited early ({proc.returncode}).\nStdout: {stdout}\nStderr: {stderr}"
            )
        try:
            with urllib.request.urlopen(health_url, timeout=0.5) as resp:
                if resp.status == 200:
                    ready = True
                    break
        except (urllib.error.URLError, ConnectionError, TimeoutError, OSError):
            time.sleep(0.1)

    if not ready:
        proc.terminate()
        stdout, stderr = proc.communicate(timeout=5)
        raise RuntimeError(
            f"Coordinator timed out waiting for {health_url}.\nStdout: {stdout}\nStderr: {stderr}"
        )

    yield f"http://{addr}"

    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


@pytest.fixture
def temp_checkpoint_dir(tmp_path):
    d = tmp_path / "checkpoints"
    d.mkdir()
    return str(d)


@pytest.fixture
def temp_dataset_dir(tmp_path):
    d = tmp_path / "dataset"
    d.mkdir()
    for i in range(5):
        (d / f"part-{i}.parquet").write_text("dummy data")
    return str(d)
