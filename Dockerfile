# opencut-mcp — Streamable HTTP service for hosted MCP clients (Cowork, etc.)
# Local stdio use (Claude Code, Claude Desktop) doesn't need this image at all;
# it runs the compiled binary directly. This is only for the hosted path.

FROM rust:1-bookworm AS builder
WORKDIR /app

# Workspace-wide files first so dependency resolution is cached across builds
# even when only opencut-mcp's own source changes.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/desktop ./apps/desktop

# -p scopes the actual compile to opencut-mcp and what it depends on
# (opencut-core). apps/desktop (the GPUI desktop app) is a workspace member
# so its Cargo.toml must exist for workspace resolution, but nothing in
# opencut-mcp's dependency graph reaches it, so it is never compiled here.
RUN cargo build --release -p opencut-mcp

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/opencut-mcp /usr/local/bin/opencut-mcp

ENV OPENCUT_MCP_TRANSPORT=http
# OPENCUT_MCP_BEARER_TOKEN is required at startup and has no default — set it
# on the Railway service, never bake it into the image.
EXPOSE 8090

CMD ["opencut-mcp"]
