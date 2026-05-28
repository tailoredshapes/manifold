## Multi-stage build for the Manifold suite.
## Build context must be /tank/repos/tailoredshapes so path deps resolve:
##   meshql-rs/, merkql/, manifold/
##
## Build the binary you want via --build-arg APP=<name>:
##   docker build --build-arg APP=groundwork -t manifold/groundwork .
##   docker build --build-arg APP=union      -t manifold/union .
##   docker build --build-arg APP=cityhall   -t manifold/cityhall .
##   docker build --build-arg APP=yard       -t manifold/yard .
##
## Dependency-cache strategy: manifest-staging
##   Phase 1 (DEP CACHE): copy every Cargo.toml + Cargo.lock, create dummy
##   src stubs, run `cargo build --release --workspace`.  This layer is only
##   invalidated when Cargo.lock or a Cargo.toml changes — never by source
##   edits.
##   Phase 2 (REAL BUILD): copy real sources, touch changed files, build -p ${APP}.
##
## Why not cargo-chef?  cargo-chef issue #4 (cross-workspace path deps) has
## been open since 2020 and is unresolved.  manifold path-deps point to
## ../../meshql-rs/* which are outside the manifold workspace root — exactly
## the unsupported case.  Manifest-staging is the proven alternative for this
## repo (see git history).

FROM rust:latest AS builder

WORKDIR /build

# ── Phase 1: Copy Cargo manifests + lock files only ──────────────────────────
# merkql (path dep of meshql-merkql)
COPY merkql/Cargo.toml  /build/merkql/Cargo.toml
COPY merkql/Cargo.lock* /build/merkql/

# meshql-rs workspace manifests
COPY meshql-rs/Cargo.toml  /build/meshql-rs/Cargo.toml
COPY meshql-rs/Cargo.lock* /build/meshql-rs/
COPY meshql-rs/meshql-core/Cargo.toml         /build/meshql-rs/meshql-core/Cargo.toml
COPY meshql-rs/meshql-mongo/Cargo.toml        /build/meshql-rs/meshql-mongo/Cargo.toml
COPY meshql-rs/meshql-graphlette/Cargo.toml   /build/meshql-rs/meshql-graphlette/Cargo.toml
COPY meshql-rs/meshql-restlette/Cargo.toml    /build/meshql-rs/meshql-restlette/Cargo.toml
COPY meshql-rs/meshql-mcp/Cargo.toml          /build/meshql-rs/meshql-mcp/Cargo.toml
COPY meshql-rs/meshql-casbin/Cargo.toml       /build/meshql-rs/meshql-casbin/Cargo.toml
COPY meshql-rs/meshql-server/Cargo.toml       /build/meshql-rs/meshql-server/Cargo.toml
COPY meshql-rs/meshql-cert/Cargo.toml         /build/meshql-rs/meshql-cert/Cargo.toml
COPY meshql-rs/meshql-merkql/Cargo.toml       /build/meshql-rs/meshql-merkql/Cargo.toml
COPY meshql-rs/meshql-merksql/Cargo.toml      /build/meshql-rs/meshql-merksql/Cargo.toml
COPY meshql-rs/meshql-sqlite/Cargo.toml       /build/meshql-rs/meshql-sqlite/Cargo.toml
COPY meshql-rs/meshql-postgres/Cargo.toml     /build/meshql-rs/meshql-postgres/Cargo.toml
COPY meshql-rs/meshql-mysql/Cargo.toml        /build/meshql-rs/meshql-mysql/Cargo.toml
COPY meshql-rs/meshql-lambda/Cargo.toml       /build/meshql-rs/meshql-lambda/Cargo.toml
COPY meshql-rs/meshql-ksql/Cargo.toml         /build/meshql-rs/meshql-ksql/Cargo.toml
COPY meshql-rs/examples/farm/Cargo.toml                    /build/meshql-rs/examples/farm/Cargo.toml
COPY meshql-rs/examples/egg-economy/Cargo.toml             /build/meshql-rs/examples/egg-economy/Cargo.toml
COPY meshql-rs/examples/egg-economy-sap/Cargo.toml         /build/meshql-rs/examples/egg-economy-sap/Cargo.toml
COPY meshql-rs/examples/egg-economy-salesforce/Cargo.toml  /build/meshql-rs/examples/egg-economy-salesforce/Cargo.toml
COPY meshql-rs/examples/egg-economy-lambda/Cargo.toml      /build/meshql-rs/examples/egg-economy-lambda/Cargo.toml
COPY meshql-rs/examples/egg-economy-ksql/Cargo.toml        /build/meshql-rs/examples/egg-economy-ksql/Cargo.toml
COPY meshql-rs/examples/farm-azure/Cargo.toml              /build/meshql-rs/examples/farm-azure/Cargo.toml

