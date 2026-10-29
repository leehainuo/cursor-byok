# syntax=docker/dockerfile:1.7

FROM node:22-bookworm-slim AS web
WORKDIR /src/apps/desktop
COPY apps/desktop/package.json apps/desktop/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY apps/desktop/index.html apps/desktop/tsconfig.json apps/desktop/tsconfig.node.json apps/desktop/vite.config.ts ./
COPY apps/desktop/plugins/ plugins/
COPY apps/desktop/public/ public/
COPY apps/desktop/src/ src/
RUN npm run build

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY server/ server/
COPY apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.toml
COPY apps/desktop/src-tauri/build.rs apps/desktop/src-tauri/build.rs
COPY apps/desktop/src-tauri/src/ apps/desktop/src-tauri/src/
COPY apps/desktop/src-tauri/capabilities/ apps/desktop/src-tauri/capabilities/
COPY apps/desktop/src-tauri/icons/ apps/desktop/src-tauri/icons/
COPY apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/tauri.conf.json
COPY protocols/cursor/ protocols/cursor/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --package cursor-server --bin cursor-server && \
    cp target/release/cursor-server /tmp/cursor-server

FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install --yes --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --system --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin cursor-byok && \
    mkdir -p /app/console /data && \
    chown cursor-byok:cursor-byok /data

COPY --from=server /tmp/cursor-server /usr/local/bin/cursor-server
COPY --from=web /src/apps/desktop/dist/ /app/console/

ENV CURSOR_LISTEN_ADDR=0.0.0.0:3000 \
    CURSOR_DATABASE_URL=sqlite:///data/cursor-server.db \
    CURSOR_CONSOLE_DIR=/app/console \
    RUST_LOG=cursor_server=info

USER cursor-byok
EXPOSE 3000
VOLUME ["/data"]
HEALTHCHECK --interval=10s --timeout=3s --retries=5 \
    CMD curl --fail --silent http://127.0.0.1:3000/__byok-api__/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/cursor-server"]
