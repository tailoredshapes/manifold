# Manifold auth: edge-header + Casbin

> Captured 2026-05-12. Aligns with canonical meshql implementations (Java in `meshql/auth/casbin/`, TypeScript in `meshobj/core/casbin_auth/`).

## Goal

Authentication and role-based authorization for the four Manifold services and the outer landing app, deployable per-customer (first customer: Azure / Entra), runnable locally with no real IdP.

## Architecture

```
┌──────────┐    OIDC/SAML/...     ┌────────┐   trusted headers   ┌─────────────┐
│  IdP     │ ───────────────────► │ Caddy  │ ──────────────────► │ groundwork  │
│ (Entra / │                      │ gateway│  X-Whatever-Id      │ union       │
│  ...)    │                      │        │  X-Whatever-Groups  │ cityhall    │
└──────────┘                      └────────┘                     │ yard        │
                                                                 │ outer (web) │
                                                                 └─────────────┘
```

**Authentication: at the edge.** Caddy talks to the customer's IdP. Header names are a Caddy ↔ service contract — arbitrary, customer-deployment-time decision. Services trust the headers because the topology guarantees Caddy is the only ingress.

**Authorization: in-app, via Casbin (per meshql best practices).** Identity from headers flows through meshql's `Auth` trait. The trait composition is the same as Java/TS canonical:

```
CasbinAuth(enforcer, inner)
    .get_auth_token(stash) → inner.get_auth_token(stash) → [user_id]
                          → enforcer.get_roles_for_user(user_id) → [role1, role2, ...]
    .is_authorized(roles, envelope) → any role ∈ envelope.authorized_tokens
                                    (empty authorized_tokens = public)
```

For Manifold, `inner` is a `StashKeyAuth` that reads a configured key from the request `Stash`. The `Stash` is populated by an axum middleware that copies trusted headers in.

## Components

| Where | Component | Purpose |
|---|---|---|
| `meshql-rs/meshql-core` | `StashKeyAuth` (new) | Reads identity from a configured Stash key; returns `[user_id]`. Generic "leaf Auth". |
| `meshql-rs/meshql-casbin` (new crate) | `CasbinAuth<A: Auth>` | Wraps any inner Auth. Exact parallel to Java `CasbinAuth.java` and TS `casbin_auth/src/index.ts`. Full test parity. |
| `manifold/manifold-edge` (new tiny crate) | axum middleware `header_identity` | Reads configured request headers, writes them to request-extension Stash, hands off to the handler. |
| `manifold/{groundwork,union,cityhall,yard}` | wiring | Replace `NoAuth` with `CasbinAuth::new(model, policy, StashKeyAuth("user_id"))`. Ship per-service `model.conf` + `policy.csv`. |
| `manifold/caddy/` (new dir) | Caddyfile templates | `Caddyfile.dev` injects a synthetic identity. `Caddyfile.azure-entra.example` shows Entra → canonical headers mapping. |

## What `meshql-casbin` looks like

Parallel to the Java/TS impls:

```rust
pub struct CasbinAuth<A: Auth> {
    enforcer: tokio::sync::Mutex<Enforcer>,
    inner: A,
}

impl<A: Auth> CasbinAuth<A> {
    pub async fn new(model_path: &str, policy_path: &str, inner: A) -> Result<Self, casbin::Error> {
        let enforcer = Enforcer::new(model_path, policy_path).await?;
        Ok(Self { enforcer: enforcer.into(), inner })
    }
}

impl<A: Auth> Auth for CasbinAuth<A> {
    fn get_auth_token(&self, context: &Stash) -> Vec<String> {
        let user_ids = self.inner.get_auth_token(context);
        if user_ids.is_empty() { return vec![]; }
        // get_roles_for_user is sync; mutex is uncontended in practice
        self.enforcer.blocking_lock().get_roles_for_user(&user_ids[0], None)
    }

    fn is_authorized(&self, credentials: &[String], envelope: &Envelope) -> bool {
        if envelope.authorized_tokens.is_empty() { return true; }
        envelope.authorized_tokens.iter().any(|t| credentials.iter().any(|c| c == t))
    }
}
```

Tests mirror the Java suite: initialize, retrieve roles, authorize on match, deny on no-match, allow on empty `authorized_tokens`.

## Dev mode

`docker-compose.yml` gets a Caddy container that injects:
```
X-Manifold-User-Id: alice@example.dev
X-Manifold-User-Groups: engineering,admin
```
…before forwarding to each Rust service on its current port. Policy file ships with a `g, alice@example.dev, admin` entry so the existing fixture data is reachable.

For tests/integration, axum routes can be hit directly with the headers — no Caddy required.

## Production / Azure

A `Caddyfile.azure-entra.example` maps `X-MS-CLIENT-PRINCIPAL-NAME` (Entra easy-auth's standard header) → `X-Manifold-User-Id`, ditto for groups. Per-customer Caddyfile is the customization point.

## What's not in scope (this round)

- MCP server auth (binaries still hit services unauthenticated; dev only)
- Authorization on writes beyond the existing `authorized_tokens` envelope filter
- Audit logging
- Group hierarchy / inheritance (Casbin supports it; policy stays flat for v1)
- Outer landing app: served by Caddy with no auth required (it's a public landing page)