# manifold workspace manifests
COPY manifold/Cargo.toml  /build/manifold/Cargo.toml
COPY manifold/Cargo.lock* /build/manifold/
COPY manifold/groundwork/Cargo.toml                           /build/manifold/groundwork/Cargo.toml
COPY manifold/union/Cargo.toml                                /build/manifold/union/Cargo.toml
COPY manifold/cityhall/Cargo.toml                             /build/manifold/cityhall/Cargo.toml
COPY manifold/yard/Cargo.toml                                 /build/manifold/yard/Cargo.toml
COPY manifold/manifold-edge/Cargo.toml                        /build/manifold/manifold-edge/Cargo.toml
COPY manifold/manifold-ingest/Cargo.toml                      /build/manifold/manifold-ingest/Cargo.toml
COPY manifold/manifold-lobby/Cargo.toml                       /build/manifold/manifold-lobby/Cargo.toml
COPY manifold/manifold-ui/Cargo.toml                          /build/manifold/manifold-ui/Cargo.toml
COPY manifold/manifold-integrations/common/Cargo.toml         /build/manifold/manifold-integrations/common/Cargo.toml
COPY manifold/manifold-integrations/catalog-from-github/Cargo.toml  /build/manifold/manifold-integrations/catalog-from-github/Cargo.toml
COPY manifold/manifold-integrations/catalog-from-gitlab/Cargo.toml  /build/manifold/manifold-integrations/catalog-from-gitlab/Cargo.toml
COPY manifold/manifold-integrations/yard-from-github/Cargo.toml     /build/manifold/manifold-integrations/yard-from-github/Cargo.toml
COPY manifold/manifold-integrations/yard-from-gitlab/Cargo.toml     /build/manifold/manifold-integrations/yard-from-gitlab/Cargo.toml
COPY manifold/manifold-integrations/union-from-okta/Cargo.toml      /build/manifold/manifold-integrations/union-from-okta/Cargo.toml

# ── Phase 1: Dummy sources so cargo can resolve + compile all deps ────────────
# merkql
RUN mkdir -p /build/merkql/src \
 && echo "// dummy" > /build/merkql/src/lib.rs

