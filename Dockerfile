# ============================================================
# LogMaker Docker Image — Multi-stage build
# Stage 1: Build the web UI (Node)
# Stage 2: Build the server with the UI embedded (Rust)
# Stage 3: Minimal runtime
#
# Build: docker buildx build --platform linux/amd64 -t logmaker .
# Run:   docker run -p 19999:19999 logmaker
# ============================================================

# ── Stage 1: UI ─────────────────────────────────────────────
FROM --platform=$BUILDPLATFORM node:22-bookworm-slim AS ui

WORKDIR /app/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui ./
# Writes the static site to /app/core/static
RUN npm run build

# ── Stage 2: Server ─────────────────────────────────────────
FROM rust:1.90-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY plugin-api plugin-api
COPY default-plugin default-plugin
COPY core core
COPY examples examples
COPY --from=ui /app/core/static core/static

RUN cargo build --release --locked -p logmaker-core

# ── Stage 3: Runtime ────────────────────────────────────────
FROM debian:bookworm-slim

LABEL maintainer="LogMaker" \
      description="LogMaker — Log Generation & Delivery Platform"

WORKDIR /app

# Create least-privilege runtime user and writable directories
RUN groupadd --system logmaker \
    && useradd --system --gid logmaker --home-dir /app --shell /usr/sbin/nologin logmaker \
    && mkdir -p /app/data /app/plugins /app/logs \
    && chown -R logmaker:logmaker /app

COPY --from=builder --chown=logmaker:logmaker /app/target/release/logmaker /app/logmaker

ENV LOGMAKER_DATA_ROOT=/app/data \
    LOGMAKER_PLUGIN_ROOT=/app/plugins \
    LOGMAKER_LOG_DIR=/app/logs \
    RUST_LOG=info

EXPOSE 19999

USER logmaker

ENTRYPOINT ["/app/logmaker"]
