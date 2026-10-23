# Session server image (CLAUDE.md §3): build with the pinned toolchain, run on distroless as a
# non-root user. Build from the repository root:
#   docker build -f apps/session-server/Dockerfile -t dnd-session-server .

FROM rust:1-slim-bookworm AS build
WORKDIR /src
# The toolchain of rust-toolchain.toml is installed on first use.
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p dnd-session-server \
    && cp target/release/dnd-session-server /dnd-session-server \
    && mkdir -p /data

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /dnd-session-server /usr/local/bin/dnd-session-server
# Owned by the image's non-root user, so a fresh named volume is writable.
COPY --from=build --chown=nonroot:nonroot /data /data
ENV DND_DATA_DIR=/data \
    DND_BIND=0.0.0.0:8080 \
    RUST_LOG=info
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
    CMD ["/usr/local/bin/dnd-session-server", "healthcheck"]
ENTRYPOINT ["/usr/local/bin/dnd-session-server"]
