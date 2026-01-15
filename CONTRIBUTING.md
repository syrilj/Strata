# Contributing to Distributed Training Runtime

Thank you for your interest in contributing! This document provides guidelines for contributing to the project.

## Code of Conduct

Be respectful and constructive in all interactions.

## Getting Started

### Prerequisites

- Rust 1.75+ ([install](https://rustup.rs/))
- Python 3.9+
- Git

### Development Setup

```bash
# Clone repository
git clone https://github.com/user/distributed-training-runtime.git
cd distributed-training-runtime

# Build project
cargo build

# Run tests
cargo test --all
pytest tests/python/

# Install development tools
cargo install cargo-watch  # Auto-rebuild on changes
cargo install cargo-edit   # Cargo add/rm commands
pip install -e ".[dev]"    # Python dev dependencies
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/issue-number-description
```

Branch naming conventions:
- `feature/` for new features
- `fix/` for bug fixes
- `docs/` for documentation changes
- `perf/` for performance improvements

### 2. Make Changes

- Write clear, concise code
- Follow existing code style
- Add tests for new functionality
- Update documentation as needed

### 3. Run Tests

```bash
# Rust tests
cargo test --all

# Python tests
pytest tests/python/ -v

# Benchmarks (to check for performance regressions)
cargo bench

# Format code
cargo fmt --all
rustfmt --check **/*.rs

# Lint
cargo clippy --all-targets --all-features
```

### 4. Commit Changes

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```bash
git commit -m "feat: add delta checkpoint support"
git commit -m "fix: handle coordinator connection retry"
git commit -m "docs: update API examples"
git commit -m "perf: optimize shard assignment algorithm"
```

Commit message format:
```
<type>(<scope>): <subject>

<body>

<footer>
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `perf`: Performance improvement
- `refactor`: Code refactoring
- `test`: Test additions/changes
- `chore`: Build process, dependencies, etc.

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub with:
- Clear description of changes
- Link to related issues (if any)
- Screenshots/benchmarks (if applicable)

## Code Style Guidelines

### Rust

Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/):

```rust
// Good: Clear naming, documented
/// Assigns shards to a worker for given epoch
///
/// # Arguments
/// * `epoch` - Training epoch number
/// * `worker_id` - Unique worker identifier
///
/// # Returns
/// Vector of shard IDs assigned to this worker
pub fn assign_shards(&self, epoch: u64, worker_id: &str) -> Vec<usize> {
    // Implementation
}

// Bad: Unclear, undocumented
pub fn assign(&self, e: u64, w: &str) -> Vec<usize> {
    // ...
}
```

**Style**:
- Use `cargo fmt` for formatting
- Run `cargo clippy` and fix warnings
- Prefer explicit error handling over `.unwrap()`
- Use descriptive variable names
- Document public APIs with `///` comments

### Python

Follow [PEP 8](https://pep8.org/):

```python
# Good
from dtruntime import CheckpointManager

async def save_checkpoint(manager: CheckpointManager, data: bytes, step: int) -> None:
    """Save checkpoint asynchronously.
    
    Args:
        manager: CheckpointManager instance
        data: Serialized checkpoint data
        step: Training step number
    """
    await manager.save_async(data, step)

# Bad
def save(m,d,s):
    m.save_async(d,s)
```

**Style**:
- Use type hints
- Document functions with docstrings
- Follow async/await patterns consistently
- Use `black` for formatting

## Testing Guidelines

### Unit Tests

Test individual components in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistent_hash_distribution() {
        let ring = ConsistentHash::new();
        ring.add_node("worker-1");
        ring.add_node("worker-2");
        
        // Verify even distribution
        let mut counts = HashMap::new();
        for i in 0..10000 {
            let node = ring.get_node_for_shard("dataset", i).unwrap();
            *counts.entry(node).or_insert(0) += 1;
        }
        
        // Each worker should get ~5000 shards (±5%)
        for count in counts.values() {
            assert!(*count > 4750 && *count < 5250);
        }
    }
}
```

### Integration Tests

Test component interactions:

```rust
#[tokio::test]
async fn test_checkpoint_end_to_end() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let backend = Arc::new(LocalBackend::new(temp_dir.path())?);
    
    // Write
    let writer = CheckpointWriter::new(backend.clone());
    let data = Bytes::from("checkpoint data");
    writer.write_checkpoint("model", 100, data.clone()).await?;
    
    // Read
    let manager = CheckpointManager::new(backend);
    let loaded = manager.load_checkpoint("model", 100).await?;
    
    assert_eq!(loaded, data);
    Ok(())
}
```

### Performance Tests

Use Criterion for benchmarks:

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_shard_assignment(c: &mut Criterion) {
    c.bench_function("assign_shards_1000_workers", |b| {
        let manager = ShardManager::new("dataset", 10000);
        b.iter(|| {
            for i in 0..1000 {
                manager.assign_shards(0, &format!("worker-{}", i));
            }
        });
    });
}

criterion_group!(benches, bench_shard_assignment);
criterion_main!(benches);
```

## Documentation

### Code Documentation

- Document all public APIs
- Include examples in doc comments
- Explain non-obvious implementation details

```rust
/// Manages checkpoint persistence with async I/O
///
/// # Examples
///
/// ```
/// use checkpoint::CheckpointManager;
/// use storage::LocalBackend;
///
/// let backend = LocalBackend::new("/checkpoints")?;
/// let manager = CheckpointManager::new(backend);
/// ```
pub struct CheckpointManager {
    // ...
}
```

### README Updates

Update README.md when:
- Adding new features
- Changing API
- Updating performance numbers
- Modifying installation steps

### Architecture Documentation

Update `docs/ARCHITECTURE.md` for:
- New components
- Design decision changes
- Performance characteristics
- Trade-off analysis

## Pull Request Process

1. **Create PR** with clear title and description
2. **Link issues**: Reference related issues (e.g., "Closes #123")
3. **Pass CI**: Ensure all tests pass
4. **Code review**: Address reviewer feedback
5. **Squash commits**: Clean up commit history
6. **Merge**: Maintainer will merge when ready

### PR Checklist

- [ ] Tests added/updated
- [ ] Documentation updated
- [ ] Benchmarks run (for performance changes)
- [ ] `cargo fmt` and `cargo clippy` pass
- [ ] Commit messages follow convention
- [ ] No breaking changes (or clearly documented)

## Performance Guidelines

When making performance-critical changes:

1. **Benchmark first**: Measure current performance
2. **Profile**: Use `perf`, `flamegraph`, or `cargo flamegraph`
3. **Optimize**: Make targeted improvements
4. **Benchmark again**: Verify improvement
5. **Document**: Update performance numbers in docs

```bash
# Profile with flamegraph
cargo flamegraph --bench checkpoint_throughput

# Run benchmarks
cargo bench --bench checkpoint_throughput -- --save-baseline before
# Make changes
cargo bench --bench checkpoint_throughput -- --baseline before
```

## Adding New Features

### Before Starting

1. **Check issues**: See if someone else is working on it
2. **Discuss**: Open an issue to discuss approach
3. **Design**: Think through architecture implications

### Implementation Checklist

- [ ] Core implementation in Rust
- [ ] Python bindings (if user-facing)
- [ ] Unit tests
- [ ] Integration tests
- [ ] Benchmarks (if performance-critical)
- [ ] Documentation
- [ ] Examples

### Example: Adding a New Storage Backend

1. Implement `StorageBackend` trait:

```rust
pub struct GCSBackend {
    bucket: String,
    client: gcs::Client,
}

#[async_trait]
impl StorageBackend for GCSBackend {
    async fn write(&self, path: &str, data: Bytes) -> Result<()> {
        // Implementation
    }
    
    async fn read(&self, path: &str) -> Result<Bytes> {
        // Implementation
    }
    
    // ... other methods
}
```

2. Add tests
3. Update documentation
4. Add example usage

## Reporting Issues

### Bug Reports

Include:
- Version (Git commit or release tag)
- Operating system
- Rust/Python version
- Minimal reproduction steps
- Expected vs actual behavior
- Logs/error messages

### Feature Requests

Include:
- Use case description
- Proposed API (if applicable)
- Alternatives considered
- Willing to implement?

## Questions?

- Open an issue for general questions
- Check existing documentation first
- Be specific and provide context

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
