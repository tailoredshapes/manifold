# Cityhall — scaffold + entity implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps tagged **(delegate)** are good Qwen units once `claude-code-delegate` is restored. Architecture stays with Claude. The Mermaid Gantt emitter is the headline deliverable; budget the most thought there.

**Goal:** A new Manifold app that owns OrgNode, Bylaw, ChangeRequest, DeploymentPlan, GanttOutput. Federates Team from Union; federates Deployable from Groundwork.

**Tech Stack:** Rust 2021, axum 0.7, sqlx 0.8 (sqlite), `meshql-rs` workspace siblings, cucumber 0.21, `chrono` for time windows.

---

## File map

```
cityhall/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs
│   ├── bylaw.rs           # gate-type evaluation
│   ├── plan.rs            # ChangeRequest → DeploymentPlan resolver
│   └── gantt.rs           # DeploymentPlan → Mermaid Gantt
├── config/
│   ├── json/
│   │   ├── org_node.schema.json
│   │   ├── bylaw.schema.json
│   │   ├── change_request.schema.json
│   │   ├── deployment_plan.schema.json
│   │   └── gantt_output.schema.json
│   └── graph/
│       ├── org_node.graphql
│       ├── bylaw.graphql
│       ├── change_request.graphql
│       ├── deployment_plan.graphql
│       └── gantt_output.graphql
├── static/
│   ├── index.html
│   └── app.js
└── tests/
    ├── cityhall_cert.rs
    └── features/
        ├── org_node.feature
        ├── bylaw.feature
        ├── change_request.feature
        ├── deployment_plan.feature
        ├── gantt_output.feature
        └── web_ui.feature
```

---

## Entity definitions

### OrgNode

Hierarchy node. Forms a tree via `parent_id`. Leaf nodes (`kind == "team"`) reference a Union Team via `team_id`.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["name", "kind"],
  "properties": {
    "name":      { "type": "string" },
    "kind":      { "type": "string", "enum": ["enterprise", "division", "domain", "product", "team"] },
    "parent_id": { "type": "string" },
    "team_id":   { "type": "string" }
  },
  "additionalProperties": true
}
```

```graphql
type OrgNode {
  id: ID
  name: String!
  kind: String!
  parent_id: String
  team_id: String
}
type Query {
  getById(id: ID, at: Float): OrgNode
  getAll(at: Float): [OrgNode]
  getByKind(kind: String, at: Float): [OrgNode]
  getByParentId(parent_id: String, at: Float): [OrgNode]
  getByTeamId(team_id: String, at: Float): [OrgNode]
}
```

Invariant (validated in custom validator, not just JSON schema): if `kind == "enterprise"`, `parent_id` MUST be absent. If `kind == "team"`, `team_id` SHOULD be present (warned, not blocked). Other kinds MUST have a `parent_id`.

### Bylaw

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["org_node_id", "gate_type"],
  "properties": {
    "org_node_id": { "type": "string" },
    "gate_type":   { "type": "string", "enum": ["AutoGate", "ApprovalGate", "WindowGate", "QuiesceGate", "FreezePeriod"] },
    "priority":    { "type": "integer", "minimum": 0, "maximum": 100 },
    "description": { "type": "string" },
    "conditions":  { "type": "string", "description": "JSON-encoded condition spec; semantics depend on gate_type" },
    "window":      { "type": "string", "description": "ISO 8601 interval, e.g. 2026-04-29T09:00Z/2026-04-29T17:00Z" },
    "quiesce_for": { "type": "string", "description": "duration like '15m', '1h'" },
    "approvers":   { "type": "string", "description": "comma-separated Person IDs (resolved via Union)" }
  },
  "additionalProperties": true
}
```

`conditions` shape per gate_type (free-text JSON for now; tightened later if usage justifies):

| gate_type | conditions example |
|---|---|
| AutoGate | `{"sla_target": "0.999"}` — passes when current SLA ≥ target |
| ApprovalGate | uses top-level `approvers` field; conditions optional |
| WindowGate | uses top-level `window` field |
| QuiesceGate | uses top-level `quiesce_for`; conditions may pin `metric: "alerts"` |
| FreezePeriod | uses top-level `window`; blocks rather than gates |

