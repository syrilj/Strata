# CLAUDE.md

Rules for coding agents (and humans) working in this repository.

## Layout

- `crates/runtime-core` – config, shared types, errors
- `crates/coordinator` – gRPC service (Tonic), HTTP API (Axum) for the dashboard, `coordinator` binary
- `crates/data-shard` – consistent hash ring, epoch shuffling, `ShardManager`
- `crates/checkpoint` – async checkpoint manager and writer
- `crates/storage` – `StorageBackend` trait, `LocalStorage`, `S3Storage` (behind the `s3` feature; not wired into the coordinator or Python API)
- `crates/python-bindings` – PyO3 extension `dtruntime._core`
- `python/dtruntime` – Python package
- `tests/rust` – Rust integration/e2e tests; `tests/python` – pytest suite (starts the real coordinator binary)
- `benchmarks/` and `crates/data-shard/benches/` – Criterion benchmarks
- `dashboard/` – React dashboard

## Build

Requires `protoc` (`apt-get install protobuf-compiler`) for the coordinator's `build.rs`.

```bash
cargo build --all-targets
python3 -m venv .venv && . .venv/bin/activate
pip install -e ".[dev]"      # builds the PyO3 extension via maturin
```

## Checks that must pass before every commit

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
pytest tests/python -v       # inside the venv, after `pip install -e ".[dev]"`
```

If you changed `dashboard/`, also run `npm ci && npx tsc --noEmit && npm run lint && npm test` in `dashboard/`.

Do not commit if any of these fail. Do not skip, `#[ignore]`, or delete a failing test to get green; fix the cause or report it.

## Numbers and claims

- **Never add benchmark or performance numbers that weren't measured.** Every number in `README.md`, `docs/`, or `CHANGELOG.md` must come from `BENCHMARKS.md`.
- When you add or update a number in `BENCHMARKS.md`, record the exact command, the date, and the machine (CPU, RAM, OS, storage), and say what the benchmark actually times.
- Do not quote hardware you did not run on, worker counts you did not test, or comparisons against baselines that do not exist in the repo.
- `DEMO_MODE=true` serves synthetic data. Never present dashboard values from demo mode as measurements.
- Do not describe features that are not in the code. Check the code before documenting a behavior (e.g. "zero-copy", "S3 checkpointing", retries, compression).
- Do not claim S3 works end-to-end until the coordinator and Python API actually use `S3Storage` and it has been tested against S3 or MinIO.

## Testing conventions

- Start test/bench servers by binding `127.0.0.1:0` and passing the listener to tonic (`serve_with_incoming_shutdown`). Do not use `portpicker`: it requires IPv6 and fails on IPv4-only hosts.
- `CheckpointManager::save_async` only enqueues a write. Benchmarks or tests that need the data on disk must wait for completion (`wait_pending`) or call the storage backend directly.

## Git

- Small, focused commits with descriptive messages.
- Never rewrite published history, force-push shared branches, or change commit dates.
