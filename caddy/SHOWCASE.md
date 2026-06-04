# Public Manifold showcase — deploy runbook

Target: a public, read-only marketing demo at `https://manifold.tailoredshapes.com`,
running on AWS behind Cloudflare, seeded with the synthetic **Meridian Freight
Solutions** dataset. No client data; no SSO.

This is the **manifold-side** package. The infra (the `atlas` environment —
AWS + Cloudflare) is owned separately; everything atlas needs is below.

## Shape

```
browser → Cloudflare (TLS, proxy) → edge (Caddy, path-mode) → /<app>/ → app:3000
```

- **Images:** `registry.tildarc.com/tailoredshapes/manifold/<app>:v0.1.4`
  (groundwork, union, cityhall, yard, manifold-ingest, manifold-lobby).
- **Edge:** `caddy/Caddyfile.showcase.example` — path-mode, public, stamps a
  fixed **read-only** demo identity (`viewer`) onto every request. Visitors
  browse the whole graph; they cannot mutate it. No policy/image change needed
  (the `viewer` role ships in each app's `policy.csv`).
- **Data:** `data/meridian_fixture.json` (synthetic), loaded by
  `data/load_fixture.py`.

## Per-app environment

Internal (service-to-service, AWS-internal DNS — adjust to the atlas network):

```
PORT=3000
DATA_DIR=/data
GROUNDWORK_URL=http://groundwork:3000      # set the peers each app federates to
UNION_URL=http://union:3000
CITYHALL_URL=http://cityhall:3000
YARD_URL=http://yard:3000
```

Public path-form URLs (drive cross-app links; set the peers each app emits):

```
GROUNDWORK_PUBLIC_URL=https://manifold.tailoredshapes.com/groundwork
UNION_PUBLIC_URL=https://manifold.tailoredshapes.com/union
CITYHALL_PUBLIC_URL=https://manifold.tailoredshapes.com/cityhall
YARD_PUBLIC_URL=https://manifold.tailoredshapes.com/yard
LOBBY_PUBLIC_URL=https://manifold.tailoredshapes.com/lobby
MANIFOLD_PUBLIC_URL=https://manifold.tailoredshapes.com
```

> Set **every** peer URL each app needs — `cityhall` needs `LOBBY_PUBLIC_URL`
> and `manifold-lobby` needs all of groundwork/union/cityhall/yard, or their
> cross-app links fall back to the `*.tildarc.com` source defaults (this was a
> real bug on the k8s deploy).

`manifold-lobby` also needs the automation identity it uses to poll/derive:
`MANIFOLD_USER_ID=lobby-system`, `MANIFOLD_USER_GROUPS=automation:lobby-derive`.

## Seeding the demo (run once, after the stack is up)

`load_fixture.py` writes, so it must authenticate as an **admin**. It's
idempotent (skips records that already exist by name), so re-running is safe.

```sh
MANIFOLD_USER_ID=seed@tailoredshapes.com MANIFOLD_USER_GROUPS=admin \
python3 data/load_fixture.py \
  --base-url-groundwork https://manifold.tailoredshapes.com/groundwork \
  --base-url-union      https://manifold.tailoredshapes.com/union \
  --base-url-cityhall   https://manifold.tailoredshapes.com/cityhall \
  --base-url-yard       https://manifold.tailoredshapes.com/yard
```

Two ways to give it admin write access while the public edge is viewer-only:
- run it from **inside** the network against the app services directly
  (`http://groundwork:3000` …) — the apps trust the `X-Manifold-User-*` headers
  the script now sends; **or**
- point a temporary admin-stamping edge at the stack for the seed run, then
  switch the public viewer edge in.

`manifold-lobby` is not seeded directly — it polls the other apps and derives
its advisories/programs once data is present.

## AWS gotcha — registry pull trust

If the AWS hosts pull from `registry.tildarc.com`, the registry's token endpoint
redirects to `git.tildarc.com/jwt/auth`, served by a Caddy **internal** cert. The
node's container runtime must trust the Caddy root CA or pulls fail with
`x509: certificate signed by unknown authority ("Caddy Local Authority")`. See
`conduit/k8s/README.md` for the fix. (Alternatively, mirror the images into ECR
and pull from there — cleaner for an AWS-native deploy.)

## Still open (manifold side)

- **Landing page** at `/` — currently the edge redirects the bare origin to
  `/groundwork/`. A proper `manifold-landing` (logo + config-driven app tiles,
  de-tildarc'd) would be the real front door. Tracked separately.
