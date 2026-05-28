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

# Copy the whole manifold workspace — simpler than per-package staging
# now that we have 7 workspace members and integration adapters too.
COPY manifold /build/manifold

WORKDIR /build/manifold

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
