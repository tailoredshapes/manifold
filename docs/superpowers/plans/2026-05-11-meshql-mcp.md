# meshql-mcp — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the generic MCP-server primitives from `groundwork-mcp` into a new `meshql-mcp` crate in the meshql-rs workspace; refactor `groundwork-mcp` onto it (validating the abstraction); then build `union-mcp`, `cityhall-mcp`, `yard-mcp` using the same configurable pattern. Each app's MCP server becomes ~20 lines of bin + ~150 LOC of app-specific custom tools.

**Architecture:** Per spec at `docs/superpowers/specs/2026-05-11-meshql-mcp-design.md`. `meshql-mcp` provides `MeshqlMcpServer` (stdio JSON-RPC transport), `MeshqlClient` (REST list/get + custom GET/POST paths), `Tool` types, and generic `catalog::tools(entities)` (list / get / search). Apps add `bin/<app>-mcp.rs` (thin wrapper), an `mcp/` module with custom tools, an `mcp_harness.rs` integration test, and a `<app>_mcp.feature` cucumber file.

**Tech Stack:** Rust 2021. tokio (async + stdio). reqwest with rustls-tls. serde_json. anyhow. async-trait. (All already in the meshql-rs workspace deps; no new dependencies.)

**Workflow:** Trunk-Based Development across both `/tank/repos/tailoredshapes/meshql-rs` (Phase 1) and `/tank/repos/tailoredshapes/manifold` (Phases 2–5). Direct commits to `main` on each repo, push after each task. No PRs.

**Cross-repo ordering:** meshql-rs Phase 1 commits land first. Then manifold's path-dep references work. Phases 2–5 commit to manifold.

---

## Phase 1 — Build `meshql-mcp` crate (Tasks 1–5)

Repo: `/tank/repos/tailoredshapes/meshql-rs`. New workspace member.

### Task 1: Crate scaffolding

**Files:**
- Modify: `Cargo.toml` (workspace `members` add `"meshql-mcp"`)
- Create: `meshql-mcp/Cargo.toml`
- Create: `meshql-mcp/src/lib.rs` (initial: `pub mod transport; pub mod client; pub mod tool; pub mod catalog;` re-exports)

- [ ] **Step 1: Add member to workspace**

```bash
# verify current shape
grep -n 'meshql-restlette\|members' /tank/repos/tailoredshapes/meshql-rs/Cargo.toml | head
```

Then add `"meshql-mcp",` next to `"meshql-restlette",` in the members list.

- [ ] **Step 2: Create the crate's Cargo.toml**

Pattern matches `meshql-restlette/Cargo.toml`:

```toml
[package]
name = "meshql-mcp"
version = "0.1.0"
edition = "2021"
description = "meshql-mcp — Model Context Protocol server for meshql-rs deployments"
license = "MIT"
repository = "https://github.com/tailoredshapes/meshql-rs"

[dependencies]
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
async-trait = { workspace = true }
```

(Verify `anyhow` / `async-trait` are in workspace deps; if not, declare directly.)

- [ ] **Step 3: Skeletons for the four modules**

`meshql-mcp/src/lib.rs`:

```rust
//! Model Context Protocol server for meshql-rs deployments.
pub mod client;
pub mod tool;
pub mod transport;
pub mod catalog;

pub use client::MeshqlClient;
pub use tool::{Tool, ToolHandler, ToolFuture, wrap_text_result};
pub use transport::{MeshqlMcpServer, McpServerConfig};
```

Each submodule starts as an empty file with module docs.

- [ ] **Step 4: Build verifies the empty crate**

```bash
cd /tank/repos/tailoredshapes/meshql-rs && cargo build -p meshql-mcp
```

Expected: clean build with no symbols defined.

- [ ] **Step 5: Commit**

```
feat(meshql-mcp): scaffold new MCP-server crate

New workspace member providing the generic primitives behind
meshql-rs-flavoured Model Context Protocol servers. Empty modules;
implementations follow.
```

Push.

### Task 2: `MeshqlClient` (REST HTTP client)

