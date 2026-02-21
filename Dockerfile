# ---------- Build stage ----------
FROM rust:1.83-bookworm AS builder

WORKDIR /app

# Copy manifests first for layer caching of dependency builds.
COPY Cargo.toml Cargo.lock ./
COPY crates/hls-core/Cargo.toml crates/hls-core/Cargo.toml
COPY crates/hls-api/Cargo.toml  crates/hls-api/Cargo.toml
COPY crates/hls-cli/Cargo.toml  crates/hls-cli/Cargo.toml

# Create dummy source files so cargo can resolve the workspace and fetch deps.
RUN mkdir -p crates/hls-core/src crates/hls-api/src crates/hls-cli/src && \
    echo "pub fn _dummy() {}" > crates/hls-core/src/lib.rs && \
    echo "pub fn _dummy() {}" > crates/hls-api/src/lib.rs && \
    echo "fn main() {}" > crates/hls-cli/src/main.rs

# Build dependencies only (cached layer).
RUN cargo build --release --bin hls-monitor 2>/dev/null || true

# Now copy real source code.
COPY crates/ crates/

# Touch the real source files so cargo recompiles them (but not deps).
RUN touch crates/hls-core/src/lib.rs crates/hls-api/src/lib.rs crates/hls-cli/src/main.rs

# Build the final binary.
RUN cargo build --release --bin hls-monitor

# ---------- Runtime stage ----------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/hls-monitor /usr/local/bin/hls-monitor

# Non-root user for security.
RUN useradd -m -s /bin/false appuser
USER appuser

EXPOSE 8080

ENTRYPOINT ["hls-monitor"]
CMD ["serve", "--listen", "0.0.0.0:8080"]
