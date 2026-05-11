# meshql-mcp — Design

## Context

`groundwork-mcp` (~1086 LOC across `src/bin/groundwork-mcp.rs` and `src/mcp/`) gives an LLM a structured way to interrogate the Groundwork catalogue: catalog list/get/search across the 6 entity types plus three graph-traversal tools (`blast_radius`, `dependencies_of`, `deployment_plan`). It's a successful Phase-5 deliverable, but it only exists for one of Manifold's four apps. The other three (Union, Cityhall, Yard) need MCP surface too.

Inspection shows ~700 of those 1086 lines are generic: the stdio JSON-RPC transport, the tool registry types, an HTTP client that hits `/<entity>/api`, and three catalog tools (list / get / search) that are agnostic over entity names. The 350-ish app-specific lines are the in-memory graph snapshot and the three graph tools that wrap it.

Cloning the generic two-thirds across three new apps would produce ~2100 LOC of near-duplicate boilerplate. Worse, every MCP-protocol revision or catalog-tool tweak would need to be applied four times. The pattern wants to be a library.

`meshql-rs` is the natural home: the franchise is "define entities, get REST + GraphQL + federation for free." Adding `+ MCP` is exactly that shape — auto-generating a tool surface from entity definitions.

## Decisions

Locked during the design exchange:

- **The crate lives in `meshql-rs` from day one** as `meshql-mcp`, alongside `meshql-restlette`, `meshql-graphlette`, etc. Manifold's apps add it as a path dependency the same way they consume `meshql-core` today.
- **Full pass in this initiative**: build the crate, refactor `groundwork-mcp` onto it (the abstraction validator), add `union-mcp` / `cityhall-mcp` / `yard-mcp`. Single coherent commit chain.
- **MCP transport: stdio JSON-RPC** (the same that `groundwork-mcp` already speaks). HTTP transport is a defensible future variant but not in scope here.
- **Reads stay on REST** for now. The frontends' "reads via /graph" CQRS rule is a frontend-design preference; the MCP server is a server-to-server consumer, and the existing REST envelopes (`{id, payload}`) already match what `groundwork-mcp`'s cucumber scenarios assert on. A future refactor can switch the catalog tools to `/graph`-shaped queries (cleaner data, federation benefits) — out of scope here to keep the refactor surface contained.
- **Library + thin bin per app**. The crate is a library; each app keeps a tiny `bin/<app>-mcp.rs` (15–25 LOC) that wires `MeshqlClient` + entity names + custom tools.
- **No upstream PR ceremony**. meshql-rs's CI runs on PRs from outside contributors, but the tailoredshapes TBD rule still applies: the maintainer commits direct to main on this repo too. The CI just runs on push as well.

## Design

### What the crate provides (the generic two-thirds)

```rust
// meshql-mcp/src/lib.rs

pub struct McpServerConfig {
    pub server_name: String,
    pub server_version: String,
    pub client: Arc<MeshqlClient>,
    /// Entity names served by the underlying meshql-rs app. Catalog tools
    /// validate the `entity` argument against this list and expose it as a
    /// JSON-Schema enum in `tools/list`.
    pub entities: Vec<&'static str>,
    /// App-specific tools (graph queries, bylaw walking, run history, etc.).
    /// Appended to the built-in catalog tools.
    pub custom_tools: Vec<Tool>,
}

pub struct MeshqlMcpServer {
    config: McpServerConfig,
    tools: Vec<Tool>,
}

impl MeshqlMcpServer {
    pub fn new(config: McpServerConfig) -> Self;
    /// Speak MCP over stdio (JSON-RPC 2.0, one request per line) until stdin
    /// closes. Logs to stderr.
    pub async fn serve_stdio(&self) -> anyhow::Result<()>;
}

pub struct MeshqlClient { /* base_url + reqwest::Client */ }
impl MeshqlClient {
    pub fn new(base_url: impl Into<String>) -> Self;
    /// Construct from an env var, falling back to a default.
    pub fn from_env(env_var: &str, default_url: &str) -> Self;
    pub fn base_url(&self) -> &str;
    /// GET /<entity>/api — returns the array of envelopes.
    pub async fn list(&self, entity: &str) -> anyhow::Result<Value>;
    /// GET /<entity>/api/<id> — returns the envelope, or None on 404.
    pub async fn get(&self, entity: &str, id: &str) -> anyhow::Result<Option<Value>>;
    /// GET /<path>  (for custom endpoints like /org_node/:id/effective_bylaws).
    pub async fn get_path(&self, path: &str) -> anyhow::Result<Value>;
    /// POST /<path>  (for custom endpoints like /change_request/:id/plan).
    pub async fn post_path(&self, path: &str, body: Value) -> anyhow::Result<Value>;
}

pub type ToolFuture = Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>;
pub type ToolHandler = Arc<dyn Fn(Arc<MeshqlClient>, Value) -> ToolFuture + Send + Sync>;

#[derive(Clone)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: ToolHandler,
}

/// Built-in catalog tools, parameterized over the entity list.
pub mod catalog {
    pub fn tools(entities: &[&'static str]) -> Vec<super::Tool>;  // returns list/get/search
}

/// Wrap a `Value` result so MCP `tools/call` returns the
/// `{ content: [{ type: "text", text }], structuredContent }` shape clients expect.
pub fn wrap_text_result(value: &Value) -> Value;
```

