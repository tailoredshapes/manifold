# meshql-mcp — Capabilities + Auto-Derivation Design

> **Supersedes** `2026-05-11-meshql-mcp-design.md` mid-rollout. The original
> design factored `groundwork-mcp` into a shared crate with `EntityConfig` + a
> generic `catalog.list`/`get`/`search` triad over REST. Phase 1 (the crate)
> landed. Phases 2–5 rolled out across the four apps with REST-based catalog
> reads. A subsequent review surfaced two problems:
>
> 1. **REST violates the project's CQRS rule.** Reads must come from `/graph`
>    (per `feedback_use_the_graph.md`) — the federation benefits are the whole
>    point. One-layer-deep REST envelopes force the LLM to chase UUIDs across
>    calls.
> 2. **Generic `catalog.list { entity }` is weak MCP.** LLMs pick tools by
>    reading their descriptions in `tools/list`. A parameterized meta-tool
>    that takes "which entity" gives the LLM nothing to choose between; named,
>    well-documented capabilities (`list_services_in_catalog`, `get_service_by_id`,
>    `services_consumed_by_deployable`) give it real selection signal.
>
> R1 (in-flight) introduces `MeshqlClient::gql()` and pivots catalog reads to
> GraphQL. That `gql()` primitive survives this redesign. Everything else
> above `gql()` — `EntityConfig`, the catalog::tools generator — is replaced.

## Context

`meshql-mcp` is a shared crate in the `meshql-rs` workspace. It provides the
stdio JSON-RPC transport, an HTTP client for meshql-rs deployments, and a
configurable tool registry. Four manifold apps (groundwork, union, cityhall,
yard) each ship their own `<app>-mcp` binary that wraps the crate with
app-specific config.

This design replaces the current `Vec<EntityConfig>` + 3 generic catalog tools
with a unified `Vec<Capability>` model where each tool is a named, described,
schema-typed operation. A schema-driven helper auto-generates baseline
capabilities from each app's GraphQL schema files so per-app configuration
stays terse.

## Decisions

- **Capability replaces EntityConfig.** Every tool exposed via `tools/list` is
  a `Capability` instance with name, description, input schema, and a handler
  spec. The current generic `catalog.list/get/search` tools become specifically-
  named per-entity capabilities like `list_deployables`, `get_deployable_by_id`,
  `find_deployables_by_name`.