**Files:** `meshql-mcp/src/client.rs`

Port from `groundwork/src/mcp/client.rs` and extend:

```rust
use anyhow::Context;
use serde_json::Value;

pub struct MeshqlClient {
    base_url: String,
    http: reqwest::Client,
}

impl MeshqlClient {
    pub fn new(base_url: impl Into<String>) -> Self;
    pub fn from_env(env_var: &str, default_url: &str) -> Self;
    pub fn base_url(&self) -> &str;
    pub async fn list(&self, entity: &str) -> anyhow::Result<Value>;
    pub async fn get(&self, entity: &str, id: &str) -> anyhow::Result<Option<Value>>;
    pub async fn get_path(&self, path: &str) -> anyhow::Result<Value>;
    pub async fn post_path(&self, path: &str, body: Value) -> anyhow::Result<Value>;
}
```

- [ ] **Step 1: Port `list`, `get` byte-for-byte from groundwork-mcp's client**
- [ ] **Step 2: Add `get_path` and `post_path` for custom endpoints**
- [ ] **Step 3: Add `from_env(env_var, default_url)` — generalized over the env var name (the existing groundwork version hardcodes `GROUNDWORK_URL`)**
- [ ] **Step 4: Unit tests for URL construction**:

```rust
#[test]
fn list_url_format() {
    let c = MeshqlClient::new("http://localhost:3000");
    // Confirm we generate /<entity>/api and /<entity>/api/<id>
    // (use a public helper or assert internal behavior indirectly via the test scaffolding)
}
```

(URL building is internal; if it's hard to test without an HTTP server stub, skip the unit and rely on integration coverage via `groundwork`'s existing tests. Pragmatic call.)

- [ ] **Step 5: `cargo build -p meshql-mcp` clean. Commit.**

```
feat(meshql-mcp): MeshqlClient — REST list/get plus arbitrary GET/POST paths

list(entity) and get(entity, id) match the shape every meshql-rs
deployment exposes. get_path / post_path support custom endpoints
(e.g. /change_request/:id/plan) that app-specific tools wrap.
```

### Task 3: `Tool` types + `wrap_text_result`

**Files:** `meshql-mcp/src/tool.rs`

Port from `groundwork/src/mcp/tools/mod.rs`:

```rust
use crate::client::MeshqlClient;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type ToolFuture = Pin<Box<dyn Future<Output = anyhow::Result<Value>> + Send>>;
pub type ToolHandler = Arc<dyn Fn(Arc<MeshqlClient>, Value) -> ToolFuture + Send + Sync>;

#[derive(Clone)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: ToolHandler,
}

pub fn wrap_text_result(value: &Value) -> Value { /* exactly as in groundwork today */ }
```

- [ ] Port + build clean + commit `feat(meshql-mcp): Tool types and wrap_text_result helper`.

### Task 4: Generic `catalog::tools(entities)`

**Files:** `meshql-mcp/src/catalog.rs`

Port from `groundwork/src/mcp/tools/catalog.rs`, parameterised:

```rust
use crate::{MeshqlClient, Tool, ToolFuture};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn tools(entities: &[&'static str]) -> Vec<Tool> {
    // returns catalog.list, catalog.get, catalog.search wired to the entity list.
    // Each handler closes over `entities.to_vec()` so the validation list is
    // available at call time without re-passing it.
}
```

- [ ] Port + build clean + cucumber-equivalent unit test:

```rust
#[test]
fn catalog_tools_returns_list_get_search() {
    let ts = tools(&["deployable", "service"]);
    assert_eq!(ts.len(), 3);
    let names: Vec<_> = ts.iter().map(|t| t.name).collect();
    assert_eq!(names, vec!["catalog.list", "catalog.get", "catalog.search"]);
    // input_schema's `entity` enum should match the entities passed in
    let list_schema = &ts[0].input_schema;
    let enum_vals = list_schema.pointer("/properties/entity/enum").unwrap();
    assert_eq!(enum_vals, &json!(["deployable", "service"]));
}
```

Commit: `feat(meshql-mcp): catalog::tools generic over entity list`.

