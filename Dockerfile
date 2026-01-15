# Multi-stage build for the Distributed Training Runtime
# Stage 1: Build Rust components
FROM rust:1.82-bookworm AS rust-builder

# Install protobuf compiler
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./
COPY crates/runtime-core/Cargo.toml crates/runtime-core/
COPY crates/checkpoint/Cargo.toml crates/checkpoint/
COPY crates/data-shard/Cargo.toml crates/data-shard/
COPY crates/storage/Cargo.toml crates/storage/
COPY crates/coordinator/Cargo.toml crates/coordinator/
COPY crates/python-bindings/Cargo.toml crates/python-bindings/

# Create dummy source files for dependency caching
RUN mkdir -p crates/runtime-core/src && echo "pub fn dummy() {}" > crates/runtime-core/src/lib.rs && \
    mkdir -p crates/checkpoint/src && echo "pub fn dummy() {}" > crates/checkpoint/src/lib.rs && \
    mkdir -p crates/data-shard/src && echo "pub fn dummy() {}" > crates/data-shard/src/lib.rs && \
    mkdir -p crates/storage/src && echo "pub fn dummy() {}" > crates/storage/src/lib.rs && \
    mkdir -p crates/coordinator/src && echo "pub fn dummy() {}" > crates/coordinator/src/lib.rs && \
    mkdir -p crates/python-bindings/src && echo "pub fn dummy() {}" > crates/python-bindings/src/lib.rs && \
    mkdir -p benchmarks/benches && echo "fn main() {}" > benchmarks/benches/dummy.rs && \
    mkdir -p tests/rust/src && echo "fn main() {}" > tests/rust/src/lib.rs && \
    echo '[package]\nname = "benchmarks"\nversion = "0.1.0"\nedition = "2021"\n\n[[bench]]\nname = "dummy"\nharness = false' > benchmarks/Cargo.toml && \
    echo '[package]\nname = "integration-tests"\nversion = "0.1.0"\nedition = "2021"' > tests/rust/Cargo.toml

# Copy proto files
COPY proto/ proto/

# Build dependencies only
RUN cargo build --release -p coordinator 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/

# Build the coordinator
RUN cargo build --release -p coordinator --bin coordinator

# Stage 2: Build dashboard
FROM node:20-alpine AS dashboard-builder

WORKDIR /app/dashboard

COPY dashboard/package*.json ./
RUN npm ci

COPY dashboard/ ./
RUN npm run build

# Stage 3: Final runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    python3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 dtruntime

WORKDIR /app

# Copy coordinator binary
COPY --from=rust-builder /app/target/release/coordinator /app/coordinator

# Copy dashboard
COPY --from=dashboard-builder /app/dashboard/dist /app/dashboard

# Copy startup script
COPY scripts/start_services.sh /app/start_services.sh
RUN chmod +x /app/start_services.sh

# Set ownership
RUN chown -R dtruntime:dtruntime /app

# Expose ports
# 50051: gRPC coordinator
# 51051: HTTP API (for dashboard)
# 3000: Dashboard UI
EXPOSE 50051 51051 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:51051/api/health || exit 1

# Default command: start all services
CMD ["/app/start_services.sh"]