- **Auto-derivation from GraphQL schemas.** meshql-mcp parses each app's
  schema strings (already `include_str!`'d in the bin) and generates baseline
  capabilities for every `Query` operation discovered (`getAll`, `getById`,
  `getByName`, `getByXId`, …). Names follow conventions; descriptions are
  bland-but-functional defaults that apps override selectively.
- **Builder pattern for assembly.** `CapabilitiesBuilder::auto_from_schemas`
  + `.describe(name, text)` overrides + `.add(custom_capability)` keeps the
  per-app bin declarative.
- **Reads exclusively via `/graph`.** No more `/<entity>/api` reads from
  `catalog.*` tools. Custom REST handlers stay for computed/aggregated GET
  endpoints (`/org_node/:id/effective_bylaws`, `/test_environment/:id/history`)
  and for POSTs (writes). The `MeshqlClient::list`/`get` methods that hit
  REST endpoints are kept for those custom uses; new code defaults to `gql()`.
- **Description style: 1–3 sentences each.** Matches the existing groundwork
  tools (`graph.blast_radius`, etc.). One sentence on what; one on when to
  use it; optional third on caveats. Reads well in `tools/list`; doesn't
  bloat context budget when an app has 20+ capabilities.
- **No new dependencies.** The schema parser is a small regex/state-machine
  walker over the meshql GraphQL subset (~80 LOC). Not worth pulling in
  `async-graphql-parser`.

## Design

### `Capability`

```rust
#[derive(Clone)]
pub struct Capability {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub handler: CapabilityHandler,
}

#[derive(Clone)]
pub enum CapabilityHandler {
    /// Most common — POST a templated GraphQL query.
    GraphQuery {
        path: String,                     // "/deployable/graph"
        /// Query with `{arg}` placeholders that get substituted from the
        /// tool's input JSON before posting. Substitution is single-pass
        /// and escapes string values to prevent injection.
        query_template: String,
    },
    /// GET a templated REST path — for computed endpoints not yet in graph.
    RestGet { path_template: String },    // "/org_node/{id}/effective_bylaws"
    /// POST a templated REST path — for writes/commands.
    RestPost {
        path_template: String,
        /// Optional body; placeholders resolved from input JSON.
        body_template: Option<Value>,
    },
    /// Escape hatch — domain logic that doesn't fit a template
    /// (groundwork's snapshot-based blast_radius/dependencies_of/deployment_plan).
    Custom(ToolHandler),
}

impl Capability {
    /// Override the description on a capability — used by the builder's
    /// `.describe()` method to swap auto-generated defaults.
    pub fn with_description(self, description: &'static str) -> Self;
}
```

### Schema parser

meshql's GraphQL subset is narrow enough to parse with a small hand-rolled
walker. The parser extracts:

- The **principal type** declared first in each schema file (the entity:
  `type Deployable { ... }`).
- The fields on that type, including any embedded foreign-type fields (e.g.
  `team: Team` — the federation projection).
- The **Query** type's operations (`getById`, `getAll`, `getByName`, `getByXId`),
  their argument names, and their return types.

```rust
/// Lifted from a schema string.
pub struct ParsedSchema {
    pub entity_name: String,           // "Deployable"
    pub entity_fields: Vec<String>,    // ["id", "name", ..., "team { id name kind description }"]
    pub query_ops: Vec<QueryOp>,
}

pub struct QueryOp {
    pub name: String,                  // "getById"
    pub args: Vec<(String, String)>,   // [("id", "ID"), ("at", "Float")]
    pub returns_list: bool,            // true for `[Deployable]`, false for `Deployable`
}

pub fn parse_meshql_schema(text: &str) -> anyhow::Result<ParsedSchema>;
```

Handles only the meshql subset. Bails on unsupported constructs with an
actionable error so a schema author sees what's wrong.

### Capability builder

```rust
pub struct CapabilitiesBuilder {
    capabilities: Vec<Capability>,
}

impl CapabilitiesBuilder {
    pub fn new() -> Self;

    /// Auto-generate baseline capabilities from a slice of schemas. For each
    /// schema, every Query operation becomes one capability following the
    /// naming convention table below. Each gets a default description that
    /// names the entity and the operation; apps override selectively via
    /// `.describe(name, text)`.
    pub fn auto_from_schemas(
        self,
        schemas: &[(&'static str, &'static str, &'static str)],
        // (entity_singular, graph_path, schema_text)
    ) -> Self;

    /// Override a capability's description by name. No-op if the name doesn't
    /// match (logs to stderr so typos surface).
    pub fn describe(self, capability_name: &'static str, description: &'static str) -> Self;

    /// Append a custom capability (graph traversal, computed-REST wrapper,
    /// anything not covered by the schema-driven defaults).
    pub fn add(self, capability: Capability) -> Self;

    /// Finalize.
    pub fn build(self) -> Vec<Capability>;
}
```

### Naming convention for auto-generated capabilities

Driven by the operation's name + return shape:

| Operation        | Returns      | Generated capability        | Default description sketch |
|------------------|--------------|-----------------------------|----------------------------|
| `getAll`         | `[Entity]`   | `list_<entities>`           | "List every <entity> in the catalogue." |
| `getById(id)`    | `Entity`     | `get_<entity>_by_id`        | "Fetch one <entity> by its UUID. Returns null if not found." |
| `getByName(name)`| `[Entity]`   | `find_<entities>_by_name`   | "Find <entities> whose name matches the given query." |
| `getByXId(x_id)` | `[Entity]`   | `<entities>_for_<x>`        | "List <entities> related to the given <x>." |
| (anything else)  | —            | skipped (custom only)       | — |

Pluralization is naive (`<entity>s`). Apps override via `.describe()` if the
default reads wrong (`person → persons` → maybe override to "people").

Argument names from the operation become input-schema property names. The
default field selection for the returned shape is "every field on the entity
type, including federated projections" — what an LLM is most likely to want.
A future refinement could let apps trim the auto-default field set per
capability via `.with_fields(name, "id name")`; not in v1.

### `McpServerConfig` simplification

```rust
pub struct McpServerConfig {
    pub server_name: String,
    pub server_version: String,
    pub client: Arc<MeshqlClient>,
    pub capabilities: Vec<Capability>,    // single unified list
}
```

`MeshqlMcpServer::new(config)` registers all capabilities; `serve_stdio()`
dispatches `tools/call` by name.

### Example: Groundwork's `groundwork-mcp.rs` after the redesign

```rust
use groundwork::mcp::custom_capabilities;
use meshql_mcp::{CapabilitiesBuilder, MeshqlClient, MeshqlMcpServer, McpServerConfig};
use std::sync::Arc;

const DEPLOYABLE_GRAPHQL: &str = include_str!("../../config/graph/deployable.graphql");
const SERVICE_GRAPHQL:    &str = include_str!("../../config/graph/service.graphql");
const EXPOSES_GRAPHQL:    &str = include_str!("../../config/graph/exposes.graphql");
const DEPENDENCY_GRAPHQL: &str = include_str!("../../config/graph/dependency.graphql");
const CONTRACT_GRAPHQL:   &str = include_str!("../../config/graph/contract.graphql");
const SLA_GRAPHQL:        &str = include_str!("../../config/graph/sla.graphql");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Arc::new(MeshqlClient::from_env("GROUNDWORK_URL", "http://localhost:3000"));

    let capabilities = CapabilitiesBuilder::new()
        .auto_from_schemas(&[
            ("deployable", "/deployable/graph", DEPLOYABLE_GRAPHQL),
            ("service",    "/service/graph",    SERVICE_GRAPHQL),
            ("exposes",    "/exposes/graph",    EXPOSES_GRAPHQL),
            ("dependency", "/dependency/graph", DEPENDENCY_GRAPHQL),
            ("contract",   "/contract/graph",   CONTRACT_GRAPHQL),
            ("sla",        "/sla/graph",        SLA_GRAPHQL),
        ])
        // Override a few descriptions that earn richer wording:
        .describe("list_deployables",
                  "List every deployable in the Groundwork catalogue, including federated team \
                   metadata and current deployment_status. Use this for inventory; for one record \
                   by id use get_deployable_by_id.")
        .describe("get_service_by_id",
                  "Fetch one service by UUID. Use blast_radius_for_service if you want to know \
                   which deployables depend on it.")
        // Custom capabilities — Groundwork's three graph-traversal tools:
        .add(custom_capabilities::blast_radius(client.clone()))
        .add(custom_capabilities::dependencies_of(client.clone()))
        .add(custom_capabilities::deployment_plan(client.clone()))
        .build();

    eprintln!("groundwork-mcp v{} → {} ({} capabilities)",
        env!("CARGO_PKG_VERSION"), client.base_url(), capabilities.len());

    MeshqlMcpServer::new(McpServerConfig {
        server_name: "groundwork-mcp".into(),
        server_version: env!("CARGO_PKG_VERSION").into(),
        client,
        capabilities,
    }).serve_stdio().await
}
```

The bin is purely declarative. Adding a new entity is one row in the
`auto_from_schemas` call.

### What survives from R1

- `MeshqlClient::gql(path, query)` — used by the `GraphQuery` handler dispatch.
- `MeshqlClient::list`/`get`/`get_path`/`post_path` — used by computed-REST
  handlers (history, availability, plan, gantt) and groundwork's snapshot
  builder. Still part of the public API.
- The stdio JSON-RPC transport in `transport.rs`.
- The `Tool` / `ToolHandler` / `ToolFuture` types — `Capability` builds on
  these; `Tool` stays as the lower-level primitive for raw access.

### What R1 introduced that we remove in C2

- `EntityConfig` struct — replaced by `Capability` + auto-derivation.
- `catalog::tools(entities)` — replaced by the builder.
- The three generic catalog tool names (`catalog.list`, `catalog.get`,
  `catalog.search`) disappear from `tools/list`; each is now multiple
  specifically-named capabilities (one per entity, one per operation).

## Non-goals (explicit)

- **Auto-derivation of `input_schema` argument descriptions** beyond bland
  defaults. Apps can override via the builder if needed.
- **GraphQL `variables` for parameterized queries.** v1 uses string
  substitution with proper escaping. Migrating to variables is a clean
  follow-up but not blocking.
- **Per-capability field-selection trimming.** Auto-derived capabilities use
  the full entity field set including federated projections. Apps can replace
  the whole capability via `.replace(name, custom_capability)` if they need a
  trimmed shape; we don't add a `with_fields` shortcut in v1.
- **MCP `resources` / `prompts` capabilities.** Tools only.
- **Authentication pass-through.** Same as the prior design — separate
  initiative, depends on `meshql_core::Auth` landing in the HTTP servers
  first.
- **HTTP/SSE MCP transport.** stdio only.

## Verification

Per phase:

- **C2 (crate redesign)**: unit tests for the schema parser (cover all four
  query-operation kinds plus a malformed-schema error case), unit tests for
  `auto_from_schemas` (assert names/descriptions for each operation), unit
  tests for `CapabilitiesBuilder::describe` (no-op on typo + warning to
  stderr). `cargo test -p meshql-mcp` green. Hook passes on push (no
  `--no-verify`).

- **C3 (per-app rollout)**: each app's cucumber harness updates assertions
  from envelope shape (`payload.X`) to flat GraphQL shape, and from the
  parameterized `catalog.list` tool to the new specifically-named tools (e.g.
  `list_deployables`). Each app's cucumber harness scenarios pass against a
  running HTTP server.

- **Final**: `cargo test --workspace` clean across both repos. Manual stdin
  smoke per app:
  ```bash
  echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | <app>-mcp
  ```
  Returns N capabilities with descriptive names and 1–3 sentence descriptions.

## Critical files

| Phase | Repo | Files |
|-------|------|-------|
| C2 | meshql-rs | `meshql-mcp/src/lib.rs` (Capability + CapabilityHandler + Builder), `meshql-mcp/src/schema.rs` (new parser), `meshql-mcp/src/catalog.rs` → renamed/removed, `meshql-mcp/src/transport.rs` (uses new config shape), unit tests across all of the above |
| C3 | manifold | `<app>/src/bin/<app>-mcp.rs` (rewritten with builder), `<app>/src/mcp/mod.rs` + `tools.rs` (custom capabilities, not `custom_tools`), `<app>/tests/features/<app>_mcp.feature` + `<app>/tests/mcp_harness.rs` (assertion updates) — 4 apps × ~6 files each |

## Open questions resolved during implementation

- **What plural form for `<entities>` in auto-generated names?** Naive `<name>s`
  for v1. Apps override via `.describe()`. A future refinement could accept a
  per-entity plural override in `auto_from_schemas` if it becomes a pain.
- **What happens when a schema's `Query` type has no operations?** Skip — that
  entity contributes no capabilities. Probably a schema bug; emit a warning
  to stderr.
- **Should `RestGet` / `RestPost` handlers also support templating?** Yes —
  `path_template` with `{id}`-style placeholders, same substitution logic as
  `GraphQuery::query_template`. Keep substitution single-pass and shared
  across handler variants.