```graphql
type Bylaw {
  id: ID
  org_node_id: String!
  gate_type: String!
  priority: Int
  description: String
  conditions: String
  window: String
  quiesce_for: String
  approvers: String
}
type Query {
  getById(id: ID, at: Float): Bylaw
  getAll(at: Float): [Bylaw]
  getByOrgNodeId(org_node_id: String, at: Float): [Bylaw]
  getByGateType(gate_type: String, at: Float): [Bylaw]
}
```

Bylaw layering rule (encoded in the resolver, not the storage):

1. When evaluating gates for an OrgNode, walk ancestors from the node up to the enterprise root.
2. Collect every Bylaw attached to any ancestor.
3. Sort by `priority` (highest first); within equal priority, ancestor-most wins.
4. **Higher layers cannot be loosened by lower layers**: if a parent has a `FreezePeriod` attached, no child Bylaw can clear it. Concretely: a child cannot remove a gate added by a parent. (Children may add stricter gates.)

### ChangeRequest

A proposed change. References target deployables (in Groundwork) by ID. Versions are free-text now; validated as deployable-known later via federation.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["summary"],
  "properties": {
    "summary":             { "type": "string" },
    "description":         { "type": "string" },
    "target_deployables":  { "type": "string", "description": "JSON array of deployable IDs from Groundwork" },
    "target_versions":     { "type": "string", "description": "JSON object {deployable_id: version} from Groundwork" },
    "requested_by":        { "type": "string", "description": "Person ID from Union" },
    "tier":                { "type": "string", "enum": ["dev", "integration", "uat", "prod"] },
    "status":              { "type": "string", "enum": ["draft", "submitted", "approved", "rejected", "deployed", "rolled_back"] }
  },
  "additionalProperties": true
}
```

```graphql
type ChangeRequest {
  id: ID
  summary: String!
  description: String
  target_deployables: String
  target_versions: String
  requested_by: String
  tier: String
  status: String
}
type Query {
  getById(id: ID, at: Float): ChangeRequest
  getAll(at: Float): [ChangeRequest]
  getByStatus(status: String, at: Float): [ChangeRequest]
  getByTier(tier: String, at: Float): [ChangeRequest]
}
```

Why `target_deployables` is a JSON-encoded string rather than `[String]`: meshql-rs's storage normalises arrays to scalar JSON in its current shape; we sidestep schema gymnastics by storing the encoded form. Resolvers parse it.

### DeploymentPlan

Computed artefact. Stored so consumers can fetch a stable plan without recomputing and to enable temporal queries against past plans.

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["change_request_id"],
  "properties": {
    "change_request_id": { "type": "string" },
    "tier":              { "type": "string", "enum": ["dev", "integration", "uat", "prod"] },
    "steps":             { "type": "string", "description": "JSON array of {order, deployable_id, action, predecessors, gates}" },
    "blockers":          { "type": "string", "description": "JSON array of strings" },
    "computed_at":       { "type": "string" }
  },
  "additionalProperties": true
}
```

```graphql
type DeploymentPlan {
  id: ID
  change_request_id: String!
  tier: String
  steps: String
  blockers: String
  computed_at: String
}
type Query {
  getById(id: ID, at: Float): DeploymentPlan
  getAll(at: Float): [DeploymentPlan]
  getByChangeRequestId(change_request_id: String, at: Float): [DeploymentPlan]
}
```

