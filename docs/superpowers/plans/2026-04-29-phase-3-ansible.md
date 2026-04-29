# Phase 3 — Ansible Import / Export

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps tagged **(delegate)** are good Qwen units.

**Pre-requisite:** Phase 1 complete. Phase 2 not strictly required, but the `CatalogDelta`/`apply_delta`/`Repos` infrastructure introduced in Phase 2 is reused here. If Phase 2 is not yet on `main`, **lift the catalog/apply_delta slices forward into Phase 3 instead** — they belong to whichever IaC phase lands first.

**Goal:** Add bidirectional integration between the Groundwork catalogue and Ansible inventories:
- `POST /import/ansible` parses a YAML Ansible inventory (with optional groundwork-tagged host vars) into Deployables + Services + Exposes + Dependencies.
- `GET /export/ansible?team=<team>` renders a YAML inventory.

**Architecture:** A `groundwork::importers::ansible` module mirroring the k8s shape: `parse(&str) -> CatalogDelta`, `emit(&CatalogSnapshot) -> String`. Same `apply_delta` glue. Same HTTP-handler shape.

**Tech Stack:** `serde_yaml` (already pulled in by Phase 2), no other new dependencies.

---

## Why a tag-based contract

Ansible inventories don't have a universal "this host exposes service X" annotation. We **define** one. Users opt in by adding `groundwork_*` host vars (or group vars) to their inventory:

```yaml
all:
  children:
    payments:
      vars:
        groundwork_team: payments-team
      hosts:
        checkout-1.prod.example.com:
          groundwork_deployable: checkout
          groundwork_repo: https://github.com/acme/checkout
          groundwork_description: Checkout flow
          groundwork_exposes:
            - { name: checkout-api, port: 8080, protocol: http }
          groundwork_depends_on:
            - { name: payments-stripe, criticality: high }
```

Without these tags, a host is **ignored** by the importer (it's not a "Deployable" in the Groundwork sense — it's just a server). This is intentional: Ansible inventories often include bastions, monitoring agents, and infra that aren't worth catalogue-ing.

When emitting, we lay the same tags on the inventory we generate.

---

## File map

| File | Operation |
|---|---|
| `groundwork/src/importers/ansible.rs` | Create — mod root |
| `groundwork/src/importers/ansible/types.rs` | Create — minimal inventory types |
| `groundwork/src/importers/ansible/parse.rs` | Create — `parse(&str) -> Result<CatalogDelta>` |
| `groundwork/src/importers/ansible/emit.rs` | Create — `emit(&CatalogSnapshot) -> Result<String>` |
| `groundwork/src/main.rs` | Modify — `/import/ansible` + `/export/ansible` routes |
| `groundwork/tests/features/import_ansible.feature` | Create |
| `groundwork/tests/features/export_ansible.feature` | Create |
| `groundwork/tests/fixtures/ansible/tagged_inventory.yaml` | Create |
| `groundwork/tests/fixtures/ansible/untagged_inventory.yaml` | Create |
| `groundwork/tests/fixtures/ansible/group_vars_inheritance.yaml` | Create |

---

## BDD: import scenarios

### Fixture `tagged_inventory.yaml`

(See "Why a tag-based contract" above for the full example. Save it as the fixture.)

### Fixture `untagged_inventory.yaml`

Same shape but with no `groundwork_*` keys. The importer must silently ignore these hosts; no Deployables created.

### Fixture `group_vars_inheritance.yaml`

```yaml
all:
  children:
    payments:
      vars:
        groundwork_team: payments-team
        groundwork_repo: https://github.com/acme/payments-monorepo
      hosts:
        billing-1.example.com:
          groundwork_deployable: billing
          groundwork_exposes:
            - { name: billing-api, port: 8080, protocol: http }
        invoicing-1.example.com:
          groundwork_deployable: invoicing
          groundwork_exposes:
            - { name: invoicing-api, port: 8080, protocol: http }
```

Both deployables inherit `team: payments-team` and `repo_url: https://github.com/acme/payments-monorepo` from the group's `vars`. The importer must merge group vars into host vars.

### Feature file `import_ansible.feature`

```gherkin
Feature: Ansible inventory import

  Background:
    Given a Groundwork server is running

  Scenario: Tagged inventory creates Deployables, Services, Exposes, Dependencies
    When I import the ansible fixture "tagged_inventory.yaml"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should contain a deployable named "checkout"
    And that deployable's "team" should be "payments-team"
    And that deployable's "repo_url" should be "https://github.com/acme/checkout"
    When I GET "/service/api"
    Then the response array should contain a service named "checkout-api"
    And the response array should contain a service named "payments-stripe"
    When I query the "exposes" graph with: { getAll { deployable_id service_id port protocol } }
    Then the response data should describe a link from "checkout" to "checkout-api" with port "8080" and protocol "http"
    When I query the "dependency" graph with: { getAll { deployable_id service_id criticality } }
    Then the response data should describe a dependency from "checkout" on "payments-stripe" with criticality "high"

  Scenario: Untagged inventory creates nothing
    When I import the ansible fixture "untagged_inventory.yaml"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should have 0 items

  Scenario: Group vars are inherited by hosts
    When I import the ansible fixture "group_vars_inheritance.yaml"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should contain a deployable named "billing"
    And that deployable's "team" should be "payments-team"
    And that deployable's "repo_url" should be "https://github.com/acme/payments-monorepo"
    And the response array should contain a deployable named "invoicing"
    And that deployable's "team" should be "payments-team"
```