### Task 5: `MeshqlMcpServer` (stdio JSON-RPC transport)

**Files:** `meshql-mcp/src/transport.rs`

Port from `groundwork/src/bin/groundwork-mcp.rs`. The bin file's `main` becomes a struct method:

```rust
use crate::{MeshqlClient, Tool, wrap_text_result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct McpServerConfig {
    pub server_name: String,
    pub server_version: String,
    pub client: Arc<MeshqlClient>,
    pub entities: Vec<&'static str>,
    pub custom_tools: Vec<Tool>,
}

pub struct MeshqlMcpServer {
    config: McpServerConfig,
    tools: Vec<Tool>,
}

impl MeshqlMcpServer {
    pub fn new(config: McpServerConfig) -> Self {
        let mut tools = crate::catalog::tools(&config.entities);
        tools.extend(config.custom_tools.iter().cloned());
        Self { config, tools }
    }
    pub async fn serve_stdio(&self) -> anyhow::Result<()> { /* ports main() loop */ }
}
```

Constants like `PROTOCOL_VERSION = "2025-06-18"` move into transport.rs.

- [ ] **Step 1: Port the request loop, response framing, and the four methods (`initialize`, `ping`, `tools/list`, `tools/call`)**

- [ ] **Step 2: Unit tests for the request handler — invoke `serve_stdio`-equivalent handler functions directly without spawning the bin:**

```rust
#[tokio::test]
async fn initialize_returns_capabilities() { ... }
#[tokio::test]
async fn tools_list_returns_configured_tools() { ... }
#[tokio::test]
async fn tools_call_unknown_tool_returns_isError() { ... }
#[tokio::test]
async fn parse_error_returns_neg_32700() { ... }
#[tokio::test]
async fn unknown_method_returns_neg_32601() { ... }
```

To test without piping stdio, expose a `pub(crate) async fn handle_request(&self, req: Value) -> Option<Value>` that returns the response (or None for notifications); `serve_stdio` is then a thin loop around it.

- [ ] **Step 3: cargo test -p meshql-mcp clean. Commit: `feat(meshql-mcp): stdio JSON-RPC transport (MeshqlMcpServer)`.**

---

## Phase 2 — Refactor `groundwork-mcp` (Tasks 6–9)

Repo: `/tank/repos/tailoredshapes/manifold`. Verifies the abstraction.

### Task 6: Add the path dependency

**Files:** `groundwork/Cargo.toml`

Add: `meshql-mcp = { path = "../../meshql-rs/meshql-mcp" }`.

(Same shape as the existing `meshql-core = { path = "../../meshql-rs/meshql-core" }` line.)

- [ ] Build verifies the dep resolves: `cargo build -p groundwork`.
- [ ] Commit: `chore(groundwork): depend on meshql-mcp`.

### Task 7: Rewrite `bin/groundwork-mcp.rs`

**Files:** `groundwork/src/bin/groundwork-mcp.rs`

From ~168 LOC to ~25:

```rust
use groundwork::mcp::custom_tools;
use meshql_mcp::{MeshqlClient, MeshqlMcpServer, McpServerConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env("GROUNDWORK_URL", "http://localhost:3000"));
    let custom_tools = custom_tools(client.clone());  // returns the 3 graph tools
    let config = McpServerConfig {
        server_name: "groundwork-mcp".into(),
        server_version: env!("CARGO_PKG_VERSION").into(),
        client,
        entities: vec!["deployable", "service", "exposes", "dependency", "contract", "sla"],
        custom_tools,
    };
    eprintln!("groundwork-mcp v{} → {} ({} tools)",
        env!("CARGO_PKG_VERSION"),
        std::env::var("GROUNDWORK_URL").unwrap_or_else(|_| "http://localhost:3000".into()),
        config.custom_tools.len() + 3);  // 3 catalog tools added by the server
    MeshqlMcpServer::new(config).serve_stdio().await
}
```

- [ ] Commit: `refactor(groundwork): MCP server uses meshql-mcp transport`.

### Task 8: Delete the now-duplicated modules