### GanttOutput

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "required": ["deployment_plan_id"],
  "properties": {
    "deployment_plan_id": { "type": "string" },
    "tier":               { "type": "string" },
    "mermaid":            { "type": "string", "description": "Mermaid Gantt syntax" }
  },
  "additionalProperties": true
}
```

```graphql
type GanttOutput {
  id: ID
  deployment_plan_id: String!
  tier: String
  mermaid: String
}
type Query {
  getById(id: ID, at: Float): GanttOutput
  getAll(at: Float): [GanttOutput]
  getByDeploymentPlanId(deployment_plan_id: String, at: Float): [GanttOutput]
}
```

---

## Resolvers

### `POST /change_request/<id>/plan` — compute a DeploymentPlan

Given a ChangeRequest:

1. Parse `target_deployables` (JSON array of Groundwork deployable IDs).
2. For each deployable, fetch its dependency graph from Groundwork (via federation/HTTP). Add transitively-affected deployables to the plan's deployable set.
3. For each affected deployable:
    a. If Groundwork reports it has no `team_id`, append `"orphan: <deployable_id>"` to `blockers`.
    b. Resolve its team to an OrgNode via `OrgNode.team_id == <team_id>`. Walk ancestors collecting Bylaws.
    c. Compute applicable gates per the layering rule.
4. Order steps via topological sort over Groundwork dependencies (dependencies first, dependents last).
5. Persist a DeploymentPlan record with `steps` (JSON-encoded list of `{order, deployable_id, action: "deploy"|"verify", predecessors: [...], gates: [{type, source_org_node, ...}]}`) and `blockers`.

If `blockers` is non-empty the plan is still emitted but flagged. The UI/caller decides how to surface that.

### `POST /deployment_plan/<id>/gantt` — emit a Mermaid Gantt

Given a stored DeploymentPlan:

1. Parse `steps`. Group by `deployable_id` to form Gantt **sections**.
2. Each step is a Gantt task with:
   - id: `step_<order>`
   - duration: estimated from `action` ("deploy" → 10min, "verify" → 5min — placeholders, configurable later).
   - dependencies: `after step_<predecessor_orders...>`.
3. Each gate becomes a **milestone** task immediately before the gated step. Format: `crit, milestone`.
4. Emit:

   ```mermaid
   gantt
       title ChangeRequest <summary> — <tier>
       dateFormat HH:mm
       axisFormat %H:%M
       section <deployable_name>
       gate ApprovalGate :crit, milestone, gate_<n>, after <pred>, 0min
       deploy <deployable_name> :step_<order>, after gate_<n>, 10min
       ...
   ```

5. Persist a GanttOutput record with the rendered string. Return it.

The renderer is **deterministic** — given the same plan, byte-identical output. Ordering of sections follows the plan's deployable order; within a section, steps in order.

---

## BDD scenarios

### `org_node.feature`

```gherkin
Feature: OrgNode hierarchy

  Background:
    Given a Cityhall server is running

  Scenario: Build a four-level hierarchy
    When I POST to "/org_node/api" with body {"name": "Acme", "kind": "enterprise"}
    Then the response status should be 201
    Given I capture the last id as "acme"
    When I POST to "/org_node/api" with body {"name": "Engineering", "kind": "division", "parent_id": "<saved.acme>"}
    Then the response status should be 201
    Given I capture the last id as "eng"
    When I POST to "/org_node/api" with body {"name": "Payments", "kind": "domain", "parent_id": "<saved.eng>"}
    Then the response status should be 201
    Given I capture the last id as "payments"
    When I POST to "/org_node/api" with body {"name": "Checkout Team", "kind": "team", "parent_id": "<saved.payments>", "team_id": "external-union-team-id"}
    Then the response status should be 201

  Scenario: Enterprise must not have a parent
    When I POST to "/org_node/api" with body {"name": "FloatingEnterprise", "kind": "enterprise", "parent_id": "anywhere"}
    Then the response status should be 400

  Scenario: Non-enterprise must have a parent
    When I POST to "/org_node/api" with body {"name": "FloatingDomain", "kind": "domain"}
    Then the response status should be 400

  Scenario: Find children of a node
    Given I have built the standard hierarchy
    When I query the "org_node" graph with: { getByParentId(parent_id: "<ids.payments>") { name kind } }
    Then there should be no GraphQL errors
    And the response data should contain "Checkout Team"