---

## BDD: export scenarios

### Feature file `export_ansible.feature`

```gherkin
Feature: Ansible inventory export

  Background:
    Given a Groundwork server is running
    And I have registered deployable "checkout" with team "payments-team" and repo_url "https://github.com/acme/checkout"
    And I have registered service "checkout-api"
    And I have recorded that "checkout" exposes "checkout-api" on port 8080 protocol "http"

  Scenario: Exported inventory contains the deployable as a host with groundwork tags
    When I GET "/export/ansible"
    Then the response status should be 200
    And the response content-type should contain "yaml"
    And the response body should contain "groundwork_deployable: checkout"
    And the response body should contain "groundwork_team: payments-team"
    And the response body should contain "groundwork_exposes"
    And the response body should contain "checkout-api"

  Scenario: Round-trip — import an inventory, then export, parses back to equivalent state
    Given I have cleared the catalogue
    And I import the ansible fixture "tagged_inventory.yaml"
    When I GET "/export/ansible"
    Then the exported YAML should round-trip with the original fixture (modulo grouping)

  Scenario: Filter by team
    Given I have registered deployable "support-tool" with team "support-team"
    When I GET "/export/ansible?team=payments-team"
    Then the response status should be 200
    And the response body should contain "groundwork_deployable: checkout"
    And the response body should not contain "support-tool"
```

The "modulo grouping" caveat: Ansible inventory grouping is not reversible from a flat list of (deployable, team) pairs alone. The round-trip test compares the **set** of `(deployable_name, team, repo_url, exposes, dependencies)` — not the group hierarchy.

---

## Slice 0 — preflight

- [ ] **Step 0.1: Confirm Phase 1 (and Phase 2 if landed) is on `main`**
- [ ] **Step 0.2: If Phase 2 has not landed, lift `catalog.rs` + `apply_delta` + `Repos` from Phase 2 into this phase as a slice 0.5**

---

## Slice 1 — Ansible types and parser

### Task 1A: Define minimal Ansible inventory types **(delegate)**

**Files:** `groundwork/src/importers/ansible/types.rs`

**Delegate prompt:**

> Create `groundwork/src/importers/ansible/types.rs` with serde-deserialisable types matching the structure of an Ansible YAML inventory:
>
> ```rust
> use serde::{Deserialize, Serialize};
> use std::collections::BTreeMap;
>
> /// Top-level inventory: a tree under "all".
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct Inventory {
>     pub all: Group,
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct Group {
>     #[serde(default)] pub vars:     BTreeMap<String, serde_yaml::Value>,
>     #[serde(default)] pub hosts:    BTreeMap<String, BTreeMap<String, serde_yaml::Value>>,
>     #[serde(default)] pub children: BTreeMap<String, Group>,
> }
> ```
>
> Add tests showing it deserialises the three fixture files (read via `include_str!` from `tests/fixtures/ansible/`).
>
> Run `cargo test --lib importers::ansible::types` and confirm clean.

### Task 1B: Implement `parse` **(delegate)**

**Files:** `groundwork/src/importers/ansible/parse.rs`

**Delegate prompt:**

> Implement `pub fn parse(yaml: &str) -> anyhow::Result<crate::catalog::CatalogDelta>` in `groundwork/src/importers/ansible/parse.rs`.
>
> Algorithm:
> 1. Deserialise into `Inventory` (from types.rs).
> 2. Recursively walk the group tree. For each group, accumulate vars by merging parent vars with child group vars (child overrides parent).
> 3. For each host, merge group vars with host-level vars (host overrides group).
> 4. If the merged vars contain `groundwork_deployable`:
>    a. Push a `DeployableInput { name: groundwork_deployable, description: groundwork_description, repo_url: groundwork_repo, team: groundwork_team, origin: <ansible group path, e.g. "payments"> }`.
>    b. For each entry in `groundwork_exposes` (a list of objects `{name, port, protocol}`): push `ServiceInput { name }` and `ExposesInput { deployable_name, service_name, port, protocol }`.
>    c. For each entry in `groundwork_depends_on` (a list of objects `{name, criticality, protocol, auth_method}`): push `ServiceInput { name }` (so we register the consumed service even if no Deployable exposes it) and `DependencyInput { deployable_name, service_name, ... }`.
> 5. Deduplicate `ServiceInput` entries by name (multiple deployables may reference the same service).
> 6. Return the assembled CatalogDelta.
>
> Edge cases:
> - A host without `groundwork_deployable`: ignore it.
> - A `groundwork_exposes` entry that is a bare string (e.g. `["checkout-api"]`): treat it as `{ name: "checkout-api" }` with no port/protocol.
> - Empty inventory or only-an-`all`-group: returns an empty CatalogDelta.
>
> Inline unit tests covering the three fixtures.
>
> Run `cargo test --lib importers::ansible::parse` and confirm clean.

