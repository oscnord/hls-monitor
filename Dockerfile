# ---------- Build stage ----------
FROM rust:1.83-bookworm AS builder

WORKDIR /app

# Copy manifests first for layer caching of dependency builds.
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir -p src && \
    echo "pub fn _dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > src/main.rs

# Build dependencies only (cached layer).
RUN cargo build --release --bin hls-monitor 2>/dev/null || true

# Now copy real source code.
COPY src/ src/
COPY tests/ tests/

# Touch the real source files so cargo recompiles them (but not deps).
RUN touch src/lib.rs src/main.rs

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
