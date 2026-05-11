# Meridian Demo Polish — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the demo-gap critique by populating seed data, adding cross-app navigation links, and implementing four feature improvements (deployment_status on Deployable, story_points on WorkOrder, staffing-dots polish, Cytoscape-based dependency graph).

**Architecture:** Three-phase rollout per spec `docs/superpowers/specs/2026-05-11-meridian-demo-polish-design.md`. Phase A is fixture-only (no app code). Phase B is per-app cross-linking. Phase C adds two scalar entity fields (deployment_status, story_points) plus the staffing UI polish and the new graph tab. Each phase is independently shippable; Trunk-Based Development throughout (direct commits to `main`, no PRs, no feature branches).

**Tech Stack:** Rust workspace (groundwork, union, cityhall, yard), MeshQL-RS, vanilla JS frontends, Cytoscape.js (new — vendored as a single file for the graph viz only). Cucumber BDD tests for backend verification. Manual verification for frontend (browser smoke checks gated behind Cloudflare Access SSO).

**Workflow rules:**
- Trunk-Based Development. Every commit lands directly on `main` and is pushed.
- Each entity / feature gets its own commit. No batching unrelated changes.
- CQRS rule from prior migration holds: writes → REST `/<entity>/api`; reads → GraphQL `/<entity>/graph`.
- Frontend ground rules: vanilla, accessible, responsive, semantic CSS.
- No PRs/MRs. No feature branches.

---

## Phase A: Seed-data pass (Tasks 1–4)

Phase A is a focused rebuild of the Meridian fixture. No app-code changes. Phase A must land **before** any of Phase C's seed-touching tasks because C1 (deployment_status) and C2 (story_points) add new fields that the fixture seeds.

### Task 1: Audit current fixture + plan the rewrite

**Files (read-only):**
- `data/meridian_fixture.json` — current snapshot
- `data/seed_meridian.py` — loader
- `data/load_fixture.py` — alternate loader / helper
- `data/fix_meridian_fixture.py` — prior fixture-fixing script (if relevant)
- `scripts/reset-data.sh` — stack reset

- [ ] **Step 1: Map the current fixture's entity counts and gaps**

```bash
cd /tank/repos/tailoredshapes/manifold
jq 'with_entries({key, value: (.value | length)})' data/meridian_fixture.json
```

Expected output: counts per entity (deployables, services, dependencies, exposes, contracts, slas, persons, teams, team_members, work_orders, org_nodes, bylaws, change_requests, deployment_plans, gantt_outputs, test_environments, test_infrastructures, mock_sources, data_sources, data_syncs, test_runs, test_suites). Note which are zero / sparse.

- [ ] **Step 2: Confirm gaps match the critique**

Cross-reference against the critique items in `docs/superpowers/specs/2026-05-11-meridian-demo-polish-design.md` Phase A section. Confirm: zero or near-zero in `work_orders` Done status, `test_runs`, `deployment_plans`, `gantt_outputs`, org_nodes beyond the root. Confirm the 17 persons all carry the same "stretched" indicator.

- [ ] **Step 3: Choose the headline change request**

Pick one of the 5 existing ChangeRequests as the narrative-coherence anchor. It should:
- Have at least 2 plausible target deployables
- Be owned by a team with at least 3 people
- Be at a tier (dev/uat/prod) that makes a Gantt plan meaningful
- Not have status `approved` (so the plan-on-submit flow is exercisable)

Document the choice in a comment at the top of `data/meridian_fixture.json` (one line: `_narrative_change_request: "cr-<id>"`).

No commit yet — this is preparation for Task 2.

### Task 2: Rebuild the fixture content

**Files:**
- Modify: `data/meridian_fixture.json` (substantial rewrite of multiple sections)
- Possibly modify: `data/seed_meridian.py` if new entity shapes need helper functions

Targets per the spec:

**WorkOrder distribution** (currently ~7; expand to ~20):
- 4 in `proposed` status
- 6 in `in_progress`
- 1 in `blocked`
- ~8–9 in `done` (with `completed_at` timestamps within the last 6 weeks)
- Distribute across all 5 teams; vary priorities (low/medium/high); reference real deployable_ids from the fixture