The transport handler (`serve_stdio`) implements:
- `initialize` — server info + capabilities (`tools.listChanged: false`).
- `notifications/initialized` — no-op acknowledgement (no response).
- `tools/list` — registry export.
- `tools/call` — dispatch by name, wrap success via `wrap_text_result`, wrap errors as `{ content, isError: true }`.
- `ping` — `{}`.
- Other methods — JSON-RPC error `-32601 method not found`.
- Parse errors — JSON-RPC error `-32700`.

### What each app contributes (the specific third)

| App | bin (~20 LOC) | Custom tools (file → lines) | Domain logic |
|-----|---------------|----------------------------|--------------|
| **groundwork** | `bin/groundwork-mcp.rs` (≈25 LOC) | `mcp/tools/graph.rs` (≈105) | `mcp/graph.rs` Snapshot (≈550) — kept verbatim from today |
| **union** | `bin/union-mcp.rs` (≈20 LOC) | `mcp/tools.rs` (≈120) — team_capacity, members_of_team, open_work_for_person | none beyond client+payload-shape helpers |
| **cityhall** | `bin/cityhall-mcp.rs` (≈20 LOC) | `mcp/tools.rs` (≈180) — ancestors_of, effective_bylaws_for, compute_plan_for_cr, render_gantt_for_plan | thin wrappers over existing custom endpoints |
| **yard** | `bin/yard-mcp.rs` (≈20 LOC) | `mcp/tools.rs` (≈180) — history_for_env, availability_for_env, estimate_for_cr, recommend_sync | thin wrappers over existing custom endpoints |

The Union app doesn't have custom endpoints beyond REST/graph, so its app-specific tools are pure rollups computed in the MCP layer from `MeshqlClient::list` results (e.g. team_capacity sums `story_points` of in-flight work orders for a team).

### What `groundwork-mcp` loses in the refactor

- `src/mcp/client.rs` (66 LOC) — deleted; replaced by `meshql_mcp::MeshqlClient`.
- `src/mcp/tools/mod.rs` (53 LOC) — deleted; types come from `meshql_mcp::{Tool, ToolHandler, ToolFuture}`.
- `src/mcp/tools/catalog.rs` (139 LOC) — deleted; replaced by `meshql_mcp::catalog::tools(&[...])`.
- Most of `src/bin/groundwork-mcp.rs` (168 → ~25 LOC) — the transport moves into the crate.
- **Stays**: `src/mcp/graph.rs` (550 LOC of Groundwork-specific Snapshot + traversal), `src/mcp/tools/graph.rs` (105 LOC of tool wrappers over that snapshot).
- **Net change**: groundwork-mcp shrinks from 1086 LOC to ~680 LOC. Same behavior; 8 existing cucumber scenarios continue to pass byte-for-byte on the wire.

## Non-goals (explicit)

