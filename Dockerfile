## Multi-stage build for Groundwork.
## Build context must be /tank/repos/tailoredshapes so path deps resolve:
##   meshql-rs/, merkql/, manifold/

FROM rust:latest AS builder

WORKDIR /build

# Copy path dependencies
COPY meshql-rs /build/meshql-rs
COPY merkql    /build/merkql

# Copy manifold workspace
COPY manifold/Cargo.toml  manifold/Cargo.lock* /build/manifold/
COPY manifold/groundwork/Cargo.toml            /build/manifold/groundwork/Cargo.toml

WORKDIR /build/manifold

# Dummy sources to cache dep builds
RUN mkdir -p groundwork/src && echo "fn main() {}" > groundwork/src/main.rs

RUN cargo build --release --workspace 2>/dev/null; true

# Copy real sources + assets
COPY manifold/groundwork/src         /build/manifold/groundwork/src
COPY manifold/groundwork/config      /build/manifold/groundwork/config
COPY manifold/groundwork/static      /build/manifold/groundwork/static

# Touch main.rs so cargo sees the change
RUN touch groundwork/src/main.rs

RUN cargo build --release -p groundwork

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/manifold/target/release/groundwork /usr/local/bin/groundwork

RUN useradd -m groundwork && mkdir /data && chown groundwork:groundwork /data
USER groundwork

VOLUME ["/data"]

ENV PORT=3000
ENV DATA_DIR=/data

EXPOSE 3000

ENTRYPOINT ["groundwork"]