**Files:**
- Delete: `groundwork/src/mcp/client.rs`
- Delete: `groundwork/src/mcp/tools/catalog.rs`
- Modify: `groundwork/src/mcp/tools/mod.rs` — drop the type aliases (re-export from meshql-mcp instead) and drop `catalog::tools` from `all_tools`. Keep `graph::tools()` registration.
- Modify: `groundwork/src/mcp/mod.rs` — remove `client` and `tools` aliasing.
- Modify: `groundwork/src/mcp/graph.rs` — replace `use crate::mcp::client::GroundworkClient;` with `use meshql_mcp::MeshqlClient as GroundworkClient;` (or type-alias at the top).
- Modify: `groundwork/src/mcp/tools/graph.rs` — same import swap.

Rename `groundwork::mcp::tools::all_tools` → `groundwork::mcp::custom_tools(client)` — returns only the 3 graph tools (the catalog tools come from `meshql_mcp::catalog::tools`, which `MeshqlMcpServer::new` adds automatically).

- [ ] **Step 1: Delete + modify**
- [ ] **Step 2: `cargo build -p groundwork` clean**
- [ ] **Step 3: Run the existing harness:**

```bash
cd /tank/repos/tailoredshapes/manifold && cargo test -p groundwork --test mcp_harness
```

Expected: all 8 scenarios pass byte-for-byte.

- [ ] **Step 4: Commit:**

```
refactor(groundwork): drop duplicated MCP scaffolding now that meshql-mcp owns it
```

### Task 9: Verify `cargo test --workspace` is green in manifold

- [ ] Run + confirm no regressions.
- [ ] No commit unless something needs fixing.

---

## Phase 3 — `union-mcp` (Tasks 10–12)

Repo: manifold. New binary + custom tools.

### Task 10: Cargo wiring + bin scaffold

