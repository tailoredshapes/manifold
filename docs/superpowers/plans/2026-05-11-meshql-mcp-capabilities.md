# meshql-mcp Capabilities + Auto-Derivation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `meshql-mcp`'s current `EntityConfig` + 3 generic `catalog.*` tools with a `Capability`-based model where each tool is a specifically-named, well-described operation. Add schema-driven auto-derivation so per-app configuration is declarative and terse. Roll out across all 4 manifold apps.

**Architecture:** Per spec at `docs/superpowers/specs/2026-05-11-meshql-mcp-capabilities-design.md`. `meshql-mcp` exports `Capability` (with a `CapabilityHandler` enum covering GraphQL queries, REST GET / POST templates, and custom escape hatch), a small schema parser for the meshql GraphQL dialect, and a `CapabilitiesBuilder` that takes a slice of `(entity_name, graph_path, schema_text)` tuples plus per-name description overrides and a `.add(custom)` chain. Each app's `<app>-mcp.rs` bin becomes a declarative capability catalog (~50 LOC). The current crate-level `gql()` method on `MeshqlClient` (from R1, commit `e1608d6` on meshql-rs main) is the underlying primitive.

**Tech Stack:** Rust 2021, tokio, serde_json, anyhow. No new dependencies — schema parser is a hand-rolled regex/state-machine walker over the meshql GraphQL subset (~80 LOC).

**Workflow:** Trunk-Based Development across both `meshql-rs` and `manifold`. Direct commits to `main`. No `--no-verify` on either repo. The meshql-rs pre-push hook now passes cleanly (fixed at `f15155f`); allow ~10 minutes per push.

**Cross-repo ordering:** C2 commits land in meshql-rs first. Then manifold's path-deps see the new API, and C3 rolls out across the four apps.

---

## C2 — meshql-mcp redesign (Tasks 1–6)

Repo: `/tank/repos/tailoredshapes/meshql-rs`. Adds Capability + schema parser; removes EntityConfig and the old catalog tools generator.

### Task 1 — Schema parser

**Files:** `meshql-mcp/src/schema.rs` (new), `meshql-mcp/src/lib.rs` (re-export)

The parser walks the meshql GraphQL subset and extracts:

- Principal entity type: `type <Name> { <fields> }`. Fields can be scalars (`name: String`), references (`team_id: String`), or federated projections (`team: Team`).
- A separate `type Query { ... }` block. Operations follow the meshql naming conventions (`getById(id: ID, at: Float): Entity`, `getAll(at: Float): [Entity]`, `getByName(name: String, at: Float): [Entity]`, `getByXId(x_id: String, at: Float): [Entity]`).

```rust
pub struct ParsedSchema {
    pub entity_name: String,        // "Deployable"
    pub entity_fields: Vec<EntityField>,
    pub query_ops: Vec<QueryOp>,
}

pub struct EntityField {
    pub name: String,               // "name", "team"
    pub type_text: String,          // "String!", "Team"
    pub is_federated: bool,         // true when type is another `type X { ... }` in the same file
    pub federation_subselection: Option<String>,  // pre-rendered "{ id name kind }" for embedded types
}

pub struct QueryOp {
    pub name: String,               // "getById", "getByDeployableId"
    pub args: Vec<(String, String)>,// [("id", "ID"), ("at", "Float")]
    pub returns_list: bool,         // [Entity] vs Entity
}

pub fn parse_meshql_schema(text: &str) -> anyhow::Result<ParsedSchema>;
```

- [ ] **Step 1: Write the parser.** Use a simple line-based walker: strip comments, track open brace state, capture `type X {` blocks. The query ops block is just another `type Query { ... }`. Field lines are `<name>: <type>` (optionally `<type>!`). The `at: Float` argument is meshql's standard temporal parameter — preserved but irrelevant to auto-generation; do NOT surface it in the input_schema.

- [ ] **Step 2: Helper — render the entity's field selection as a query body.** For each entity field:
  - Scalar: include the name (`id`, `name`).
  - Federated: include `name { <sub-selection> }` where `<sub-selection>` is the federated type's scalar fields (recursively, one level deep — `team { id name kind description }`).

  Result: a `&str` ready to drop into `{ getAll { <here> } }`.

