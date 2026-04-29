## Multi-stage build for the Manifold suite.
## Build context must be /tank/repos/tailoredshapes so path deps resolve:
##   meshql-rs/, merkql/, manifold/
##
## Build the binary you want via --build-arg APP=<name>:
##   docker build --build-arg APP=groundwork -t manifold/groundwork .
##   docker build --build-arg APP=union      -t manifold/union .
##   docker build --build-arg APP=cityhall   -t manifold/cityhall .

FROM rust:latest AS builder

WORKDIR /build

# Copy path dependencies
COPY meshql-rs /build/meshql-rs
COPY merkql    /build/merkql

# Copy manifold workspace
COPY manifold/Cargo.toml  manifold/Cargo.lock* /build/manifold/
COPY manifold/groundwork/Cargo.toml            /build/manifold/groundwork/Cargo.toml
COPY manifold/union/Cargo.toml                 /build/manifold/union/Cargo.toml
COPY manifold/cityhall/Cargo.toml              /build/manifold/cityhall/Cargo.toml
COPY manifold/yard/Cargo.toml                  /build/manifold/yard/Cargo.toml

WORKDIR /build/manifold

# Dummy sources to cache dep builds
RUN mkdir -p groundwork/src union/src cityhall/src yard/src \
 && echo "fn main() {}" > groundwork/src/main.rs \
 && echo "fn main() {}" > union/src/main.rs \
 && echo "fn main() {}" > cityhall/src/main.rs \
 && echo "" > cityhall/src/lib.rs \
 && echo "fn main() {}" > yard/src/main.rs \
 && echo "" > yard/src/lib.rs

RUN cargo build --release --workspace 2>/dev/null; true

# Copy real sources + assets
COPY manifold/groundwork/src      /build/manifold/groundwork/src
COPY manifold/groundwork/config   /build/manifold/groundwork/config
COPY manifold/groundwork/static   /build/manifold/groundwork/static
COPY manifold/union/src           /build/manifold/union/src
COPY manifold/union/config        /build/manifold/union/config
COPY manifold/union/static        /build/manifold/union/static
COPY manifold/cityhall/src        /build/manifold/cityhall/src
COPY manifold/cityhall/config     /build/manifold/cityhall/config
COPY manifold/cityhall/static     /build/manifold/cityhall/static
COPY manifold/yard/src            /build/manifold/yard/src
COPY manifold/yard/config         /build/manifold/yard/config
COPY manifold/yard/static         /build/manifold/yard/static

# Force re-build of the real sources.
RUN find groundwork/src union/src cityhall/src yard/src -name '*.rs' -exec touch {} +

ARG APP=groundwork
RUN cargo build --release -p ${APP}

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

ARG APP=groundwork

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/manifold/target/release/${APP} /usr/local/bin/manifold-app

RUN useradd -m manifold && mkdir /data && chown manifold:manifold /data
USER manifold

VOLUME ["/data"]

ENV DATA_DIR=/data

EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/manifold-app"]
