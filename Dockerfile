# ============================================================
# LogMaker Docker Image — Multi-stage build
# Stage 1: Build the web UI (Node)
# Stage 2: Build the server with the UI embedded (Rust)
# Stage 3: Minimal runtime
#
# Build: docker buildx build --platform linux/amd64 -t logmaker .
#        Platforms: linux/amd64, linux/arm64 (the server is cross-compiled, so
#        either builds quickly on any host)
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
# Runs on the build machine and cross-compiles for the target platform,
# which is much faster than compiling under emulation.
FROM --platform=$BUILDPLATFORM rust:1.90-bookworm AS builder

ARG BUILDARCH
ARG TARGETARCH
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) triple=x86_64-unknown-linux-gnu; gnu=x86_64-linux-gnu ;; \
      arm64) triple=aarch64-unknown-linux-gnu; gnu=aarch64-linux-gnu ;; \
      *) echo "unsupported platform: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    if [ "$TARGETARCH" != "$BUILDARCH" ]; then \
      apt-get update; \
      apt-get install -y --no-install-recommends "gcc-$(echo "$gnu" | tr _ -)" "libc6-dev-$TARGETARCH-cross"; \
      rm -rf /var/lib/apt/lists/*; \
      rustup target add "$triple"; \
      printf '[target.%s]\nlinker = "%s-gcc"\n' "$triple" "$gnu" >> "$CARGO_HOME/config.toml"; \
    fi; \
    echo "$triple" > /target

WORKDIR /app
COPY Cargo.toml Cargo.lock rustfmt.toml ./
COPY plugin-api plugin-api
COPY default-plugin default-plugin
COPY core core
COPY examples examples
COPY --from=ui /app/core/static core/static

RUN cargo build --release --locked -p logmaker-core --target "$(cat /target)" \
    && cp "target/$(cat /target)/release/logmaker" /logmaker

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

COPY --from=builder /logmaker /app/logmaker

# Logs go to stdout only (docker logs / kubectl logs); set LOGMAKER_LOG_DIR
# to also write daily files.
ENV LOGMAKER_DATA_ROOT=/app/data \
    LOGMAKER_PLUGIN_ROOT=/app/plugins \
    LOGMAKER_LOG_DIR="" \
    RUST_LOG=info

EXPOSE 19999

USER 10001:10001

ENTRYPOINT ["/app/logmaker"]