- **HTTP transport** for MCP. stdio only. JSON-RPC framed by newline. (HTTP/SSE is the other MCP transport per spec; defensibly future work.)
- **Auth pass-through**. The current `groundwork-mcp` runs ambient (any caller can hit any tool). Hooking `meshql_core::Auth` into the MCP request path is its own initiative — and depends on auth landing on the underlying HTTP servers first, which we deferred earlier.
- **REST → /graph for MCP catalog tools**. Documented above. Future refactor; out of scope here.
- **IaC import/export tools** (Phase 5's "promised expansion"). Out of scope; not blocked by this refactor.
- **MCP resource / prompt surfaces**. Tools only. Resources and prompts are MCP-spec capabilities we don't need yet.
- **Cross-repo CI orchestration**. meshql-rs's CI runs on its own pushes; manifold's CI runs on its own pushes. The two repos' work flows in lockstep manually during this initiative (meshql-rs change first, then manifold app commits referencing the new path dep).

## Verification

Per phase:

- **Phase 1 (crate scaffolding)**: `cargo build -p meshql-mcp` in meshql-rs clean; unit tests for the transport (initialize round-trip, tools/list shape, tools/call dispatch, parse-error → -32700, unknown method → -32601, unknown tool → error result with `isError: true`).

- **Phase 2 (groundwork refactor)**: from manifold, `cargo build -p groundwork` clean; `cargo test -p groundwork --test mcp_harness` — the existing 8 scenarios continue to pass. **No wire-format change.**

- **Phase 3-5 (new apps)**: per app, a new cucumber feature `<app>_mcp.feature` with at least:
  1. `tools/list` returns the expected tool names.
  2. One catalog tool (`<app>.list` for a representative entity) returns the expected count.
  3. One custom tool returns a shape that exercises an app-specific endpoint.

  Plus per-app `<app>/tests/mcp_harness.rs` modeled on `groundwork/tests/mcp_harness.rs` — spawns the bin against the running HTTP server via stdio pipes.

- **Final**: `cargo test --workspace` in manifold continues to pass (modulo the existing 8 pre-existing MCP-binary skips in groundwork's regular cucumber — those will keep skipping because the regular test runner doesn't spawn the bin; the dedicated `mcp_harness.rs` per app does).

## Critical files

| Phase | Repo | Files |
|-------|------|-------|
| 1 | meshql-rs | `Cargo.toml` (add `meshql-mcp` to members), `meshql-mcp/Cargo.toml`, `meshql-mcp/src/lib.rs`, `meshql-mcp/src/transport.rs`, `meshql-mcp/src/client.rs`, `meshql-mcp/src/tool.rs`, `meshql-mcp/src/catalog.rs`, `meshql-mcp/tests/transport.rs` |
| 2 | manifold | `groundwork/Cargo.toml`, `groundwork/src/bin/groundwork-mcp.rs`, `groundwork/src/mcp/mod.rs`, deletion of `groundwork/src/mcp/client.rs` + `groundwork/src/mcp/tools/mod.rs` + `groundwork/src/mcp/tools/catalog.rs`, keep `graph.rs` + `tools/graph.rs` |
| 3 | manifold | `union/Cargo.toml`, `union/src/bin/union-mcp.rs`, `union/src/mcp/mod.rs`, `union/src/mcp/tools.rs`, `union/tests/features/union_mcp.feature`, `union/tests/mcp_harness.rs` |
| 4 | manifold | same shape for cityhall |
| 5 | manifold | same shape for yard |

## Open questions resolved during implementation

These don't block the spec but get decided in the per-phase plans:

1. **What's the right name for the catalog-tool list?** `catalog.list` (the current groundwork convention) or `<entity>.list`? Sticking with `catalog.*` keeps groundwork's existing cucumber scenarios working byte-for-byte; new apps inherit the same name.
2. **Tool-name namespacing across apps.** All four apps will speak `catalog.list` — there's no conflict because each app exposes its own MCP server, not a shared one. A future "Manifold-wide MCP gateway" would need namespacing, but that's separate.
3. **Crate version 0.1.0.** Pin in `meshql-mcp/Cargo.toml`; matches sibling crates.