```

The `I have built the standard hierarchy` step is a fixture that creates Acme→Engineering→Payments→{Checkout, Billing} and stashes the IDs.

### `bylaw.feature`

```gherkin
Feature: Bylaws layered along the org chart

  Background:
    Given a Cityhall server is running
    And I have built the standard hierarchy

  Scenario: Attach an enterprise-level freeze
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "FreezePeriod", "window": "2026-12-23T00:00Z/2026-12-27T00:00Z", "description": "year-end freeze", "priority": 100}
    Then the response status should be 201

  Scenario: Cannot attach with unknown gate_type
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "VibesCheck"}
    Then the response status should be 400

  Scenario: Effective bylaws for a leaf walk all ancestors
    Given enterprise "<ids.acme>" has a "FreezePeriod" bylaw
    And domain "<ids.payments>" has an "ApprovalGate" bylaw with approvers "person-abc"
    And team "<ids.checkout>" has a "QuiesceGate" bylaw with quiesce_for "15m"
    When I GET "/org_node/<ids.checkout>/effective_bylaws"
    Then the response status should be 200
    And the response body should contain "FreezePeriod"
    And the response body should contain "ApprovalGate"
    And the response body should contain "QuiesceGate"
    And the bylaws should be ordered by ancestor depth (root first)

  Scenario: A child cannot loosen a parent bylaw
    Given enterprise "<ids.acme>" has a "FreezePeriod" bylaw with priority 100
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.checkout>", "gate_type": "AutoGate", "priority": 100, "conditions": "{\"override_parent\": true}"}
    And I GET "/org_node/<ids.checkout>/effective_bylaws"
    Then the response body should contain "FreezePeriod"