**TestRun history** (currently 0; generate ~100):
- ~100 records distributed across the 23 test_environments
- Date range: past 8 weeks
- Status distribution: ~60% `passed`, ~20% `failed`, ~10% `cancelled`, ~8% `errored`, ~2% currently `running`
- Vary duration_minutes (5–240, log-normal distribution looks realistic), cost_actual (computed from environment's cost_per_hour × duration)
- ~20% of runs reference one of the 5 ChangeRequests via change_request_id
- ~80% reference a team_id (the team that owns the deployable being tested)
- ~50% reference a test_suite_id (we may need to also expand the test_suites entity from 0 to ~5–8 to support this — see below)

**TestSuites** (currently 0; add ~6):
- One per major deployable type (e.g., `customer-api-suite`, `driver-app-suite`, `dispatch-console-suite`, `legacy-crm-suite`, `tracking-suite`, `payments-suite`)
- Reference deployable_id; include realistic runner (`jest`, `pytest`, `playwright`, `gradle`) and command strings

**DataSources, DataSyncs** (currently 0; add 3–4 of each):
- 3 DataSources: `meridian-prod-snapshot` (kind: prod_snapshot), `synthetic-shipments` (kind: synthetic), `fedex-mock` (kind: external_mock)
- 4 DataSyncs: connecting prod-snapshot → uat sandbox (kind: pull, refresh: periodic), synthetic → isolated env (kind: push), etc.

**DeploymentPlans** (currently 0; pre-compute for the 5 ChangeRequests):
- Run `cityhall`'s existing planning logic (`cityhall/src/plan.rs`) for each ChangeRequest. Capture the resulting steps/gates/windows/blockers and inline them in the fixture.
- See [Open question #4 in the spec]: pre-computing for the fixture may bake stale data. Verify by reading `cityhall/src/plan.rs` — if the computation is deterministic from the inputs the fixture provides, baking it in the fixture is safe. If it computes from runtime state (current time, current bylaw evaluations), defer this and instead arrange for `seed_meridian.py` to trigger plan computation on first load.

**GanttOutputs** (currently 0; one per DeploymentPlan once C above lands):
- Render Mermaid for each plan. Inline `mermaid` text and `tier` per the entity shape.

**Org tree expansion** (currently 1 visible node; add ~15–20):
- "Meridian Freight Solutions" (enterprise) is the existing root
- Add divisions: "Customer Operations" (division), "Driver Operations" (division), "Platform Engineering" (division)
- Add domains under each: Customer Operations → "Customer-Facing Apps", "Customer Support Tools"; Driver Operations → "Driver Apps", "Dispatch"; Platform Engineering → "Infrastructure", "Data Platform"
- Add products under each domain
- Leaf nodes (kind: `team`) reference the existing 5 Union.Team ids via team_id

**Bylaws on the org tree** (currently 6; pin them to the new tree):
- Re-attach the existing 6 bylaws to plausible org_nodes throughout the hierarchy (not all at the root)
- The freeze-window bylaw → enterprise root (cascades to all)
- Quiesce-gate bylaw → Platform Engineering division
- Approval-gate for auth changes → leaf team that owns Auth Service
- UAT sign-off → Customer-Facing Apps domain

**Person availability diversification:**
- The Person entity currently has every record carrying a "stretched" indicator. First, check whether this is a stored field (`availability` or `status`) or a derived signal (computed from WorkOrder load).
- If stored: rewrite distribution to ~60% available, ~25% stretched, ~15% pto/unavailable.
- If derived: this is a Phase A no-op — fixing the WorkOrder distribution (above) will naturally diversify the derived signal.

- [ ] **Step 1: Read the spec's Phase A section carefully + the current fixture's shape**

```bash
cat docs/superpowers/specs/2026-05-11-meridian-demo-polish-design.md | sed -n '/Phase A/,/Phase B/p'
head -300 data/meridian_fixture.json
```

- [ ] **Step 2: Verify whether "stretched" is stored or derived**

```bash
jq '.persons[0] | keys' data/meridian_fixture.json
grep -rn 'stretched\|availability\|capacity' union/static/app.js union/config/ | head -20
```

If `availability` (or similar) is a payload field, it's stored. If `stretched` only appears in rendering logic computed from work_order counts, it's derived.

- [ ] **Step 3: Verify DeploymentPlan pre-compute strategy**

```bash
head -100 cityhall/src/plan.rs
grep -n 'fn compute_plan\|fn build_plan\|fn plan_for' cityhall/src/plan.rs
```

If the plan is a pure function of the change request + applicable bylaws + target deployables (all in the fixture), pre-compute and inline. If it consumes runtime state (current time, currently-applicable freeze windows relative to now()), arrange for `seed_meridian.py` to trigger the computation against the just-loaded data.

- [ ] **Step 4: Rewrite the fixture in one pass**

This is a large mechanical edit. Best approach: hold the whole new fixture content in memory and write it. Validate well-formedness:

```bash
jq empty data/meridian_fixture.json
```

- [ ] **Step 5: Reset the stack and seed**

```bash
./scripts/reset-data.sh
```

(If `reset-data.sh` doesn't exist or doesn't do what's expected: check what does, possibly `docker compose down -v && docker compose up -d --build && python data/seed_meridian.py`).

- [ ] **Step 6: Verify counts via curl against each app**

```bash
echo "--- groundwork ---"
for e in deployable service dependency exposes contract sla; do
  printf "  %s: " "$e"
  curl -sS -X POST "http://localhost:3050/$e/graph" -d "{\"query\":\"{ getAll { id } }\"}" | jq '.data.getAll | length'
done

echo "--- union ---"
for e in person team team_member work_order; do
  printf "  %s: " "$e"
  curl -sS -X POST "http://localhost:3051/$e/graph" -d "{\"query\":\"{ getAll { id } }\"}" | jq '.data.getAll | length'
done

echo "--- cityhall ---"
for e in org_node bylaw change_request deployment_plan gantt_output; do
  printf "  %s: " "$e"
  curl -sS -X POST "http://localhost:3052/$e/graph" -d "{\"query\":\"{ getAll { id } }\"}" | jq '.data.getAll | length'
done

echo "--- yard ---"
for e in test_environment test_infrastructure mock_source data_source data_sync test_run test_suite; do
  printf "  %s: " "$e"
  curl -sS -X POST "http://localhost:3053/$e/graph" -d "{\"query\":\"{ getAll { id } }\"}" | jq '.data.getAll | length'
done
```

Expected: counts roughly match the spec's targets. No GraphQL errors.

- [ ] **Step 7: Commit**

```bash
cd /tank/repos/tailoredshapes/manifold && git add data/meridian_fixture.json data/seed_meridian.py
git commit -m "$(cat <<'EOF'
data: rebuild Meridian fixture for demo realism

Closes the seed-data gaps surfaced by the platform evaluation:

- WorkOrder: ~20 records distributed across all 5 statuses (no more Done: 0)
- TestRun: ~100 records over the past 8 weeks with realistic status mix
- TestSuite + DataSource + DataSync: populated so Yard's tabs come alive
- DeploymentPlan: pre-computed for the 5 ChangeRequests
- GanttOutput: rendered for each plan
- Org tree: expanded from 1 visible node to ~15-20 with division/domain/product/team depth
- Bylaws: re-attached at appropriate org levels so cascade is visible
- Person availability: diversified (~60% available, 25% stretched, 15% pto)

One ChangeRequest is wired as the narrative-coherence anchor:
- Visible target deployables in Groundwork
- Owning team in Union with related work orders
- Computed plan and Gantt in Cityhall
- Recent test runs against target deployables in Yard
EOF
)"
git push origin main
```

### Task 3: Validate fixture loads cleanly + cucumber green

- [ ] **Step 1: Run full workspace cucumber**

```bash
cd /tank/repos/tailoredshapes/manifold && cargo test --workspace 2>&1 | tail -25
```

Expected: all scenarios pass (the 8 pre-existing MCP-tool skips in groundwork are unrelated and acceptable). If a scenario fails because it referenced a now-changed fixture entity, investigate and fix the test (probably in `<app>/tests/features/` or `<app>/tests/common/`). The scenario shouldn't depend on specific counts; if it does, that's a brittle test pre-existing.

- [ ] **Step 2: Smoke-curl federation still works**

```bash
curl -sS -X POST http://localhost:3050/deployable/graph -d '{"query":"{ getAll { id name team { id name } } }"}' | jq '.data.getAll[0]'
```

Expected: deployable with team hydrated.

- [ ] **Step 3: Manual browser smoke check (human-only — surface to user)**

Surface to the user that the rebuilt fixture is live; they should walk through each app's tabs in the browser and confirm no dead-screen messages appear. Tabs to check specifically: Cityhall Plans, Cityhall Org (now-expanded tree), Yard Runs, Yard Sync, Union Work (Done column should have items).

No commit for this task — verification only.

### Task 4: Wire the narrative-coherence trail (commit if any fixture tweaks needed)

After Task 2 + 3, walk the chosen headline ChangeRequest manually through all four apps. Verify the trail is coherent. If gaps appear (e.g., the chosen target deployable's owning team has no related work orders), make targeted fixture edits and commit:

```bash
git commit -m "data(fixture): tighten narrative trail for headline ChangeRequest <name>"
git push origin main
```

If no edits are needed: no commit; just confirm.

---

## Phase B: Cross-entity linking (Tasks 5–8)

Phase B adds clickable cross-app navigation. One commit per app. Vanilla JS only — no shared helper introduced. Each app builds its own anchor tag using its target's `*_PUBLIC_URL` env value (already passed through `docker-compose.yml` for groundwork→union, etc.; we may need to add a few more PUBLIC_URLs in docker-compose where they're missing).

### Task 5: Cross-app PUBLIC_URL audit + docker-compose patches

**Files:**
- Read: `docker-compose.yml`
- Modify: `docker-compose.yml` (add any missing `*_PUBLIC_URL` env vars)
- Modify: each affected `<app>/src/main.rs` (read the new env vars, expose via a `/config.json` endpoint that the frontend can fetch)

Wait — this risks introducing a `/config.json` endpoint that resembles the previous-session BFF pattern. Let it not: the `/config.json` carries only static deployment-time URLs (no entity data), and is one tiny GET. That's config, not BFF. (See `feedback_use_the_graph.md` — the rule rules out cross-service data via proxy; it explicitly permits "a `/config.json` endpoint that publishes another app's public URL for cross-app linking ... that's not BFF, that's just config.")

- [ ] **Step 1: Audit current PUBLIC_URL env vars**

```bash
grep -n 'PUBLIC_URL' /tank/repos/tailoredshapes/manifold/docker-compose.yml
```

Expected: at least `UNION_PUBLIC_URL` is wired into groundwork (per memory `feedback_use_the_graph.md` and the previous session's work). Identify which others are missing.

- [ ] **Step 2: Add the missing PUBLIC_URL vars per app**

Each app needs the public URLs of every app it cross-links into. Per the spec:
- groundwork (already has UNION_PUBLIC_URL): no changes needed if its cross-links only target Union
- union: needs GROUNDWORK_PUBLIC_URL (for WorkOrder → Deployable) + CITYHALL_PUBLIC_URL (for WorkOrder → ChangeRequest if linked)
- cityhall: needs GROUNDWORK_PUBLIC_URL (for ChangeRequest target → Deployable, OrgNode → Deployable) + UNION_PUBLIC_URL (for OrgNode.team → Team detail)
- yard: needs GROUNDWORK_PUBLIC_URL (for TestEnvironment → Deployable, TestSuite → Deployable) + UNION_PUBLIC_URL (for TestRun → Team) + CITYHALL_PUBLIC_URL (for TestRun → ChangeRequest)

Add each via `${VAR:-https://<app>.tildarc.com}` defaults in `docker-compose.yml`.

- [ ] **Step 3: Each app exposes a `/config.json` endpoint that publishes the URLs it knows**

Pattern (in each `<app>/src/main.rs`):

```rust
async fn serve_config(State(state): State<AppState>) -> Response {
    let body = serde_json::json!({
        "groundwork_public_url": state.groundwork_public_url,
        "union_public_url":      state.union_public_url,
        "cityhall_public_url":   state.cityhall_public_url,
        "yard_public_url":       state.yard_public_url,
    }).to_string();
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}
```

Only include URLs the app actually needs. Add the route in the existing `Router::new()` block (the same place where `/health` is registered).

Then the frontend's `init()` (or equivalent bootstrap) fetches `/config.json` once and caches the URLs on `state.config`.

- [ ] **Step 4: Smoke verify**

```bash
for port in 3050 3051 3052 3053; do
  echo "--- :$port ---"
  curl -sS "http://localhost:$port/config.json" | jq .
done
```

Each should return JSON with the relevant `*_public_url` keys.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: cross-app PUBLIC_URL config endpoints

Each app now serves /config.json with the public URLs of its cross-link
targets (no entity data — config only, sourced from env vars in
docker-compose.yml). Frontends fetch this once at bootstrap to build
cross-app <a href>s without hardcoding URLs.

Not a BFF pattern: /config.json carries no data, only deployment-time
config strings. See memory feedback_use_the_graph.md."
git push origin main
```

### Tasks 6, 7, 8: Per-app cross-link wiring

Each task follows the same shape; one task per app (skipping groundwork unless it has outbound links beyond Union — likely it has dependency→contract and dependency→SLA which are *intra-app* deeplinks, not cross-app).

Pattern per task:

- [ ] **Step 1: Read the app's `static/app.js` to identify all currently-bare cross-entity ID references in render code**

For example, in `cityhall/static/app.js`, search for places like `${esc(node.team_id)}` or `${esc(cr.requested_by)}` — these are spots where a raw ID is rendered as text.

- [ ] **Step 2: Fetch /config.json on bootstrap if not already**

Add to the app's `init()`:

```javascript
async function loadConfig() {
  try {
    const res = await fetch('/config.json');
    if (res.ok) state.config = await res.json();
  } catch { /* fall back to no-link rendering */ }
}
```

- [ ] **Step 3: Wrap each cross-entity ID in an anchor**

Helper (inline or in a small section near the existing render helpers):

```javascript
function crossLink(appKey, entityKind, id, label) {
  const base = state.config?.[`${appKey}_public_url`];
  if (!base) return label;  // graceful fallback
  return `<a href="${base.replace(/\/$/, '')}#${entityKind}/${encodeURIComponent(id)}">${esc(label)}</a>`;
}
```

Then replace each bare render with a call to this helper. Examples:

- `${esc(cr.requested_by)}` → `${crossLink('union', 'persons', cr.requested_by, cr.requested_by_name || cr.requested_by)}`
- `${esc(node.team_id)}` → `${crossLink('union', 'teams', node.team_id, node.team?.name || node.team_id)}`
- `${esc(wo.deployable_id)}` → `${crossLink('groundwork', 'deployables', wo.deployable_id, wo.deployable?.name || wo.deployable_id)}`

For intra-app deeplinks (Groundwork's dependency → contract/SLA), don't use `crossLink`; just use `<a href="#<entity>/<id>">` directly.

- [ ] **Step 4: node --check + smoke-curl + rebuild + manual verify a few**

- [ ] **Step 5: Commit per app**

```bash
git commit -m "feat(<app>): cross-app deeplinks on bare entity IDs

Bare cross-entity ID references in render code now resolve to
<a href> elements pointing at the corresponding app's hash-routed URL.
Graceful fallback to plain text if /config.json is unreachable."
git push origin main
```

**Per-app cross-link inventory:**

- **Task 6 — groundwork (`groundwork/static/app.js`):**
  - dependency row → its service detail (intra-app, `#services/<id>`)
  - dependency row → its deployable detail (intra-app)
  - exposes row → its deployable + service details
  - deployable row → its team in Union (cross-app via `union_public_url`)
  - Note: dependency → contract is not a foreign key on Dependency; it's a transitive relationship (Dependency.service_id → Service ← Contract.service_id). If you want a direct dep→contract link, surface it in the dependency detail panel by looking up contracts where `service_id` matches.

- **Task 7 — union (`union/static/app.js`):**
  - WorkOrder card → its deployable in Groundwork (verify whether already wired; if so, skip)
  - WorkOrder card → its change_request in Cityhall (if `change_request_id` is set — fixture's headline ChangeRequest WorkOrders should be wired)
  - TeamMember row → Person detail (intra-app)

- **Task 8 — cityhall + yard (one task each, but bundled here for brevity):**

  cityhall: ChangeRequest.target_deployables → Deployable list links; ChangeRequest.requested_by → Person in Union (when stored as id); OrgNode.team_id → Team in Union (when leaf); DeploymentPlan step → target Deployable.

  yard: TestEnvironment.deployable_id → Deployable; TestRun.change_request_id → ChangeRequest in Cityhall; TestRun.team_id → Team in Union; TestSuite.deployable_id → Deployable.

Commit each app separately.

---

## Phase C: Feature work (Tasks 9–18)

### Task 9 (C1.1): Add `deployment_status` field to Deployable schema

**Files:**
- Modify: `groundwork/config/schema/deployable.json` — add `deployment_status` to the JSON Schema definition; allow null; default `"unknown"`; enum values `["operational", "degraded", "down", "unknown"]`
- Modify: `groundwork/config/graph/deployable.graphql` — add `deployment_status: String` to the `Deployable` type

- [ ] **Step 1: Read both files**

```bash
cat groundwork/config/schema/deployable.json
cat groundwork/config/graph/deployable.graphql
```

- [ ] **Step 2: Add to JSON Schema**

```json
"deployment_status": {
  "type": ["string", "null"],
  "enum": ["operational", "degraded", "down", "unknown", null],
  "default": "unknown"
}
```

- [ ] **Step 3: Add to GraphQL projection**

After the existing fields in the `type Deployable { ... }` block:

```graphql
deployment_status: String
```

- [ ] **Step 4: Rebuild + verify**

```bash
cd /tank/repos/tailoredshapes/manifold && docker compose up -d --build groundwork
sleep 5
curl -sS -X POST http://localhost:3050/deployable/graph \
  -d '{"query":"{ getAll { id name deployment_status } }"}' | jq '.data.getAll[0]'
```

Expected: response includes `deployment_status` (will be `null` until Task 10 seeds it).

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(groundwork): add deployment_status field to Deployable

New scalar (operational | degraded | down | unknown) on Deployable.
JSON Schema validates; GraphQL projects. Fixture seed + frontend
rendering follow in next commits."
git push origin main
```

### Task 10 (C1.2): Seed `deployment_status` in the fixture

**Files:**
- Modify: `data/meridian_fixture.json`

Add a `deployment_status` value to every Deployable. Distribution: ~22 operational, ~5 degraded, ~2 down, ~1 unknown (out of 30). Pick the down/degraded ones plausibly (e.g., Legacy CRM down; one analytics service degraded).

- [ ] **Step 1: Edit fixture, then seed**

```bash
./scripts/reset-data.sh
```

- [ ] **Step 2: Verify**

```bash
curl -sS -X POST http://localhost:3050/deployable/graph \
  -d '{"query":"{ getAll { name deployment_status } }"}' \
  | jq '[.data.getAll[] | .deployment_status] | group_by(.) | map({status: .[0], count: length})'
```

Expected: counts roughly match the distribution above.

- [ ] **Step 3: Commit**

```bash
git commit -m "data(fixture): seed deployment_status on all Deployables"
git push origin main
```

### Task 11 (C1.3): Render deployment_status as a badge column in the Deployables list

**Files:**
- Modify: `groundwork/static/app.js` — `ENTITIES.deployables` config (extend `getRowBadge` or add a parallel column), plus update the graph query to include `deployment_status`
- Modify: `groundwork/static/index.html` — possibly add CSS variables for status colors if not already there

- [ ] **Step 1: Update the graph query in `ENTITIES.deployables.graph.list`**

Add `deployment_status` to the existing query string.

- [ ] **Step 2: Render the badge**

There's already a `getRowBadge` for deployables (showing team name from the post-fix federation work — see commit `e93dce9` and follow-up `f8a69b2`). The team-name badge stays; we add a status badge in parallel. Two options:
  - Compose into the existing badge: `${team.name || ''} · ${status.label}` — simple, no DOM change
  - Add a parallel badge slot: extend `buildRow` to support two badges. Slight DOM change.

Pick the simpler composition for v1; advanced layout (color swatches) can come later.

Status colors via inline CSS classes:
- `operational` → green dot or text
- `degraded` → amber
- `down` → red
- `unknown` → grey

- [ ] **Step 3: Filter dropdown in the deployables list header**

Add a `<select>` with options `All | operational | degraded | down | unknown`. On change, store the filter in `state.filter.deployment_status` and apply in `renderList()` (which currently filters by the search box; extend it to also filter by status).

- [ ] **Step 4: node --check + rebuild + curl + manual smoke check**

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(groundwork): show deployment_status as colored badge + filter

Each Deployable row now shows its deployment_status (operational/
degraded/down/unknown) with a colored indicator. A filter dropdown in
the list header narrows by status. Read via the existing /graph query.
No write integration yet — status is seeded only."
git push origin main
```

### Task 12 (C2.1): Add `story_points` field to WorkOrder schema

Mirror Task 9's shape, but for Union and WorkOrder.

**Files:**
- Modify: `union/config/schema/work_order.json` — add `story_points: { "type": ["integer", "null"], "minimum": 0 }`
- Modify: `union/config/graph/work_order.graphql` — add `story_points: Int`

Commit: `feat(union): add story_points field to WorkOrder`. Verify via curl and commit.

### Task 13 (C2.2): Seed `story_points` on existing WorkOrders

**Files:**
- Modify: `data/meridian_fixture.json` — add `story_points` to every WorkOrder. Use the planning-poker Fibonacci sequence: 1, 2, 3, 5, 8 (some larger 13 if needed).

Reset, verify counts, commit: `data(fixture): seed story_points on all WorkOrders`.

### Task 14 (C2.3): Render story_points in Union UI

**Files:**
- Modify: `union/static/app.js` — update the work_order graph query to select `story_points`; add a per-team rollup in the Teams tab card; add a per-status rollup in the Work-board kanban column headers; add the field to the WorkOrder create/edit modal.
- Modify: `union/static/index.html` — modal form: add an `<input type="number" min="0">` for `story_points` (or a numeric select with the 1/2/3/5/8 ladder, your choice)

- [ ] **Step 1: Add to graph query**

In `loadAll()`, update the work_order query:

```javascript
'{ getAll { id team_id summary deployable_id deployable { id name } change_request_id status priority story_points } }'
```

- [ ] **Step 2: Per-team rollup in Teams card**

In whatever function renders team cards (search for `renderTeams` or similar), compute `team.openPoints = workOrders.filter(wo => wo.team_id === team.id && wo.status !== 'done').reduce((s, wo) => s + (wo.story_points || 0), 0)`. Render as `{team.openPoints} pts in flight`.

- [ ] **Step 3: Per-status rollup in kanban**

In the kanban header for each status column, append `· ${columnPoints} pts`.

- [ ] **Step 4: Modal form field**

In the WorkOrder modal, add the input. Confirm it's submitted via the REST helper (it should be, since the JSON Schema now accepts the field).

- [ ] **Step 5: node --check + rebuild + verify + commit**

```bash
git commit -m "feat(union): show story_points + per-team and per-status rollups

WorkOrder cards show their estimate; Teams tab card surfaces a total
points-in-flight per team; Work board kanban columns show points per
status. Modal includes a story_points input on create/edit."
git push origin main
```

### Task 15 (C3): Staffing dots polish

**Files:**
- Modify: `union/static/app.js` — the `renderStaffing` function (or equivalent for the staffing matrix)
- Modify: `union/static/index.html` — add CSS for larger dots and week labels (or modify the existing CSS block in the file's `<style>` tag)

Changes:
- Dot size from current (~6px) to ~12px
- Week-label header row across the matrix (column headers showing the week of the year or the week's start date)
- Hover tooltip shows the full work-order summary (currently abbreviated)
- Click a dot deeplinks to the work-order detail (use the same hash-route convention)
- Ensure keyboard navigation works: each dot is a `<button>` (or has `role="button" tabindex="0"`); Enter/Space activate it

- [ ] **Step 1: Read renderStaffing's current shape**

```bash
grep -n 'function renderStaffing\|"staffing"\|class=".*staffing' /tank/repos/tailoredshapes/manifold/union/static/app.js | head
```

- [ ] **Step 2: Make the changes**

- [ ] **Step 3: node --check + rebuild + smoke verify**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(union): staffing matrix legibility pass

- Dot size up from ~6px to ~12px so cells are scannable without hover
- Week-label header row above the matrix
- Hover tooltip shows full work-order summary
- Click dot deeplinks to work-order detail
- Keyboard-accessible: each dot is a button with Enter/Space activation"
git push origin main
```

### Task 16 (C4.1): Vendor Cytoscape.js + add Graph tab DOM

**Files:**
- Create: `groundwork/static/vendor/cytoscape.min.js` (vendor the latest stable from https://js.cytoscape.org/ — verify the license is permissive — MIT — before vendoring)
- Modify: `groundwork/src/main.rs` — add a new `serve_cytoscape_js()` handler + route serving the file with `application/javascript; charset=utf-8` and the existing `Cache-Control: no-cache, must-revalidate`. **Use `include_str!`** for consistency with the existing static-serving pattern (no new filesystem-read code).
- Modify: `groundwork/static/index.html` — add a new tab to the existing sidebar (or top tab list — match the current layout) labeled "Graph". The tab content area gets a div for the Cytoscape container and a div for the side detail panel. Reserve a header bar slot for the filter controls. Add a `<script src="/static/vendor/cytoscape.min.js"></script>` (or load via dynamic `import` if the file's module shape supports it; verify before deciding).
- Modify: `groundwork/static/app.js` — extend the sidebar/tab navigation to recognize the new "Graph" entity; on activation, call a new `renderGraph()` function (stubbed in this task — full implementation in Task 17).

- [ ] **Step 1: Download + vendor Cytoscape**

```bash
mkdir -p /tank/repos/tailoredshapes/manifold/groundwork/static/vendor
curl -sSL https://cdn.jsdelivr.net/npm/cytoscape@3.30.2/dist/cytoscape.min.js \
  -o /tank/repos/tailoredshapes/manifold/groundwork/static/vendor/cytoscape.min.js
ls -lh /tank/repos/tailoredshapes/manifold/groundwork/static/vendor/cytoscape.min.js
```

Confirm filesize is reasonable (~300KB). If the implementer prefers a different CDN URL or a different version, use it; just pin a specific version.

- [ ] **Step 2: Add serve handler in `groundwork/src/main.rs`**

Pattern (alongside `serve_app_js`):

```rust
const CYTOSCAPE_JS: &str = include_str!("../static/vendor/cytoscape.min.js");

async fn serve_cytoscape_js() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        CYTOSCAPE_JS,
    )
        .into_response()
}
```

The `immutable` cache-control is appropriate because the file is version-pinned in the codebase (immutable filename would be ideal — `cytoscape.3.30.2.min.js` — but optional).

Add the route: `.route("/static/vendor/cytoscape.min.js", get(serve_cytoscape_js))` in the same router block where `/static/app.js` is registered.

- [ ] **Step 3: Add Graph tab to index.html**

Match the existing sidebar pattern; add a new nav item with `data-entity="graph"` (or whatever the existing naming convention is — read the HTML first).

Add a container div for the graph and a side panel:

```html
<div id="screen-graph" class="screen">
  <div class="graph-toolbar">
    <!-- filter controls go here in Task 17 -->
  </div>
  <div id="cy" style="width: 100%; height: 80vh;"></div>
  <aside id="graph-detail" class="graph-detail" hidden>
    <!-- focused node details -->
  </aside>
</div>
```

Add the `<script>` tag for cytoscape **before** `app.js`:

```html
<script src="/static/vendor/cytoscape.min.js"></script>
<script src="/static/app.js"></script>
```

- [ ] **Step 4: Stub renderGraph + wire the tab**

In `groundwork/static/app.js`, add:

```javascript
function renderGraph() {
  document.getElementById('graph-detail').hidden = true;
  // Task 17 fills this in.
  document.getElementById('cy').textContent = 'graph rendering coming in next commit';
}
```

In `setActiveEntity` (or the equivalent screen-switch function), handle the new `'graph'` key: instead of `renderList()`, call `renderGraph()`.

- [ ] **Step 5: Rebuild + verify**

```bash
docker compose up -d --build groundwork
curl -sSI http://localhost:3050/static/vendor/cytoscape.min.js | head -3
curl -sS http://localhost:3050/static/vendor/cytoscape.min.js | head -c 100
```

Expected: 200 status, JavaScript content type, JS source visible.

- [ ] **Step 6: Manual smoke (browser)**

In browser console at `https://groundwork.tildarc.com`: confirm `typeof cytoscape === 'function'`. Click the new Graph tab; the stub message appears.

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(groundwork): vendor Cytoscape.js + scaffold Graph tab

Vendored cytoscape 3.30.2 (~300KB, MIT licensed) into static/vendor/,
served via /static/vendor/cytoscape.min.js with long-cache headers.
A new 'Graph' tab in the sidebar renders a stub container; the actual
visualization lands in the next commit."
git push origin main
```

### Task 17 (C4.2): Render the dependency graph

**Files:**
- Modify: `groundwork/static/app.js` — flesh out `renderGraph()` with data fetch + cytoscape init + layout + interactions + filter UI

- [ ] **Step 1: Data fetch**

`renderGraph()` calls two graph queries:

```javascript
const [depResp, nodeResp] = await Promise.all([
  gqlQuery('/dependency/graph', '{ getAll { id deployable_id service_id criticality protocol auth_method } }'),
  gqlQuery('/deployable/graph', '{ getAll { id name team_id team { id name } deployment_status } }'),
]);
```

Plus we need services to make edges meaningful (a Dependency's `service_id` points at a Service, which is `exposed` by a Deployable through the `Exposes` entity — so the deployable→deployable edge is really `Dep.deployable_id` → uses `Exposes.deployable_id` that exposes `Dep.service_id`). For v1 simplicity, fetch exposes too:

```javascript
const exposeResp = await gqlQuery('/exposes/graph', '{ getAll { id deployable_id service_id } }');
```

Build a `service → exposing_deployable` map from exposes, then resolve each Dependency to a `consumer_deployable → producer_deployable` edge.

- [ ] **Step 2: Cytoscape init**

```javascript
const cy = cytoscape({
  container: document.getElementById('cy'),
  elements: [
    ...deployables.map(d => ({
      data: { id: d.id, label: d.name, status: d.deployment_status || 'unknown', team: d.team?.name },
    })),
    ...resolvedEdges.map(e => ({
      data: { id: e.id, source: e.consumer, target: e.producer, criticality: e.criticality },
    })),
  ],
  style: [
    { selector: 'node', style: {
        'label': 'data(label)',
        'background-color': statusColor,  // function mapping status → color
        'text-valign': 'center', 'text-halign': 'right', 'text-margin-x': 4,
        'font-size': 10, 'width': 20, 'height': 20,
    }},
    { selector: 'edge', style: {
        'width': crit => ({ low: 1, medium: 2, high: 4 }[crit] || 1),
        'line-color': crit => ({ low: '#ccc', medium: '#888', high: '#c33' }[crit] || '#aaa'),
        'curve-style': 'bezier',
        'target-arrow-shape': 'triangle',
    }},
    { selector: '.faded', style: { 'opacity': 0.15 } },
    { selector: ':selected', style: { 'border-width': 3, 'border-color': '#000' } },
  ],
  layout: { name: 'cose', animate: true, idealEdgeLength: 100, nodeOverlap: 20 },
});
```

(The `statusColor` and edge-style functions need to be plain JS, not Cytoscape-mapper syntax — adjust accordingly. Cytoscape style mappers use a specific syntax — see Cytoscape docs or use the `style: cytoscape.stylesheet().selector(...).style(...)` builder.)

- [ ] **Step 3: Click-to-focus interaction**

```javascript
cy.on('tap', 'node', evt => {
  const node = evt.target;
  cy.elements().addClass('faded');
  node.removeClass('faded').neighborhood().removeClass('faded');
  showGraphDetail(node.data());
});
cy.on('tap', evt => {
  if (evt.target === cy) {
    cy.elements().removeClass('faded');
    document.getElementById('graph-detail').hidden = true;
  }
});
```

`showGraphDetail(data)` renders the side panel with the focused deployable's name, status, team, related contracts (look up via the already-fetched contracts/exposes/services), and related SLAs.

- [ ] **Step 4: Filter UI**

In the toolbar div:

```html
<label><input type="checkbox" data-filter-crit="high" checked> high</label>
<label><input type="checkbox" data-filter-crit="medium" checked> medium</label>
<label><input type="checkbox" data-filter-crit="low" checked> low</label>
<select id="filter-team"><option value="">All teams</option><!-- populated from data --></select>
<button id="reset-graph">Reset</button>
```

Hook up change handlers that hide/show edges by criticality and nodes by team.

- [ ] **Step 5: Table-view fallback for keyboard users**

Add a "View as table" toggle button that hides the canvas and shows a sortable HTML table of the same data — accessibility fallback. Implementation: a `<table>` with columns deployable, status, team, depends_on (count), depended_on_by (count). Reuse `state.data.deployables` for rows.

- [ ] **Step 6: node --check + rebuild + manual smoke check**

Reload the Graph tab; confirm 30 nodes + ~86 edges render; node colors reflect status; edge styles reflect criticality; click-to-focus works; filters work; table fallback works.

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(groundwork): interactive dependency graph

New Graph tab renders all Deployables and their inter-service
dependency relationships using Cytoscape's cose layout. Nodes are
colored by deployment_status; edges are styled by criticality.
Click a node to focus its neighborhood and see contracts/SLAs in
the side panel. Filter controls hide/show by criticality and team.
A 'View as table' toggle provides a keyboard-accessible fallback.

The graph composes locally in the browser from three /graph queries
(deployables, dependencies, exposes) — no new backend endpoint."
git push origin main
```

### Task 18 (C4.3): Cucumber feature for graph data shape

**Files:**
- Create: `groundwork/tests/features/graph_viz_data.feature` — verify the three queries the graph composes from return shape-compatible responses
- Possibly: `groundwork/tests/common/` — helpers if not already present

Sample scenario:

```gherkin
Feature: Graph visualization data shape

  Scenario: getAll deployables exposes deployment_status field
    Given the groundwork app is running
    When I POST to /deployable/graph with query "{ getAll { id deployment_status } }"
    Then the response has no GraphQL errors
    And each deployable's deployment_status is one of operational, degraded, down, unknown, or null

  Scenario: dependency + exposes data can be composed into edges
    Given the groundwork app is running
    When I POST to /dependency/graph with query "{ getAll { id deployable_id service_id criticality } }"
    And I POST to /exposes/graph with query "{ getAll { id deployable_id service_id } }"
    Then every Dependency.service_id has at least one Exposes record
    And every Dependency.deployable_id resolves to a Deployable
```

- [ ] Run: `cargo test -p groundwork --test groundwork_cert`
- [ ] Commit: `test(groundwork): cucumber scenarios for graph viz data shape`

---

## Final cross-cutting verification (Task 19)

- [ ] **Step 1: `cargo test --workspace`** must pass cleanly (same 8 pre-existing MCP skips OK)
- [ ] **Step 2: Per-app screen tour** (human via SSO-gated URLs):
  - groundwork: deployables list shows status badges + filter; graph tab loads
  - union: work board shows Done column populated; staffing dots are scannable; team cards show points-in-flight; modal has story_points field
  - cityhall: org tree expands to multi-level; plans tab shows generated plans; bylaws cascade visible
  - yard: runs tab has history; sync chart has data; environments link to their served deployable
- [ ] **Step 3: Cross-app traversal smoke**: click a target_deployable in a Cityhall ChangeRequest → lands in Groundwork on that Deployable's detail; click that Deployable's team → lands in Union on that Team; click a recent test run → lands in Yard on that run.
- [ ] **Step 4: Narrative trail end-to-end**: follow the headline ChangeRequest through all four apps without dead ends.

No commit for this task — verification only. If issues are found, they may warrant follow-up commits or follow-up plans.

---

## Out of scope (for this plan)

- **Auth integration** — next initiative after this lands.
- **Sprint / Cycle entity** — story points only.
- **Real Kubernetes / Argo / cloud integration** for deployment_status.
- **D3-based graph viz** — Cytoscape is the chosen library.
- **pushState routing migration** — hash routing remains.
- **Cross-app shape reconciliation** (the four `loadAll()` styles flagged in the previous review).
- **MeshQL-RS repo changes** — all work stays in `manifold/`.
- **New frontend test framework** — manual + cucumber as today.