# meshql-rs members (all are libs; mongo/sqlite also have [[bin]] but those
# require optional features so they won't be triggered by a plain build)
RUN mkdir -p \
      /build/meshql-rs/meshql-core/src \
      /build/meshql-rs/meshql-mongo/src \
      /build/meshql-rs/meshql-graphlette/src \
      /build/meshql-rs/meshql-restlette/src \
      /build/meshql-rs/meshql-mcp/src \
      /build/meshql-rs/meshql-casbin/src \
      /build/meshql-rs/meshql-server/src \
      /build/meshql-rs/meshql-cert/src \
      /build/meshql-rs/meshql-merkql/src \
      /build/meshql-rs/meshql-merksql/src \
      /build/meshql-rs/meshql-sqlite/src \
      /build/meshql-rs/meshql-postgres/src \
      /build/meshql-rs/meshql-mysql/src \
      /build/meshql-rs/meshql-lambda/src \
      /build/meshql-rs/meshql-ksql/src \
      /build/meshql-rs/examples/farm/src \
      /build/meshql-rs/examples/egg-economy/src \
      /build/meshql-rs/examples/egg-economy-sap/src \
      /build/meshql-rs/examples/egg-economy-salesforce/src \
      /build/meshql-rs/examples/egg-economy-lambda/src \
      /build/meshql-rs/examples/egg-economy-ksql/src \
      /build/meshql-rs/examples/farm-azure/src \
 && for lib in meshql-core meshql-mongo meshql-graphlette meshql-restlette \
               meshql-mcp meshql-casbin meshql-server meshql-cert \
               meshql-merkql meshql-merksql meshql-sqlite meshql-postgres \
               meshql-mysql meshql-lambda meshql-ksql; do \
      echo "// dummy" > /build/meshql-rs/$lib/src/lib.rs; \
    done \
 && for ex in farm egg-economy egg-economy-sap egg-economy-salesforce \
              egg-economy-lambda egg-economy-ksql farm-azure; do \
      echo "fn main() {}" > /build/meshql-rs/examples/$ex/src/main.rs; \
    done \
 && mkdir -p \
      /build/meshql-rs/meshql-mongo/tests \
      /build/meshql-rs/meshql-merkql/tests \
      /build/meshql-rs/meshql-merksql/tests \
      /build/meshql-rs/meshql-sqlite/tests \
      /build/meshql-rs/meshql-postgres/tests \
      /build/meshql-rs/meshql-mysql/tests \
      /build/meshql-rs/meshql-ksql/tests \
 && for f in \
      meshql-mongo/tests/farm_cert.rs \
      meshql-mongo/tests/repo_cert.rs \
      meshql-mongo/tests/searcher_cert.rs \
      meshql-merkql/tests/farm_cert.rs \
      meshql-merkql/tests/repo_cert.rs \
      meshql-merkql/tests/searcher_cert.rs \
      meshql-merksql/tests/repo_cert.rs \
      meshql-merksql/tests/searcher_cert.rs \
      meshql-sqlite/tests/cross_service_cert.rs \
      meshql-sqlite/tests/egg_economy_cert.rs \
      meshql-sqlite/tests/farm_cert.rs \
      meshql-sqlite/tests/repo_cert.rs \
      meshql-sqlite/tests/searcher_cert.rs \
      meshql-postgres/tests/farm_cert.rs \
      meshql-postgres/tests/repo_cert.rs \
      meshql-postgres/tests/searcher_cert.rs \
      meshql-mysql/tests/farm_cert.rs \
      meshql-mysql/tests/repo_cert.rs \
      meshql-mysql/tests/searcher_cert.rs \
      meshql-ksql/tests/repo_cert.rs \
      meshql-ksql/tests/searcher_cert.rs; do \
      echo "// dummy" > /build/meshql-rs/$f; \
    done

# manifold workspace members
# groundwork, union, cityhall, yard, manifold-lobby, manifold-ingest: lib + bin(s)
# manifold-edge, manifold-ui, manifold-integrations/common: lib only
# manifold-integrations/*: bin only
RUN mkdir -p \
      /build/manifold/groundwork/src/bin \
      /build/manifold/union/src/bin \
      /build/manifold/cityhall/src/bin \
      /build/manifold/yard/src/bin \
      /build/manifold/manifold-edge/src \
      /build/manifold/manifold-ingest/src/bin \
      /build/manifold/manifold-lobby/src/bin \
      /build/manifold/manifold-ui/src \
      /build/manifold/manifold-integrations/common/src \
      /build/manifold/manifold-integrations/catalog-from-github/src \
      /build/manifold/manifold-integrations/catalog-from-gitlab/src \
      /build/manifold/manifold-integrations/yard-from-github/src \
      /build/manifold/manifold-integrations/yard-from-gitlab/src \
      /build/manifold/manifold-integrations/union-from-okta/src \
 && for app in groundwork union cityhall yard; do \
      echo "// dummy" > /build/manifold/$app/src/lib.rs; \
      echo "fn main() {}" > /build/manifold/$app/src/main.rs; \
      echo "fn main() {}" > /build/manifold/$app/src/bin/$app-mcp.rs; \
    done \
 && for app in manifold-ingest manifold-lobby; do \
      echo "// dummy" > /build/manifold/$app/src/lib.rs; \
      echo "fn main() {}" > /build/manifold/$app/src/main.rs; \
      echo "fn main() {}" > /build/manifold/$app/src/bin/$app-mcp.rs; \
    done \
 && echo "// dummy" > /build/manifold/manifold-edge/src/lib.rs \
 && echo "// dummy" > /build/manifold/manifold-ui/src/lib.rs \
 && echo "// dummy" > /build/manifold/manifold-integrations/common/src/lib.rs \
 && for intg in catalog-from-github catalog-from-gitlab yard-from-github \
                yard-from-gitlab union-from-okta; do \
      echo "fn main() {}" > /build/manifold/manifold-integrations/$intg/src/main.rs; \
    done \
 && mkdir -p \
      /build/manifold/groundwork/tests \
      /build/manifold/union/tests \
      /build/manifold/cityhall/tests \
      /build/manifold/yard/tests \
 && for f in \
      groundwork/tests/groundwork_cert.rs \
      groundwork/tests/mcp_harness.rs \
      union/tests/union_cert.rs \
      union/tests/mcp_harness.rs \
      cityhall/tests/cityhall_cert.rs \
      cityhall/tests/mcp_harness.rs \
      yard/tests/yard_cert.rs \
      yard/tests/mcp_harness.rs; do \
      echo "// dummy" > /build/manifold/$f; \
    done

