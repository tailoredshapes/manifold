# Path-based deployment for Manifold

**Date:** 2026-06-01
**Status:** Approved, implementing

## Problem

Manifold currently assumes each app owns an origin (`groundwork.tildarc.com`,
`union.tildarc.com`, …). Enterprise landing zones often can't or won't
provision a subdomain per app (or a wildcard cert), and need the whole suite
behind a single hostname with per-app path prefixes:

```
https://manifold.client.com/groundwork/…
https://manifold.client.com/union/…
https://manifold.client.com/cityhall/…
```

This must be a deployment option — domain-per-app mode (the tildarc.com
default) keeps working unchanged.

## What breaks under a prefix

Two URL layers exist:

1. **Server-to-server** (`UNION_URL=http://union:3000`) — internal Docker
   network, used by federation resolvers. **Unaffected** by path mode.
2. **Browser-facing**:
   - `*_PUBLIC_URL` → `/config.json` → `crossLink()` builds cross-app
     `<a href>`s. Already env-driven and already tolerates a path in the base.
   - Each app's frontend fetches its **own** data via **root-absolute** paths
     (`/deployable/graph`, `/deployable/api`, `/config.json`) and references
     assets via `/static/…`, plus `app.js` imports the shared lib via the
     leading-slash specifier `from '/static/manifold-ui.js'`.

Under `https://host/groundwork/`, every root-absolute reference resolves
against the origin root (`https://host/static/app.js`, `https://host/deployable/graph`)
— no edge handler — so nothing loads. This is the work.

## Approach A — runtime base detection (chosen)

The frontend derives its own base from where it was loaded; no env var, no
server templating, and the edge and app can never disagree because the app
reads reality.

### 1. Base detection in `manifold-ui.js`

```js
// import.meta.url === "https://host/groundwork/static/manifold-ui.js"  (path mode)
//                 === "https://groundwork.tildarc.com/static/manifold-ui.js" (domain mode)
const APP_BASE = import.meta.url.replace(/\/static\/manifold-ui\.js.*$/, '');
export function apiUrl(path) {
  if (/^https?:\/\//.test(path)) return path;            // absolute cross-app URL — leave alone
  return path.startsWith('/') ? APP_BASE + path : path;  // root-relative → prefix
}
```

`apiFetch`, `gqlQuery`, and `loadManifoldConfig`'s default route every
root-relative path through `apiUrl()`. Absolute cross-app URLs pass through
untouched. **Domain mode is byte-identical**: `APP_BASE` is the origin, so
`apiUrl('/x/graph')` equals today's `/x/graph`.

### 2. Frontend reference fixes

- 5× `index.html`: `/static/…` → `static/…` (relative). Relies on a
  trailing-slash served path (see edge redirect). Identical at `/` in domain mode.
- 5× `app.js`: `from '/static/manifold-ui.js'` → `from './manifold-ui.js'` —
  module specifiers resolve against the module's own URL, so this is
  prefix-agnostic and needs no base tag.
- Hub link (`https://manifold.tildarc.com`, hardcoded in 3 HTMLs): driven from
  a new `manifold_public_url` in `/config.json`, set on boot.

### 3. Lobby cleanup

`manifold-lobby/app.js` has 7 hardcoded `https://{app}.tildarc.com/#…` links
that bypass the config mechanism. Replace with `crossLink(...)` — exactly the
domain-coupling being removed.

### 4. Edge + deployment config

- **Dev** (`Caddyfile.dev`): already strips prefixes; add per-app
  `handle /groundwork` → 308 redirect to `/groundwork/` so relative assets
  resolve.
- **Prod**: add `caddy/Caddyfile.single-origin.example` — one hostname,
  `handle_path /app/*` → container, with real edge auth.
- `crossLink()`: emit a trailing slash before the `#hash`.
- `*_PUBLIC_URL`: no code change; in path mode set to `https://host/groundwork`
  etc. Add `MANIFOLD_PUBLIC_URL`.

### 5. Rust footprint

Only add `manifold_public_url` to the `/config.json` body in each app's
`main.rs`. Static/graph/api routes are unchanged because the edge strips the
prefix before forwarding.

## Scope notes

- The landing page itself (`manifold.tildarc.com`, served outside this
  compose) is out of scope; only the link to it becomes config-driven.
- The lobby hardcoded-link cleanup is folded into this work.

## Testing

- Unit: `apiUrl()` — root-relative prefixing, absolute pass-through,
  domain-mode identity.
- Manual: `docker-compose up`, hit `http://localhost:8090/groundwork/`, confirm
  assets load, data fetches resolve, cross-app links land correctly, bare
  `/groundwork` redirects to `/groundwork/`; confirm domain-mode (direct
  container port) still works.
