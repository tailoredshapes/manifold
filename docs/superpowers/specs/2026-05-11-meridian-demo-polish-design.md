# Meridian Demo Polish — Design

## Context

A platform-evaluation pass on the four Manifold apps surfaced a coherent set of critiques, summarized in one sentence: *the most interesting features are either empty or barely populated, so viewers are asked to take the concept on faith.* The specific weaknesses called out — empty Cityhall Plans tab, empty Yard Runs/Sync tabs, "Done: 0" in Union, shallow org tree, every-person-stretched, no Groundwork dependency graph, no deployment status, no Union capacity primitive — split surprisingly cleanly into three buckets. Most of the gap by volume is *seed data*, not code.

This spec covers a three-phase initiative to close the demo gap before the auth integration kicks off. Each phase is independently shippable and lands directly on `main` per Trunk-Based Development.

## Decisions

Made during the brainstorm with explicit user input:

- **Run all three buckets before auth.** Auth was the planned next-step; demo polish takes priority based on the evaluation findings.
- **Story points only — no Sprint entity.** WorkOrder gains a `story_points: number` field plus a per-team rollup. No sprint/cycle concept; that's a methodology commitment we're not making.
- **Dependency graph is interactive and force-directed.** Cytoscape.js (preferred over D3 for this use case — higher-level graph API, less hand-written layout code) vendored or via CDN. Slight breach of the project's vanilla-only rule; explicitly accepted.
- **Deployment status is a simple seeded field**, not a live infrastructure integration. `deployment_status: operational | degraded | down | unknown` on Deployable, set in the fixture, rendered as a colored badge. Real Kubernetes/etc. integration is a separate effort.
- **Cross-app deeplinks use the `#<entity>/<id>` hash convention** already established by the REST → /graph migration. Each app's public URL (per `*_PUBLIC_URL` env vars / `tildarc.com` mapping) is used for cross-app `<a href>`s.

## Phase A — Seed-data pass (1–2 days)

A focused rebuild of `data/meridian_fixture.json` and minor tweaks to `data/load_fixture.py` / `data/seed_meridian.py` if the new shapes require them. No app code changes.

Resolves:

- **Union Work board "Done: 0" gap.** Distribute the existing ~7 WorkOrders plus new additions so each status column has plausible occupancy. Target distribution: 4 proposed, 6 in-progress, 1 blocked, 8 done.
- **Yard Runs + Sync tabs are dead screens.** Generate ~100 TestRun records across the 23 environments over the past 8 weeks. Vary status (passed/failed/cancelled/errored), duration_minutes, cost_actual; populate the chart-needs-≥2-runs gate. Tie ~20% of runs to the existing change requests so the change-request → test-run linkage is visible.
- **Cityhall Plans tab is empty.** Pre-compute DeploymentPlans for the 5 existing ChangeRequests using the existing planning logic in `cityhall/src/plan.rs`. Capture the computed plans into the fixture so they survive a reset.
- **Cityhall Org tree is shallow.** Below "Meridian Freight Solutions" (the only currently-rendered root), add a realistic 4-tier structure: divisions (e.g., "Customer Operations", "Driver Operations", "Platform"), domains under each, products under each domain, and leaf teams pointing at Union.Team. Total ~15–20 nodes so the bylaw cascade has visible depth.
- **Union People "every-person-stretched" gap.** Diversify the availability / stretched-badge field across the 17 people: roughly 60% healthy, 25% stretched, 15% unavailable/PTO. (If "stretched" is currently a derived signal rather than a stored field, this becomes a one-line render fix instead — verify during execution.)
- **Narrative coherence.** Choose one of the 5 existing ChangeRequests as the *headline workflow*. Ensure the threading is coherent across all four apps: visible target deployables in Groundwork; an owning team in Union with related work orders in the work board; a computed deployment plan with a Gantt in Cityhall; recent test runs against the target deployable's environments in Yard. A demo viewer should be able to follow this one change request end-to-end.

**Acceptance:** Reset the stack with the new fixture, click every tab in every app, confirm no "empty state" message appears on a screen the critique flagged. The narrative ChangeRequest can be traced through all four apps without dead ends.

## Phase B — Cross-entity linking (2–3 days)

