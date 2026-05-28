# ── Stage 1: Builder ─────────────────────────────────────────────────────────
# CI can override the target via CARGO_BUILD_TARGET (see .cargo/config.toml).
# In Docker (Linux) we use x86_64-unknown-linux-gnu instead of windows-gnu.
FROM rust:1.78-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc \
    libc6-dev \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Override Windows-pinned target with Linux target
ENV CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu

# Cache dependencies by copying manifests first
COPY Cargo.toml Cargo.lock ./
# Create a dummy main so cargo can resolve & fetch deps
RUN mkdir src && echo 'fn main(){}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Copy real source and build
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/x86_64-unknown-linux-gnu/release/airp-core .

# Data directory (mount volume here in production)
RUN mkdir -p /app/data

# Default port
EXPOSE 8000

# DX-2: set AIRP_ACCESS_KEY env var to enable auth (empty = no auth)
ENV AIRP_DATA_DIR=/app/data
ENV AIRP_LOG=info

ENTRYPOINT ["/app/airp-core"]
CMD ["daemon", "--port", "8000"]