# ── Phase 1: Compile all dependencies (cached layer) ─────────────────────────
WORKDIR /build/manifold
RUN cargo build --release --workspace 2>&1 | tail -5; true

# ── Phase 2: Copy real sources ────────────────────────────────────────────────
# merkql real source
COPY merkql/src /build/merkql/src

# meshql-rs real sources (only the crates manifold actually uses + their deps)
COPY meshql-rs/meshql-core/src        /build/meshql-rs/meshql-core/src
COPY meshql-rs/meshql-graphlette/src  /build/meshql-rs/meshql-graphlette/src
COPY meshql-rs/meshql-restlette/src   /build/meshql-rs/meshql-restlette/src
COPY meshql-rs/meshql-mcp/src         /build/meshql-rs/meshql-mcp/src
COPY meshql-rs/meshql-casbin/src      /build/meshql-rs/meshql-casbin/src
COPY meshql-rs/meshql-server/src      /build/meshql-rs/meshql-server/src
COPY meshql-rs/meshql-cert/src        /build/meshql-rs/meshql-cert/src
COPY meshql-rs/meshql-merkql/src      /build/meshql-rs/meshql-merkql/src
COPY meshql-rs/meshql-sqlite/src      /build/meshql-rs/meshql-sqlite/src

# manifold real sources
COPY manifold/groundwork/src       /build/manifold/groundwork/src
COPY manifold/groundwork/config    /build/manifold/groundwork/config
COPY manifold/groundwork/static    /build/manifold/groundwork/static
COPY manifold/union/src            /build/manifold/union/src
COPY manifold/union/config         /build/manifold/union/config
COPY manifold/union/static         /build/manifold/union/static
COPY manifold/cityhall/src         /build/manifold/cityhall/src
COPY manifold/cityhall/config      /build/manifold/cityhall/config
COPY manifold/cityhall/static      /build/manifold/cityhall/static
COPY manifold/yard/src             /build/manifold/yard/src
COPY manifold/yard/config          /build/manifold/yard/config
COPY manifold/yard/static          /build/manifold/yard/static
COPY manifold/manifold-edge/src    /build/manifold/manifold-edge/src
COPY manifold/manifold-ingest/src  /build/manifold/manifold-ingest/src
COPY manifold/manifold-ingest/config /build/manifold/manifold-ingest/config
COPY manifold/manifold-lobby/src   /build/manifold/manifold-lobby/src
COPY manifold/manifold-lobby/config /build/manifold/manifold-lobby/config
COPY manifold/manifold-lobby/static /build/manifold/manifold-lobby/static
COPY manifold/manifold-ui/src      /build/manifold/manifold-ui/src
COPY manifold/manifold-ui/static   /build/manifold/manifold-ui/static
COPY manifold/manifold-integrations/common/src          /build/manifold/manifold-integrations/common/src
COPY manifold/manifold-integrations/catalog-from-github/src  /build/manifold/manifold-integrations/catalog-from-github/src
COPY manifold/manifold-integrations/catalog-from-gitlab/src  /build/manifold/manifold-integrations/catalog-from-gitlab/src
COPY manifold/manifold-integrations/yard-from-github/src     /build/manifold/manifold-integrations/yard-from-github/src
COPY manifold/manifold-integrations/yard-from-gitlab/src     /build/manifold/manifold-integrations/yard-from-gitlab/src
COPY manifold/manifold-integrations/union-from-okta/src      /build/manifold/manifold-integrations/union-from-okta/src

# Touch all .rs files so cargo sees them as newer than the dummy-compiled artifacts
RUN find /build -name "*.rs" -exec touch {} +

# ── Phase 2: Build the requested app ─────────────────────────────────────────
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
