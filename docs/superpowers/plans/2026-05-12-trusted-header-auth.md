# Trusted-header auth — implementation plan

**Goal:** ship the design in `specs/2026-05-12-trusted-header-auth-design.md`.

**Architecture:** Caddy at edge authenticates; injects trusted headers; manifold-edge middleware lifts headers into Stash; meshql-casbin wraps StashKeyAuth and resolves roles via Casbin policy.

**Tech:** Rust 2021, axum, tokio, `casbin = "2.20"`, jcasbin/casbin-node parity.

**Repo split:** meshql-rs land first (new crate + small core addition), then manifold consumes via path-dep.

---

## Phase 1 — meshql-rs Casbin support

### Task 1.1: `StashKeyAuth` in meshql-core

**Files:** `meshql-rs/meshql-core/src/auth.rs` (extend); `meshql-rs/meshql-core/tests/auth_tests.rs` (new)

- [ ] Add `StashKeyAuth { key: String }` impl of `Auth`. `get_auth_token` returns `vec![stash.get(&key).to_string()]` (or empty if missing). `is_authorized` returns `true` (it's a leaf — composed Auth wraps it).
- [ ] Test: stash with key → `["alice"]`; stash without key → `[]`.
- [ ] Commit: `feat(meshql-core): StashKeyAuth — Auth leaf that reads identity from a Stash key`

### Task 1.2: scaffold `meshql-casbin` crate

**Files:** `meshql-rs/Cargo.toml` (workspace member), `meshql-rs/meshql-casbin/Cargo.toml`, `meshql-rs/meshql-casbin/src/lib.rs`

- [ ] New workspace member; deps: `meshql-core` (path), `casbin = "2.20"`, `tokio`, `async-trait`, `thiserror`.
- [ ] Empty `lib.rs` with module docs that point at the canonical Java/TS impls.
- [ ] `cargo build -p meshql-casbin` clean.
- [ ] Commit: `feat(meshql-casbin): scaffold crate (parallels meshql/auth/casbin and meshobj/core/casbin_auth)`

### Task 1.3: `CasbinAuth<A>` implementation + tests parallel to canonical

**Files:** `meshql-rs/meshql-casbin/src/lib.rs`; `meshql-rs/meshql-casbin/tests/casbin_auth.rs`; `meshql-rs/meshql-casbin/tests/fixtures/{model.conf,policy.csv}`

- [ ] Implement `CasbinAuth<A: Auth>` with `new(model_path, policy_path, inner) -> Result<Self, casbin::Error>`.
- [ ] `get_auth_token`: delegate to inner, then `enforcer.get_roles_for_user(user_id, None)`.
- [ ] `is_authorized`: empty `authorized_tokens` → true; else any-overlap.
- [ ] Test fixtures: standard RBAC `model.conf`, policy with `g, alice, admin` + `g, bob, editor`.
- [ ] Tests (parallel to `CasbinAuthTest.java` / `casbin.spec.ts`):
  - initialize from model+policy
  - `get_auth_token` returns roles for known user
  - `get_auth_token` returns empty for unknown user
  - `is_authorized` true when credential overlaps `authorized_tokens`
  - `is_authorized` false when no overlap
  - `is_authorized` true when `authorized_tokens` empty
- [ ] `cargo test -p meshql-casbin` all green.
- [ ] Commit: `feat(meshql-casbin): CasbinAuth wrapper with role-resolution + tests`

### Task 1.4: push meshql-rs main

- [ ] Pre-push hook green (testcontainer crates serialized as before).
- [ ] `git push origin main` from meshql-rs.

---

## Phase 2 — manifold-edge middleware

### Task 2.1: `manifold-edge` crate

**Files:** `manifold/Cargo.toml` (member), `manifold/manifold-edge/Cargo.toml`, `manifold/manifold-edge/src/lib.rs`, `manifold/manifold-edge/tests/middleware.rs`

- [ ] New workspace member. Deps: `axum`, `tower`, `tokio`, `meshql-core` (path-dep `../../meshql-rs/meshql-core`), `stash` (whatever meshql uses).
- [ ] Exports `HeaderConfig { user_id_header: String, groups_header: String }` and `header_identity_layer(cfg)` → `tower::Layer` that, on every request:
  - reads configured headers
  - writes their values into request-extension Stash under canonical keys (`user_id`, `groups`)
- [ ] Tests: request with both headers → stash populated; request without → stash empty (no middleware error).
- [ ] `cargo test -p manifold-edge` green.
- [ ] Commit: `feat(manifold-edge): axum middleware that lifts trusted headers into request Stash`

---

## Phase 3 — wire CasbinAuth into each service

Per-service: groundwork (canary first), then union, cityhall, yard.

### Task 3.1: groundwork

**Files:** `manifold/groundwork/Cargo.toml`, `manifold/groundwork/src/main.rs`, `manifold/groundwork/config/{model.conf,policy.csv}`

- [ ] Add deps: `meshql-casbin` (path), `manifold-edge` (path).
- [ ] Ship `config/model.conf` (standard RBAC) and `config/policy.csv` (mostly-permissive: `g, alice@example.dev, admin` + `p, admin, *, *`).
- [ ] In `main.rs`: read `MANIFOLD_AUTH_MODEL_PATH` / `MANIFOLD_AUTH_POLICY_PATH` env vars (defaults: `./config/model.conf`, `./config/policy.csv`); read `MANIFOLD_USER_HEADER` / `MANIFOLD_GROUPS_HEADER` env vars (defaults: `X-Manifold-User-Id`, `X-Manifold-User-Groups`).
- [ ] Wire `CasbinAuth::new(model, policy, StashKeyAuth::new("user_id")).await?` into meshql-server config in place of `NoAuth`.
- [ ] Add the `header_identity_layer` to the axum router.
- [ ] Smoke: existing cucumber tests pass (fixture data has `authorized_tokens: []` → public, still readable).
- [ ] Commit: `feat(groundwork): wire CasbinAuth + header_identity middleware`

### Task 3.2: union — same shape as 3.1
### Task 3.3: cityhall — same shape as 3.1
### Task 3.4: yard — same shape as 3.1

After each: commit + `cargo test -p <app>` green.

---

## Phase 4 — Caddy templates + docker-compose

### Task 4.1: dev Caddyfile + compose

**Files:** `manifold/caddy/Caddyfile.dev`, `manifold/docker-compose.yml`

- [ ] `Caddyfile.dev`: listens on `:8080`; reverse-proxies `/groundwork/* → :3050`, `/union/* → :3051`, `/cityhall/* → :3052`, `/yard/* → :3053`; injects `X-Manifold-User-Id: alice@example.dev`, `X-Manifold-User-Groups: admin` on every upstream request.
- [ ] `docker-compose.yml`: add `caddy` service mounting `./caddy/Caddyfile.dev` at `:80`; expose `:8080`. Keep direct ports on the four services for now (so MCP servers can keep hitting them unauthenticated).
- [ ] Smoke: `curl http://localhost:8080/groundwork/api/...` reaches groundwork with synthetic identity.
- [ ] Commit: `feat(caddy): dev gateway injects synthetic identity headers`

### Task 4.2: Azure Entra example

**Files:** `manifold/caddy/Caddyfile.azure-entra.example`

- [ ] Template Caddyfile mapping `X-MS-CLIENT-PRINCIPAL-NAME` → `X-Manifold-User-Id`, etc. Include comments on Entra easy-auth context.
- [ ] No deployment yet — just the template for first-customer kickoff.
- [ ] Commit: `docs(caddy): Azure Entra Caddyfile template for first-customer deployment`

---

## Phase 5 — End-to-end verification

- [ ] Bring up `docker compose up`. Caddy on `:8080`, four services on `:3050-3053`.
- [ ] Through Caddy: `curl localhost:8080/groundwork/graph -d '{"query":"{ getAll { id name } }"}'` returns data (auth header injected by Caddy, role resolved, fixture has empty `authorized_tokens` so allowed).
- [ ] Direct (no Caddy): `curl localhost:3050/graph` — should also work in dev mode (no header → empty user → empty roles → still passes the empty-`authorized_tokens` check on fixture data).
- [ ] Negative test: temporarily set an Envelope's `authorized_tokens: ["editor"]`; verify alice (admin) is denied, then change policy so admin matches editor, verify allowed.
- [ ] Commit `docs/superpowers/plans/...` checked off, plus a short "what was done" note in memory.

---

## Open questions / deferred

- Outer landing app behind Caddy (decision: served by Caddy with no auth — public landing page).
- MCP server auth (out of scope this round).
- Real Casbin model with finer-grained resource/action rules (v1 ships with `p, admin, *, *`).