- [ ] **Step 1B.1: Write fixture files** (yourself)
- [ ] **Step 1B.2: Delegate parser**
- [ ] **Step 1B.3: Verify, commit**

```bash
cargo test --lib importers::ansible 2>&1 | tail -20
git add groundwork/src/importers/ansible groundwork/tests/fixtures/ansible/
git commit -m "feat: ansible inventory parser (groundwork-tagged hosts → CatalogDelta)"
```

---

## Slice 2 — Ansible emitter

### Task 2A: Implement `emit` **(delegate)**

**Files:** `groundwork/src/importers/ansible/emit.rs`

**Delegate prompt:**

> Implement `pub fn emit(snapshot: &crate::catalog::CatalogSnapshot) -> anyhow::Result<String>` in `groundwork/src/importers/ansible/emit.rs`.
>
> Algorithm:
> 1. Group deployables by `team` (or "ungrouped" if team is None).
> 2. Build an `Inventory` where each team is a child group of `all`.
> 3. For each deployable:
>    a. Choose a host name: `<deployable.name>.example.com` if `origin` is None, else `<deployable.name>.<origin>.example.com`. (This is a placeholder; users will edit it.)
>    b. Build host vars: `groundwork_deployable`, `groundwork_repo`, `groundwork_description`, plus `groundwork_exposes` (from snapshot.exposes) and `groundwork_depends_on` (from snapshot.dependencies).
>    c. Hoist `groundwork_team` to the group level (so it doesn't repeat per-host).
>    d. Hoist `groundwork_repo` to the group level if **all** deployables in the group share it; otherwise leave it on each host.
> 4. Serialise via `serde_yaml::to_string` of the resulting Inventory.
>
> Inline unit tests:
> - Snapshot with one deployable + one service + one exposes → emitted YAML reparses to equivalent CatalogDelta.
> - Snapshot with multiple deployables in the same team → group var hoisting works.
>
> Run `cargo test --lib importers::ansible` clean.

- [ ] **Step 2A.1: Delegate**
- [ ] **Step 2A.2: Verify, commit**

---

## Slice 3 — HTTP routes

### Task 3A: Wire `/import/ansible` and `/export/ansible`

**Files:** `groundwork/src/main.rs`

- [ ] **Step 3A.1: Mirror the k8s import/export handlers**

Same shape as Phase 2 Slice 4. Wraps `importers::ansible::parse` and `importers::ansible::emit`. The export handler accepts an optional `?team=<name>` query param and filters the snapshot by team before emitting.

- [ ] **Step 3A.2: Compile, commit**

```bash
cargo build 2>&1 | tail -10
git add groundwork/src/main.rs
git commit -m "feat: HTTP routes /import/ansible and /export/ansible"
```

---

## Slice 4 — BDD scenarios + step defs

Mirrors Phase 2 Slice 5, with ansible-flavoured step defs. Reuse:

- `the response array should contain a deployable named "..."`
- `that deployable's "FIELD" should be "VALUE"`
- `the response data should describe a dependency from "..." on "..." with criticality "..."` — new step def, drills into `data.getAll[]` looking for a row where `deployable_id` resolves (via cached id-by-name) to the named deployable and similar for service_id and criticality.

- [ ] **Step 4.1: Add step defs**
- [ ] **Step 4.2: Run, iterate parser/emitter until green**
- [ ] **Step 4.3: Commit**

```bash
git add -A
git commit -m "test: BDD scenarios for ansible import/export with round-trip"
```

---

## Definition of done — Phase 3

- [ ] All scenarios in `import_ansible.feature` and `export_ansible.feature` pass.
- [ ] Round-trip property `parse(emit(snapshot)) == snapshot` holds for the three fixtures (modulo grouping).
- [ ] Hand-written `ansible-inventory --list -i <exported.yaml>` (manual check on a host with `ansible-core` installed) parses without error.
- [ ] No regression in Phases 1–2.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Ansible's INI inventory format is more common in the wild than YAML. | Phase 3 covers YAML only. INI parsing is a follow-on; users can convert with `ansible-inventory --list -y > inventory.yaml`. |
| `groundwork_exposes` and `groundwork_depends_on` are an invented convention. | Documented in `groundwork/README.md` as the contract. The convention also doubles as a self-documenting tag for inventory authors. |
| Ansible group vars also live in `group_vars/<group>.yml` files (separate from inventory). | Phase 3 only parses inline `vars:` blocks. The README mentions the limitation and points users at `ansible-inventory --list -y` to flatten. |
| Deployable's `team` is a string; multiple Deployables in different "teams" but same "group" lose group structure on export. | The emitter chooses team as the grouping axis — that matches the v0.2 model. If users want a different grouping later, that's a flag on the export endpoint. |
