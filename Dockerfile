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

# Least-privilege runtime user. The fixed numeric UID/GID lets Kubernetes
# check runAsNonRoot and grant volume access with fsGroup: 10001.
RUN groupadd --gid 10001 logmaker \
    && useradd --uid 10001 --gid logmaker --home-dir /app --no-create-home --shell /usr/sbin/nologin logmaker \
    && mkdir -p /app/data /app/plugins \
    && chown -R logmaker:logmaker /app

COPY --from=builder /app/target/release/logmaker /app/logmaker

# Logs go to stdout only (docker logs / kubectl logs); set LOGMAKER_LOG_DIR
# to also write daily files.
ENV LOGMAKER_DATA_ROOT=/app/data \
    LOGMAKER_PLUGIN_ROOT=/app/plugins \
    LOGMAKER_LOG_DIR="" \
    RUST_LOG=info

EXPOSE 19999

USER 10001:10001

ENTRYPOINT ["/app/logmaker"]