```

The last scenario asserts the rule "higher layers cannot be overridden": the child's AutoGate is *added* to the chain but the parent's FreezePeriod still applies.

### `change_request.feature`

```gherkin
Feature: Change requests

  Background:
    Given a Cityhall server is running

  Scenario: Submit a minimal change request
    When I POST to "/change_request/api" with body {"summary": "bump checkout to v1.2.3"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Status moves through the workflow
    Given I have submitted change request "bump-checkout"
    When I PUT "/change_request/api/<ids.bump-checkout>" with body {"summary": "bump checkout to v1.2.3", "status": "submitted"}
    Then the response status should be 200
    And the response body should contain "submitted"

  Scenario: Reject invalid status
    When I POST to "/change_request/api" with body {"summary": "x", "status": "yolo"}
    Then the response status should be 400
```

### `deployment_plan.feature`

```gherkin
Feature: Deployment plan computed from a change request

  Background:
    Given a Cityhall server is running
    And I have built the standard hierarchy
    And I have a change request "deploy-checkout-v2" with target deployables ["dep-checkout"]

  Scenario: Compute a plan with no bylaws — direct deploy
    When I POST to "/change_request/<ids.deploy-checkout-v2>/plan" with body {"tier": "dev"}
    Then the response status should be 201
    And the response body should contain "change_request_id"
    And the plan should have 1 step
    And the plan step 0 should be "deploy dep-checkout"

  Scenario: Compute a plan with an enterprise FreezePeriod adds a gate to every step
    Given enterprise "<ids.acme>" has a "FreezePeriod" bylaw with window "now/+1d"
    When I POST to "/change_request/<ids.deploy-checkout-v2>/plan" with body {"tier": "prod"}
    Then the response status should be 201
    And the plan step 0 should have a "FreezePeriod" gate

  Scenario: Compute a plan with no team for a deployable — orphan blocker
    Given the deployable "dep-orphan" has no team
    And the change request "deploy-orphan" targets "dep-orphan"
    When I POST to "/change_request/<ids.deploy-orphan>/plan" with body {"tier": "prod"}
    Then the response status should be 201
    And the plan blockers should contain "orphan: dep-orphan"
```

The first BDD pass uses **mocked Groundwork**: a fake HTTP server in the cucumber harness that returns deterministic deployable+team payloads. Real federation is wired in the federation phase.

### `gantt_output.feature`

```gherkin
Feature: Mermaid Gantt output from deployment plan

  Background:
    Given a Cityhall server is running
    And I have computed a deployment plan with 2 sequential steps and one ApprovalGate

  Scenario: Generate Gantt for a single-step plan
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response status should be 201
    And the response body should contain "gantt"
    And the response body should contain "title"
    And the response body should contain "dateFormat"
    And the response body should contain "section"

  Scenario: Each step appears as a Gantt task
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response body should contain "step_0"
    And the response body should contain "step_1"

  Scenario: ApprovalGate appears as a milestone
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response body should contain ":crit, milestone"
    And the response body should contain "ApprovalGate"

  Scenario: Sections group steps by deployable
    Given the plan involves two deployables "dep-a" and "dep-b"
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response body should contain "section dep-a"
    And the response body should contain "section dep-b"
```

The Gantt output is deterministic, so an exact-match check is also valuable:

```gherkin
  Scenario: Deterministic output
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    And I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then both responses should be byte-equal
```

---

## Slices

### Slice 0 — workspace bootstrap

- [ ] Add `"cityhall"` to `manifold/Cargo.toml`.
- [ ] Mirror `groundwork/Cargo.toml` for `cityhall/Cargo.toml` plus `chrono = { version = "0.4", features = ["serde"] }` (for window evaluation).
- [ ] `cityhall/README.md` from the manifold/README.md Cityhall section.
- [ ] `cargo build --workspace` clean.
- [ ] Commit.

### Slice 1 — entity wiring + custom validators

Mirror the Groundwork main.rs pattern for all five entities. Default port `PORT=3002`. JSON-Schema enum validation for `gate_type`, `OrgNode.kind`, `ChangeRequest.status`, etc.

Custom validator extras:

- `OrgNode`: enterprise-without-parent / non-enterprise-with-parent rules.
- `Bylaw`: gate_type-specific required fields (`WindowGate` requires `window`; `ApprovalGate` requires `approvers`; `QuiesceGate` requires `quiesce_for`).

Done as a single `make_cityhall_validator(entity_name, schema)` returning `ValidatorFn`.

- [ ] Implement.
- [ ] BDD scenarios for org_node, bylaw, change_request green.
- [ ] Commit.

### Slice 2 — bylaw layering resolver

A small route family beyond the standard CRUD:

- `GET /org_node/:id/ancestors` — returns the ancestor chain root-first.
- `GET /org_node/:id/effective_bylaws` — collects bylaws along the ancestor chain, returns root-first ordered list.

These do not need dedicated entities; they're computed routes that fan out via the existing repos.

- [ ] Implement `effective_bylaws_for(node_id)` helper in `src/bylaw.rs`.
- [ ] Add the routes.
- [ ] BDD scenarios green.
- [ ] Commit.

### Slice 3 — DeploymentPlan resolver (delegate-friendly)

The plan resolver has two halves:

1. **Groundwork client** (delegate): a thin reqwest client that fetches Deployable + dependency edges from Groundwork. Mirror Phase 5 MCP plan's `GroundworkClient`.
2. **Plan builder** (Claude): topological sort + gate assembly. The algorithm matters.

In tests, the Groundwork client is replaced with a stub that reads from a fixture map.

- [ ] Define `src/plan.rs::compute_plan(repos, groundwork_client, change_request) -> DeploymentPlan`.
- [ ] Define `POST /change_request/:id/plan` route that calls compute_plan and persists to the DeploymentPlan repo.
- [ ] BDD scenarios green using the stub.
- [ ] Commit.

### Slice 4 — Mermaid Gantt emitter (the headline) (delegate the renderer half)

The renderer is mostly text construction over a typed `Plan` struct. Suitable for delegation.

**Delegate prompt:**

> Implement `pub fn render_gantt(plan: &Plan) -> String` in `cityhall/src/gantt.rs` where `Plan` is:
>
> ```rust
> pub struct Plan {
>     pub change_request_summary: String,
>     pub tier: String,
>     pub steps: Vec<Step>,
> }
> pub struct Step {
>     pub order: usize,
>     pub deployable_name: String,
>     pub action: String,        // "deploy" | "verify"
>     pub predecessor_orders: Vec<usize>,
>     pub gates: Vec<Gate>,
>     pub estimated_minutes: u32,
> }
> pub struct Gate {
>     pub gate_type: String,
>     pub source_org_node: String,
> }
> ```
>
> Output Mermaid Gantt syntax. Layout:
>
> ```mermaid
> gantt
>     title ChangeRequest <summary> — <tier>
>     dateFormat HH:mm
>     axisFormat %H:%M
>     section <deployable_name 1>
>     <gate milestones> :crit, milestone, gate_<step.order>_<gate.idx>, after step_<pred> | 0min, 0min
>     <action> <deployable_name> :step_<order>, after step_<preds...> or after gate_<step.order>_<gate.idx>, <minutes>min
>     section <deployable_name 2>
>     ...
> ```
>
> Sections appear in the order their first step's `order` field appears. Within a section, steps in order. If a step has gates, emit each gate as a `:crit, milestone` line *before* the step, then the step depends on the gates.
>
> Determinism contract: `render_gantt(p) == render_gantt(p)` byte-for-byte for the same input.
>
> Inline unit tests covering at least:
> - one step, no gates, no predecessors;
> - two steps in the same section with a predecessor edge;
> - two steps in different sections;
> - a step with two gates, one ApprovalGate and one WindowGate.
>
> Run `cargo test --lib gantt` clean.

- [ ] Define the `Plan` shape (Claude).
- [ ] Delegate the renderer.
- [ ] Define `POST /deployment_plan/:id/gantt` route that loads the plan, materialises a `Plan` struct, calls `render_gantt`, persists a GanttOutput, returns it.
- [ ] BDD scenarios green.
- [ ] Commit.

### Slice 5 — Web UI

Mirror Groundwork. Sidebar entries: `org-nodes`, `bylaws`, `change-requests`, `plans`, `gantts`. Each entity gets a `dynamic-select` for FK fields (e.g. `bylaw.org_node_id` selects from `data.org_nodes`).

The interesting UI piece: the `gantts` view should render the Mermaid string. For v0.1 of the UI, just show the raw text in a `<pre>`. Live rendering (with `mermaid.js` from CDN) is a future polish.

- [ ] Implement.
- [ ] Manual smoke; commit.

### Definition of done

- [ ] `cargo test --workspace` passes.
- [ ] `cargo run -p cityhall` boots on `:3002`.
- [ ] All five entity CRUDs round-trip via curl.
- [ ] Effective bylaws walk works for a 4-deep hierarchy.
- [ ] `POST /change_request/:id/plan` produces a DeploymentPlan against a stub Groundwork.
- [ ] `POST /deployment_plan/:id/gantt` produces valid Mermaid that renders in https://mermaid.live (manual paste check).
- [ ] No regression in Groundwork or Union.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Storing JSON-encoded arrays as strings is awkward to query. | Acceptable for v0.1: meshql-rs's value-store treats payloads as opaque. If we need to query "all change_requests targeting deployable X", we'll add a join entity (e.g. `ChangeRequestTarget`) in v0.3. |
| Mermaid Gantt syntax has dialect quirks across renderers (mermaid-cli, mermaid.live, GitHub). | Stick to the most-supported subset (no advanced dependency syntax beyond `after`). Determinism + manual paste-check at https://mermaid.live is the gate. |
| Bylaw layering "higher cannot be overridden" can be modelled in too many ways. | Lock in: a child Bylaw is *additive*. Children can add stricter gates; they cannot mark a parent gate as not applicable. The resolver collects everything; the merge is union, not override. |
| ChangeRequest fanout via Groundwork dependencies could explode for large graphs. | Phase 5 MCP plan already caps depth at 10 with cycle detection. Reuse the same shape. |
| Time windows: "now/+1d" syntax is invented. | Use ISO 8601 intervals only (`<start>/<end>`) for stored bylaws; introduce a relative-time helper later if useful. |
