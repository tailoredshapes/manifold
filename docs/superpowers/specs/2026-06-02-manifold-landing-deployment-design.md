# Deploy manifold-landing as a 7th App Service app (config-driven)

**Date:** 2026-06-02
**Status:** Approved, implementing

## Problem

The "Manifold" hub link in each app should point at the client's static
landing page. `manifold-landing/` exists but is a **lone `index.html`** — no
Dockerfile, no CI, not in the publish matrix, hand-served on tildarc — and it
**hardcodes `*.tildarc.com`** for its app tiles and footer. So there's no way
it lands in National Grid QA, and as-is its tiles would send NG users back to
tildarc.

A storage-static-website ("Azure web page") host was considered and rejected:
the NG landing zone is strict (naming Deny + 18 mandatory tags) and very likely
Denies public-access storage. Not worth arguing for new tech. Reuse the proven
App Service + ACR-token pattern instead.

## Design

### 1. Landing becomes a tiny Rust crate (a stripped manifold-lobby)

The shared `manifold/Dockerfile` builds any workspace member via
`--build-arg APP=<name>` → `cargo build -p $APP` → `target/release/$APP` as the
entrypoint, copying `manifold/<app>/static`. So the lowest-friction
"static-server image" is a `manifold-landing` workspace crate that serves:

- `/` → its `index.html`
- `/config.json` → the app public URLs from env

on `PORT` (3000). No graphlettes, no restlettes, **no auth** (public hub of
links). ~40 lines of axum mirroring what manifold-lobby already does.

### 2. Config-driven links (removes tildarc hardcoding)

`/config.json` is built from env — `GROUNDWORK_PUBLIC_URL`, `UNION_PUBLIC_URL`,
`CITYHALL_PUBLIC_URL`, `YARD_PUBLIC_URL`, `LOBBY_PUBLIC_URL` — with the same
tildarc-subdomain defaults every other app uses (so `manifold.tildarc.com`
keeps working). The inline JS in `index.html` fetches `config.json` and sets
the 5 tile hrefs; `manifold-ingest` (headless) is not shown. NG overrides the
env via conduit `app_settings`.

### 3. manifold repo changes

- Add `manifold-landing` to workspace `members`.
- Move `manifold-landing/index.html` → `manifold-landing/static/index.html`.
- Add `manifold-landing/Cargo.toml` + `src/main.rs`.
- Rewrite the inline link map in `index.html` to read `/config.json`.
- Add `COPY manifold/manifold-landing/static …` to the Dockerfile.
- Add `manifold-landing` to the `publish-images` matrix in `.gitlab-ci.yml`.
- Cut **v0.1.3** — republishes all 7 images at one version (no skew).

### 4. conduit changes (existing for_each does the wiring)

- Add one block to `local.apps`:
  `"manifold-landing" = { image_tag = "v0.1.3", env_fragment = "MANIFOLD" }`.
- `for_each` auto-creates the Web App + ACR token + diagnostics. The cross-ref
  `app_settings` logic auto-injects `MANIFOLD_PUBLIC_URL` (= landing's
  `azurewebsites.net` URL) onto every other app — wiring the hub link — and
  injects the other apps' `*_PUBLIC_URL` onto landing for its tiles.
- Bump every `image_tag` to `v0.1.3` (supersedes the earlier v0.1.2 bump).

## Verification

- `cargo build -p manifold-landing`; run locally; `curl /` and `/config.json`;
  confirm tiles render from config and env override works; headless-browser
  check of rendered hrefs.
- conduit: `terraform fmt` + `validate`; read-only `plan` offered before any
  apply. Apply to NG QA only on explicit go-ahead.

## Out of scope

- Client-specific branding/content on the landing (ships standard content).
- Public-storage / Static Web Apps hosting (rejected above).