Small per-app frontend changes that turn currently-bare ID references and labels into clickable cross-entity navigation. No new entities. No backend changes. No new shared infrastructure beyond the existing `*_PUBLIC_URL` env conventions.

Specifically:

- **Groundwork**: dependency row → contract detail; dependency row → SLA detail; service row → its exposing deployable(s).
- **Cityhall**: change-request target_deployable → Groundwork Deployable page; org_node.team_id → Union Team page; deployment-plan step → its target Deployable.
- **Yard**: TestEnvironment → its served Deployable; TestRun → its triggering ChangeRequest (Cityhall) + target Deployable + responsible Team (Union).
- **Union**: WorkOrder card → Groundwork Deployable page (verify whether already wired); WorkOrder card → Cityhall ChangeRequest if linked; TeamMember row → Person detail.

**Pattern:** each clickable cross-entity label becomes an `<a href={publicUrl}#<entity>/<id>}>` inheriting the existing hash-routing convention. Public URLs come from each app's existing `*_PUBLIC_URL` env (already wired in `docker-compose.yml`). No new abstractions; no new shared frontend helper.

**Acceptance:** Click every cross-entity ID/label in every app — each one navigates to the corresponding target. Manual smoke check covers all four apps.

## Phase C — Feature work (~1.5–2 weeks)

In order, smallest to largest so the foundation lands before the headline:

### C1 — Deployment status on Deployable (Groundwork, ~1–2 days)

New scalar field on Deployable:

```
deployment_status: "operational" | "degraded" | "down" | "unknown"
```

Optionally paired with `tier_status` later (per-environment status); v1 is one-status-per-deployable to keep the data model simple.

Schema change: `groundwork/config/schema/deployable.json` (add field, allow null, default `"unknown"`). GraphQL projection: add `deployment_status: String` to `groundwork/config/graph/deployable.graphql`. Frontend: a colored badge column in the deployables list; status filter dropdown in the header. Seed: each of the 30 deployables gets a plausible status in the fixture (most `operational`, a few `degraded`, one or two `down`).

**No backend integration.** This is a *displayed* field set by the operator or seed data, not a *computed* field reflecting actual infrastructure state. Real-source integration (Kubernetes, Argo, etc.) is explicitly a follow-up effort.

### C2 — Story points on WorkOrder (Union, ~1 day)

New scalar field on WorkOrder:

```
story_points: number  // small integer, optional
```

Schema change: `union/config/schema/work_order.json`. GraphQL: add to `union/config/graph/work_order.graphql`. Frontend:

- Per-team total points in the Teams tab card (alongside existing capacity gauge).
- Per-status total points in the Work board kanban column headers ("To Do · 12 pts").
- Editable field in the WorkOrder create/edit modal.

Seed: each of the work orders gets a plausible point estimate (1, 2, 3, 5, 8 scale).

No new entity, no Sprint concept, no velocity tracking. Just the primitive.

### C3 — Staffing dots polish (Union, ~1 day)

UI tweak only — no schema change.