- [ ] **Step 3: Unit tests** (in `schema.rs`'s `#[cfg(test)] mod tests`):
  - `parses_deployable_schema_with_federation` — uses the actual `deployable.graphql` text inlined as a test fixture; asserts `entity_name == "Deployable"`, fields include `team` with federation, 3 query ops detected.
  - `parses_dependency_schema_with_getbyfk` — `getByDeployableId` and `getByServiceId` detected with the right arg names.
  - `bails_on_malformed_schema` — asserts `parse_meshql_schema("garbage")` returns an Err with an informative message.
  - `renders_field_selection_with_one_level_federation` — `team { id name kind description }` produced for a Deployable field set.

- [ ] **Step 4: Commit.**

```
feat(meshql-mcp): meshql GraphQL schema parser

Tiny hand-rolled walker over the meshql GraphQL dialect. Extracts the
principal entity type, its fields (including federated projections
expanded one level deep), and the Query type's operations. Used by
CapabilitiesBuilder to auto-generate baseline capabilities.

No new dependencies — the meshql subset is narrow enough that ~80 LOC
of state-machine matching covers everything we use.
```

Push (~10min hook).

### Task 2 — `Capability` + `CapabilityHandler` + `MeshqlMcpServer::new` adaptation

**Files:** `meshql-mcp/src/lib.rs` (rename module structure), new `meshql-mcp/src/capability.rs`, `meshql-mcp/src/transport.rs` (update config)

- [ ] **Step 1: Introduce `Capability` and `CapabilityHandler`** per the spec. `Capability` is roughly `Tool` extended with a typed handler instead of a free-form closure. Note: `Tool` stays — it's the lower-level primitive `MeshqlMcpServer` uses internally. `Capability::into_tool(self, client: Arc<MeshqlClient>) -> Tool` performs the conversion based on `CapabilityHandler` variant.

  - `GraphQuery { path, query_template }` → closure that substitutes args into the template, posts via `client.gql(&path, &substituted)`.
  - `RestGet { path_template }` → closure that substitutes args into the path, calls `client.get_path(&substituted)`.
  - `RestPost { path_template, body_template }` → substitutes path and body, calls `client.post_path`.
  - `Custom(handler)` → unwraps directly to a `Tool`.

- [ ] **Step 2: Substitution logic.** Single shared function that takes a template + input JSON and produces the substituted string. Placeholders are `{<arg_name>}`. String values are GraphQL-escaped (replace `"` → `\"`, `\` → `\\`) before insertion. JSON numbers / booleans / nulls insert as their JSON representation. Missing placeholders → error.

- [ ] **Step 3: Change `McpServerConfig`.**

  ```rust
  pub struct McpServerConfig {
      pub server_name: String,
      pub server_version: String,
      pub client: Arc<MeshqlClient>,
      pub capabilities: Vec<Capability>,    // was: entities: Vec<EntityConfig>
  }
  ```

  In `MeshqlMcpServer::new`, transform each `Capability` into its `Tool` representation using the cloned `client`.

- [ ] **Step 4: Remove `EntityConfig`** from the public API (and remove its definition from `transport.rs` / wherever R1 placed it). The replacement is `Capability`.

- [ ] **Step 5: Update existing unit tests.** `transport::tests::tools_list_returns_configured_tools` and others that built `McpServerConfig` now construct `Capability` instances. Use straightforward `Capability` literals or a small helper.

- [ ] **Step 6: Commit.**

```
feat(meshql-mcp): Capability + CapabilityHandler replace EntityConfig

Each tool exposed via tools/list is now a named, described, schema-typed
Capability. CapabilityHandler covers GraphQuery / RestGet / RestPost
declaratively (with shared placeholder substitution) plus a Custom
escape hatch for tools whose logic doesn't fit a template. EntityConfig
removed.

McpServerConfig.entities → McpServerConfig.capabilities. The previous
auto-generated catalog.list/get/search trio for every entity is gone —
apps now configure specifically-named capabilities, with auto-derivation
arriving in the next commit to keep the per-app surface terse.
```

Push.

### Task 3 — `CapabilitiesBuilder` + auto-derivation

**Files:** `meshql-mcp/src/capability.rs` (extend)

- [ ] **Step 1: `CapabilitiesBuilder` per the spec.** Owns a `Vec<Capability>`; methods chain by `self`.

- [ ] **Step 2: `auto_from_schemas` implementation.** For each `(entity_name, graph_path, schema_text)` tuple, parse the schema, walk its `query_ops`, and for each op generate a Capability following the naming table in the spec:

  | Op pattern               | Capability name                    | Default description                |
  |--------------------------|------------------------------------|------------------------------------|
  | `getAll`                 | `list_<entities>`                  | "List every <entity>..."           |
  | `getById(id)`            | `get_<entity>_by_id`               | "Fetch one <entity> by UUID..."    |
  | `getByName(name)`        | `find_<entities>_by_name`          | "Find <entities> by name..."       |
  | `getByXId(x_id)`         | `<entities>_for_<x>`               | "List <entities> related to..."    |

  Each generated capability uses the entity's full field selection (with federation expansion) for `query_template`. Input schema is derived from the op's arg names (excluding `at`).

  Naive pluralization: append `s` (apps override via `.describe` if it reads wrong).

- [ ] **Step 3: `.describe(name, text)`.** Find the capability with the matching name; replace its description. If no match, eprintln a warning ("describe: no capability named ..."), continue.

- [ ] **Step 4: `.add(capability)`.** Append to the list. No de-dup; if an app adds a name that already exists, the later one wins (or we error — TBD; for v1, log a warning and replace).

- [ ] **Step 5: `.build()`.** Returns the `Vec<Capability>`.

- [ ] **Step 6: Unit tests:**
  - `auto_derives_list_get_search_from_deployable_schema` — asserts the 3 capabilities exist with expected names + input schemas.
  - `auto_derives_for_by_fk_from_dependency_schema` — `services_for_deployable` or `dependencies_for_service` (depending on the schema's actual op name) appears with the right arg.
  - `describe_overrides_default` — calling `.describe("list_deployables", "X")` results in that capability's description being "X".
  - `describe_on_typo_warns_but_continues` — calling `.describe("typo", "X")` is a no-op (verify via stderr capture or just by absence of effect).
  - `add_custom_capability_appears_in_build` — confirms the custom is in the final list.

- [ ] **Step 7: Commit.**

```
feat(meshql-mcp): CapabilitiesBuilder + schema-driven auto-derivation

Apps configure their MCP server with a small declarative slice:

  CapabilitiesBuilder::new()
      .auto_from_schemas(&[(entity, path, schema), ...])
      .describe("list_X", "...")  // override the bland defaults
      .add(custom_capability)
      .build()

auto_from_schemas walks each schema's Query type and produces one
Capability per operation following naming conventions (list_<E>,
get_<E>_by_id, find_<E>s_by_name, <E>s_for_<fk>). Each gets a default
description naming the entity and the operation; apps override
selectively via `.describe`.
```

Push.

### Task 4 — Remove the old `catalog` module

**Files:** `meshql-mcp/src/catalog.rs` (delete), `meshql-mcp/src/lib.rs` (drop re-export)

The auto-derivation makes the old `catalog::tools(entities)` helper redundant. Delete it.

- [ ] **Step 1: Delete `catalog.rs`.**
- [ ] **Step 2: Remove `pub mod catalog;` from `lib.rs`.**
- [ ] **Step 3: Remove the re-export of `catalog::*` from `lib.rs`** if present.
- [ ] **Step 4: `cargo build -p meshql-mcp` clean.** `cargo test -p meshql-mcp` clean (any tests that touched `catalog::tools` should already have been updated in Task 2's test sweep, or this is the natural moment to clean them up).
- [ ] **Step 5: Commit.**

```
refactor(meshql-mcp): drop the catalog::tools generator

Superseded by CapabilitiesBuilder::auto_from_schemas. The catalog
module's specific helpers (catalog.list / catalog.get / catalog.search
as generic parameterized tools) had to give way to specifically-named
per-entity capabilities — generic tools weren't giving LLMs enough
selection signal in tools/list.
```

Push.

### Task 5 — `MeshqlClient` cleanup pass

**Files:** `meshql-mcp/src/client.rs`

R1 introduced `gql()` alongside the original REST methods. Confirm all still serve a real purpose:

- `list(entity)` — used by groundwork's snapshot loader (`graph.rs`). KEEP.
- `get(entity, id)` — KEEP. Available for custom capabilities that prefer REST.
- `get_path(path)` — used by RestGet handlers. KEEP.
- `post_path(path, body)` — used by RestPost handlers. KEEP.
- `gql(path, query)` — used by GraphQuery handlers. KEEP.

No deletions. Add a one-paragraph module-level doc summarizing which methods to use when (graph for reads; REST for writes/custom computed endpoints).

- [ ] **Step 1: Doc update only.**
- [ ] **Step 2: Commit.**

```
docs(meshql-mcp): document when to use each MeshqlClient method
```

Push.

### Task 6 — Crate-level docs

**Files:** `meshql-mcp/src/lib.rs` (top-level `//!` comment), `meshql-mcp/README.md` (new)

The crate now has a coherent story worth a short README. Top-level lib doc covers:

- What the crate does (stdio JSON-RPC MCP server for meshql-rs deployments).
- The two layers: low-level `Tool` + transport, high-level `Capability` + builder.
- Example usage (the groundwork-mcp bin sketch from the spec).
- The auto-derivation conventions table.

README mirrors this for the crates.io / GitHub readers.

- [ ] **Step 1: Write the docs.**
- [ ] **Step 2: Commit.**

```
docs(meshql-mcp): top-level guide + auto-derivation conventions
```

Push.

---

## C3 — Per-app rollout (Tasks 7–10)

Repo: `/tank/repos/tailoredshapes/manifold`. Each app's `<app>-mcp.rs` becomes a CapabilitiesBuilder declaration. Cucumber harnesses get wire-format updates.

### Task 7 — Groundwork

**Files:**
- `groundwork/src/bin/groundwork-mcp.rs` — rewrite using `CapabilitiesBuilder::auto_from_schemas` over the 6 schemas + 3 custom capabilities for the graph tools.
- `groundwork/src/mcp/tools/mod.rs` (renaming to clearer location if needed) — the 3 graph tools become `pub fn blast_radius(client) -> Capability`, etc.
- `groundwork/src/mcp/graph.rs` — Snapshot loader switches from `client.list(entity)` to `client.gql(...)`. Use the schema parser's field-selection helper or inline a minimal `{ getAll { id name ... } }` per entity (just the fields the snapshot needs).
- `groundwork/tests/features/mcp_tools.feature` — scenarios renamed to use the new capability names (`list_deployables` instead of `catalog.list { entity: "deployable" }`); response-shape assertions updated from envelope (`payload.X`) to flat (`X`).
- `groundwork/tests/mcp_harness.rs` — assertion sweep.

Three commits:
1. **`feat(groundwork): MCP server uses Capability builder`** — bin + custom_capabilities module.
2. **`refactor(groundwork): MCP snapshot loader reads via /graph`** — graph.rs internal switch.
3. **`test(groundwork): cucumber harness updates for capability names + flat shape`** — features + harness.

Verify after each: `cargo build -p groundwork` clean. After commit 3: `cargo test -p groundwork --test mcp_harness` → 8 scenarios pass against the new wire format.

### Task 8 — Union

Same shape, smaller surface (no snapshot loader). 2 commits:
1. `feat(union): MCP server uses Capability builder` — bin + custom_capabilities (team_capacity, team_members, person_assignments stay; refactor to `Capability` instances).
2. `test(union): cucumber harness updates for capability names + flat shape`.

### Task 9 — Cityhall

Same shape. 2 commits.

### Task 10 — Yard

Same shape. 2 commits.

### Task 11 — Final cross-cutting verification

- `cargo test --workspace` in manifold clean.
- Stdin smoke-check each `<app>-mcp` binary: `tools/list` returns the expected number of capabilities with descriptive names. Verify a few descriptions read well.
- Confirm the auto-derived capability names are sensible across all 4 apps (no awkward plurals; consistent style). If any read wrong, add `.describe()` overrides to the affected bins.

---

## Out of scope (explicit)

- **GraphQL `variables` parameterization** — v1 uses string substitution with escaping. Migrating to `variables` is a clean follow-up that hardens injection protection but adds API surface.
- **Per-capability field trimming** — auto-derived capabilities use full entity fields (with federation). Apps can replace whole capabilities via custom `.add()` if they need a trimmed shape; no `.with_fields` shortcut in v1.
- **Authentication pass-through** — same deferred initiative as before.
- **HTTP/SSE MCP transport** — stdio only.
- **MCP `resources` / `prompts`** — tools only.
- **Promoting `meshql-mcp` to crates.io** — separate maintenance question.
- **Smart pluralization** — naive `<entity>s`. Apps override.