**Files:**
- Modify: `union/Cargo.toml` — add `meshql-mcp` path-dep; add `[[bin]] name = "union-mcp"` entry.
- Create: `union/src/bin/union-mcp.rs` (~20 LOC, exact shape of groundwork's post-refactor bin).
- Create: `union/src/mcp/mod.rs` — `pub mod tools; pub use tools::custom_tools;`
- Create: `union/src/mcp/tools.rs` — initially returning an empty `Vec<Tool>` so the bin builds.

Entities: `["person", "team", "team_member", "work_order"]`.

- [ ] Build verifies bin compiles + runs (stdin EOF → exits clean).
- [ ] Commit: `feat(union): scaffold union-mcp bin (catalog tools only, no custom yet)`.

### Task 11: Custom tools

**Files:** `union/src/mcp/tools.rs`

Implement at least:

- `team.capacity(team_id)` — sums `story_points` over in-flight (`!= "done"`) WorkOrders for the team. Returns `{ team_id, team_name, points_in_flight, member_count }`.
- `team.members(team_id)` — returns the Person records associated via TeamMember rows.
- `person.assignments(person_id)` — returns open work orders across all teams this person belongs to.

Each is ~30–40 LOC: the handler closure calls `client.list(...)` for the relevant entities, computes the rollup, and returns a `Value`.

- [ ] Implement
- [ ] Commit: `feat(union): MCP custom tools — team capacity, members, person assignments`.

### Task 12: Cucumber feature + harness

**Files:**
- Create: `union/tests/features/union_mcp.feature` — at least 3 scenarios (tools/list count + names, one catalog tool, one custom tool).
- Create: `union/tests/mcp_harness.rs` — model on `groundwork/tests/mcp_harness.rs`; spawn the bin against the running HTTP server via stdio.

- [ ] **Step 1: Read groundwork's harness to understand the bin-spawning pattern.**

```bash
head -80 /tank/repos/tailoredshapes/manifold/groundwork/tests/mcp_harness.rs
```

- [ ] **Step 2: Port the harness with union-specific paths.**
- [ ] **Step 3: Write the 3 feature scenarios.**
- [ ] **Step 4: `cargo test -p union --test mcp_harness` → passes (or shows skips if it can't find the bin in the test runner, which matches groundwork's behavior).**
- [ ] **Step 5: Commit: `test(union): cucumber harness for union-mcp`.**

---

## Phase 4 — `cityhall-mcp` (Tasks 13–15)

Same shape as Phase 3, for Cityhall. Entities: `["org_node", "bylaw", "change_request", "deployment_plan", "gantt_output"]`.

### Task 13: Cargo wiring + bin scaffold

Commit: `feat(cityhall): scaffold cityhall-mcp bin`.

### Task 14: Custom tools

Implement:

- `org.ancestors(org_node_id)` — wraps `GET /org_node/:id/ancestors`. Returns the chain enterprise → ... → leaf.
- `org.effective_bylaws(org_node_id)` — wraps `GET /org_node/:id/effective_bylaws`. Returns the cascade.
- `change_request.plan(change_request_id, tier?)` — wraps `POST /change_request/:id/plan` with optional tier. Returns the computed plan envelope.
- `deployment_plan.gantt(deployment_plan_id)` — wraps `POST /deployment_plan/:id/gantt`. Returns the Mermaid string envelope.

All four wrap existing endpoints; the tool body is `client.post_path` or `client.get_path`.

Commit: `feat(cityhall): MCP custom tools — ancestors, effective_bylaws, plan, gantt`.

### Task 15: Cucumber feature + harness

Same shape; 3+ scenarios. Commit: `test(cityhall): cucumber harness for cityhall-mcp`.

---

## Phase 5 — `yard-mcp` (Tasks 16–18)

Same shape. Entities: `["test_environment", "test_infrastructure", "mock_source", "data_source", "data_sync", "test_run", "test_suite"]`.

### Task 16: Cargo wiring + bin scaffold

Commit: `feat(yard): scaffold yard-mcp bin`.

### Task 17: Custom tools

- `environment.history(test_environment_id)` — wraps `GET /test_environment/:id/history`.
- `environment.availability(test_environment_id)` — wraps `GET /test_environment/:id/availability`.
- `change_request.estimate(change_request_id, tier?)` — wraps `POST /change_request/:id/estimate`.
- `data_sync.recommend(edge)` — wraps `POST /data_sync/recommend`.

Commit: `feat(yard): MCP custom tools — env history/availability, CR estimate, sync recommend`.

### Task 18: Cucumber feature + harness

Commit: `test(yard): cucumber harness for yard-mcp`.

---

## Final cross-cutting verification (Task 19)

- [ ] **`cargo build --workspace` clean in both repos.**
- [ ] **`cargo test --workspace` in manifold:** 131+ scenarios continue to pass; per-app `mcp_harness.rs` shows the new scenarios when run against the local HTTP servers (`docker compose up -d` first, then `cargo test -p <app> --test mcp_harness`).
- [ ] **Manual smoke: each MCP bin accepts the canonical `initialize` → `tools/list` → `tools/call` triple via stdin.**

Example:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | ./target/debug/cityhall-mcp
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | ./target/debug/cityhall-mcp
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"catalog.list","arguments":{"entity":"bylaw"}}}' | GROUNDWORK_URL=http://localhost:3052 ./target/debug/cityhall-mcp
```

(With each app's HTTP server running on its known port.)

No commit for this task — verification only.

---

## Out of scope (explicit)

- **HTTP/SSE MCP transport.** stdio only.
- **Auth pass-through.** MCP requests don't carry auth headers through to the underlying HTTP server yet. Hooking `meshql_core::Auth` is its own initiative.
- **REST → /graph for MCP catalog tools.** Documented in the spec as a defensible future refactor.
- **IaC import/export tools** (Phase 5's mooted expansion).
- **MCP `resources` / `prompts` capabilities.** Tools only.
- **Auto-derivation of entity lists from `<app>/config/json/*.schema.json`.** A nice add — eliminates the hardcoded list per app — but pure polish; defer.
- **Cross-app MCP gateway.** A single MCP entry point that fans out to all four apps' tools is a future possibility but the per-app namespacing isn't there yet.