- Increase dot size from current (~6px) to ~12px.
- Add a week-label header row across the staffing matrix.
- Hover tooltip shows the full work-order summary (currently shows abbreviated form).
- Click a dot deeplinks to the relevant work-order detail.
- Keyboard-navigable for accessibility (per the project's accessibility ground rule).

### C4 — Dependency graph (Groundwork, ~5–7 days)

The headline of Phase C. A new "Graph" tab alongside the existing entity list views.

**Library:** Cytoscape.js, vendored as a single file in `groundwork/static/vendor/cytoscape.min.js` (or pulled from CDN — decision deferred to implementation). One new asset; no build step; no package manager.

**Data:** Two GraphQL queries to populate the visualization on tab open — `getAll` on deployables (already migrated), `getAll` on dependencies. The graph composes locally; no new backend endpoint.

**Visual shape:**
- Nodes are Deployables, colored by `deployment_status` (Phase C1 must land first), labeled with name.
- Edges are Dependency relationships, styled by `criticality` (low = thin/light, medium = medium, high = thick/dark or red).
- Layout: Cytoscape's `cose` (compound spring embedder) layout by default.
- Interactions: drag-to-rearrange; pan/zoom; click a node to focus its neighborhood (fade non-adjacent); right-side detail panel showing the focused node's contracts, SLAs, exposed services, and team owner.
- Filters: a header bar with checkboxes for criticality levels and a team dropdown. Filtering hides edges/nodes matching the criteria.

**Accessibility:** the graph view is a non-essential visualization. The existing list views remain the canonical interface. Keyboard navigation through the graph is not in scope for v1; document the limitation and offer a "view as table" toggle that goes back to the existing list.

**Out of scope for v1:** path-tracing between two specified deployables; impact-radius highlighting (transitive dependencies); persisted layout; export to PNG/SVG.

## Non-goals (explicit)

- **Auth integration.** Deferred to its own initiative immediately following Phase C.
- **Sprint / Cycle entity.** Story points only; methodology-agnostic.
- **Real deployment-status integration** with Kubernetes / Argo / cloud providers. v1 is fixture-seeded only.
- **D3-based graph viz.** Cytoscape is the chosen library.
- **pushState routing migration.** Hash routing remains.
- **Multi-tenant changes.** Instance-per-client model is unchanged.
- **Refactoring of the existing four `loadAll()` shapes** to a single canonical pattern. That's a cleanup effort orthogonal to this one.
- **MeshQL `meshql-rs` repo changes.** All work stays inside `/tank/repos/tailoredshapes/manifold/`.
- **New automated test infrastructure.** Existing cucumber suites continue to gate; new manual verification per phase. Frontend test framework not introduced.

## Verification approach

Per the project's testing posture (cucumber for backend, manual for frontend):

- **Phase A**: Reset stack; visually click every screen in every app; confirm no empty-state messages on screens the critique flagged. Trace the narrative ChangeRequest end-to-end through all four apps.
- **Phase B**: Click every cross-entity ID/label in every app; verify each navigates to the right target.
- **Phase C1**: cucumber feature for the new `deployment_status` field on `/deployable/api` and `/deployable/graph`. Manual check of the badge column and filter dropdown.
- **Phase C2**: cucumber feature for the new `story_points` field. Manual check of the rollups in Teams card and kanban headers.
- **Phase C3**: pure manual — visual diff against the current staffing UI.
- **Phase C4**: cucumber not applicable (visualization). Manual: open the Graph tab; verify 30 nodes and ~86 edges render; verify status colors and criticality styles; verify filter and focus interactions; verify the table-view fallback works for keyboard users.

`cargo test --workspace` must remain green throughout. Each entity-level schema change (C1, C2) requires the existing JSON schema validation and the GraphQL projection to be consistent — verify both points in each migration.

## Critical files

| Phase | Files |
|-------|-------|
| A | `data/meridian_fixture.json` (rewrite), `data/seed_meridian.py` (possible tweaks), `data/fix_meridian_fixture.py` (extend or replace) |
| B | All four `*/static/app.js` (small per-app edits); no backend |
| C1 | `groundwork/config/schema/deployable.json`, `groundwork/config/graph/deployable.graphql`, `groundwork/static/app.js`, `groundwork/static/index.html` (filter UI); fixture updates |
| C2 | `union/config/schema/work_order.json`, `union/config/graph/work_order.graphql`, `union/static/app.js`, `union/static/index.html` (modal form, kanban headers); fixture updates |
| C3 | `union/static/app.js`, `union/static/index.html`, `union/static/style.css` (or wherever the staffing CSS lives) |
| C4 | New: `groundwork/static/vendor/cytoscape.min.js` (or CDN reference in `index.html`); `groundwork/static/app.js` (new graph tab + view logic); `groundwork/static/index.html` (new tab DOM); `groundwork/src/main.rs` (serve the vendor file if locally hosted) |

## Open questions to resolve during implementation

These don't block the spec but require decisions during the per-phase plans:

1. **Cytoscape: CDN or vendored?** Vendored is more aligned with self-contained deployment; CDN is simpler. Decide before C4.
2. **Headline ChangeRequest choice.** Pick during Phase A — should be one that has plausible deployable targets and a believable team owner.
3. **Stretched/availability data shape.** Verify if Union's "stretched" is derived or stored; choose accordingly during Phase A.
4. **DeploymentPlan computation as seed.** The `cityhall/src/plan.rs` computation is normally runtime; ensure pre-computing for the fixture doesn't bake stale data. Plans may need to be regenerated on first server start rather than literal-stored in the JSON.
