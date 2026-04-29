# Phase 4 — Terraform Import / Export

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps tagged **(delegate)** are good Qwen units. The HCL parsing surface is more complex than YAML, so favour smaller delegated chunks here.

**Pre-requisite:** Phase 1 complete; the catalog/apply_delta machinery from whichever earlier phase landed first.

**Goal:** Add bidirectional integration between the Groundwork catalogue and Terraform HCL:
- `POST /import/terraform` parses HCL (one or more `.tf` files concatenated) into Deployables + Services + Exposes (+ Dependencies where derivable).
- `GET /export/terraform?provider=aws|gcp|azure` renders catalogue slices as HCL stubs.

**Architecture:** A `groundwork::importers::terraform` module mirroring k8s/ansible. Like Ansible, we adopt a **tag-based contract**: resources opt in to the catalogue with a `groundwork_*` tag map. Without tags, a resource is ignored.

**Tech Stack:** Add `hcl-rs = "0.18"` (the [pure-Rust HCL parser](https://crates.io/crates/hcl-rs)). No HashiCorp tools needed.

---

## Why a tag-based contract (again)

Same reason as Ansible: Terraform resources are too varied to interpret automatically. A `aws_ecs_service` is a Deployable. A `aws_lambda_function` is also a Deployable. A `aws_db_instance` might or might not be a Service depending on whether it's exposed. Rather than maintain a heuristic per resource type, we ask users to opt in:

```hcl
resource "aws_ecs_service" "checkout" {
  name = "checkout"
  # ... ECS config ...

  tags = {
    groundwork_deployable = "checkout"
    groundwork_team       = "payments-team"
    groundwork_repo       = "https://github.com/acme/checkout"
    groundwork_exposes    = "checkout-api:8080:http"
    groundwork_depends_on = "payments-stripe:high"
  }
}

resource "aws_db_instance" "payments_db" {
  # ...
  tags = {
    groundwork_service     = "payments-db"
    groundwork_service_type = "database"
    groundwork_endpoint    = "internal"
  }
}
```

Tag values are strings; lists are encoded as comma-separated strings of records: `"name1:port1:proto1,name2:port2:proto2"`. The parser splits on `,` then `:`. (Not pretty, but tags only support strings.)

For users that prefer a richer expression, we accept a parallel `groundwork_*_json` tag whose value is a JSON-encoded list. The parser checks for the JSON form first.

---

## File map

| File | Operation |
|---|---|
| `groundwork/Cargo.toml` | Modify — add `hcl-rs = "0.18"` |
| `groundwork/src/importers/terraform.rs` | Create — mod root |
| `groundwork/src/importers/terraform/types.rs` | Create — minimal HCL types |
| `groundwork/src/importers/terraform/parse.rs` | Create — `parse(&str) -> Result<CatalogDelta>` |
| `groundwork/src/importers/terraform/emit.rs` | Create — `emit(&CatalogSnapshot) -> Result<String>` |
| `groundwork/src/main.rs` | Modify — `/import/terraform` + `/export/terraform` |
| `groundwork/tests/features/import_terraform.feature` | Create |
| `groundwork/tests/features/export_terraform.feature` | Create |
| `groundwork/tests/fixtures/terraform/aws_ecs_service.tf` | Create |
| `groundwork/tests/fixtures/terraform/aws_managed_db.tf` | Create |
| `groundwork/tests/fixtures/terraform/multi_resource.tf` | Create |
| `groundwork/tests/fixtures/terraform/untagged.tf` | Create |
| `groundwork/tests/fixtures/terraform/json_tags.tf` | Create |

---

## BDD: import scenarios

### Fixture `aws_ecs_service.tf`

```hcl
resource "aws_ecs_service" "checkout" {
  name            = "checkout"
  cluster         = aws_ecs_cluster.payments.id
  task_definition = aws_ecs_task_definition.checkout.arn
  desired_count   = 3

  tags = {
    groundwork_deployable  = "checkout"
    groundwork_team        = "payments-team"
    groundwork_repo        = "https://github.com/acme/checkout"
    groundwork_description = "Checkout flow service"
    groundwork_exposes     = "checkout-api:8080:http"
    groundwork_depends_on  = "payments-stripe:high"
  }
}
```

### Fixture `aws_managed_db.tf`

```hcl
resource "aws_db_instance" "payments_db" {
  identifier = "payments-prod"
  engine     = "postgres"
  # ...

  tags = {
    groundwork_service      = "payments-db"
    groundwork_service_type = "database"
    groundwork_endpoint     = "payments-db.internal:5432"
  }
}
```

### Fixture `multi_resource.tf`

Combines `aws_ecs_service.checkout` and `aws_db_instance.payments_db` and an `aws_lambda_function` opting in. Plus the dependency: `groundwork_depends_on = "payments-db:high"` on the ECS service.

### Fixture `untagged.tf`

Has resources without `groundwork_*` tags. Importer must ignore them.

### Fixture `json_tags.tf`

Uses the JSON variant:

```hcl
resource "aws_ecs_service" "billing" {
  name = "billing"
  tags = {
    groundwork_deployable      = "billing"
    groundwork_exposes_json    = "[{\"name\":\"billing-api\",\"port\":\"8080\",\"protocol\":\"http\"},{\"name\":\"billing-grpc\",\"port\":\"9090\",\"protocol\":\"grpc\"}]"
    groundwork_depends_on_json = "[{\"name\":\"payments-db\",\"criticality\":\"high\"}]"
  }
}
```

### Feature file `import_terraform.feature`

```gherkin
Feature: Terraform HCL import

  Background:
    Given a Groundwork server is running

  Scenario: A tagged ECS service becomes a Deployable with Exposes and Dependency
    When I import the terraform fixture "aws_ecs_service.tf"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should contain a deployable named "checkout"
    And that deployable's "team" should be "payments-team"
    When I GET "/service/api"
    Then the response array should contain a service named "checkout-api"
    And the response array should contain a service named "payments-stripe"
    When I query the "exposes" graph with: { getAll { deployable_id service_id port protocol } }
    Then the response data should describe a link from "checkout" to "checkout-api" with port "8080" and protocol "http"
    When I query the "dependency" graph with: { getAll { deployable_id service_id criticality } }
    Then the response data should describe a dependency from "checkout" on "payments-stripe" with criticality "high"

  Scenario: A tagged db_instance becomes a Service with no Deployable
    When I import the terraform fixture "aws_managed_db.tf"
    Then the response status should be 200
    When I GET "/service/api"
    Then the response array should contain a service named "payments-db"
    When I GET "/deployable/api"
    Then the response array should have 0 items

  Scenario: Multi-resource HCL imports all tagged resources
    When I import the terraform fixture "multi_resource.tf"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should have at least 2 items
    When I GET "/service/api"
    Then the response array should contain a service named "payments-db"

  Scenario: Untagged HCL imports nothing
    When I import the terraform fixture "untagged.tf"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should have 0 items

  Scenario: JSON-formatted exposes/depends_on tags are parsed
    When I import the terraform fixture "json_tags.tf"
    Then the response status should be 200
    When I GET "/service/api"
    Then the response array should contain a service named "billing-api"
    And the response array should contain a service named "billing-grpc"
    When I query the "exposes" graph with: { getAll { deployable_id service_id port protocol } }
    Then the response data should describe a link from "billing" to "billing-grpc" with port "9090" and protocol "grpc"
```

---

## BDD: export scenarios

### Feature file `export_terraform.feature`

```gherkin
Feature: Terraform HCL export

  Background:
    Given a Groundwork server is running
    And I have registered deployable "checkout" with team "payments-team" and repo_url "https://github.com/acme/checkout"
    And I have registered service "checkout-api"
    And I have recorded that "checkout" exposes "checkout-api" on port 8080 protocol "http"

  Scenario: Exported HCL stub for AWS provider includes a tagged ECS service
    When I GET "/export/terraform?provider=aws"
    Then the response status should be 200
    And the response content-type should contain "hcl"
    And the response body should contain "resource \"aws_ecs_service\" \"checkout\""
    And the response body should contain "groundwork_deployable = \"checkout\""
    And the response body should contain "groundwork_exposes    = \"checkout-api:8080:http\""

  Scenario: Round-trip — import a tagged HCL, then export, parses back to equivalent state
    Given I have cleared the catalogue
    And I import the terraform fixture "aws_ecs_service.tf"
    When I GET "/export/terraform?provider=aws"
    Then the exported HCL should round-trip with the original fixture (modulo resource scaffolding)
```

The "modulo resource scaffolding" caveat: we don't preserve every `aws_ecs_service` field (cluster, task_definition, etc.) because the catalogue doesn't model them. We only round-trip the **tags** and the resource identity. The compare uses `parse_to_delta` and compares the deltas.

---

## Slice 0 — preflight

- [ ] **Step 0.1: Add `hcl-rs` to Cargo.toml**

```toml
hcl-rs = "0.18"
```

```bash
cargo build 2>&1 | tail -10
```

- [ ] **Step 0.2: Familiarise with `hcl-rs` API**

```bash
cargo doc --open -p hcl-rs
```

The key types: `hcl::Body`, `hcl::Block`, `hcl::Attribute`, `hcl::Value`, `hcl::Expression`. Parse via `hcl::from_str`. Emit via `hcl::to_string`.

- [ ] **Step 0.3: Commit**

```bash
git add groundwork/Cargo.toml groundwork/Cargo.lock
git commit -m "chore: add hcl-rs dependency for terraform import/export"
```

---

## Slice 1 — Terraform parser

### Task 1A: Define the HCL helpers **(delegate)**

**Files:** `groundwork/src/importers/terraform/types.rs`

**Delegate prompt:**

> Create `groundwork/src/importers/terraform/types.rs` with helpers for inspecting HCL `Block`s. Define:
>
> ```rust
> /// A resource block: `resource "<type>" "<name>" { ... }`.
> /// Returns (resource_type, resource_name, body).
> pub fn resource_blocks<'a>(body: &'a hcl::Body) -> impl Iterator<Item=(&'a str, &'a str, &'a hcl::Body)> { ... }
>
> /// Extract the `tags = { ... }` attribute from a body. Returns a map of strings.
> pub fn extract_tags(body: &hcl::Body) -> std::collections::BTreeMap<String, String> { ... }
> ```
>
> Plus rustdoc and inline tests reading `tests/fixtures/terraform/aws_ecs_service.tf` via `include_str!`.
>
> Run `cargo test --lib importers::terraform::types` clean.

### Task 1B: Implement `parse` **(delegate, in two halves)**

**Files:** `groundwork/src/importers/terraform/parse.rs`

**Delegate prompt (half 1 — tag → records):**

> In `groundwork/src/importers/terraform/parse.rs`, implement helpers:
>
> ```rust
> /// Parse a `groundwork_exposes` tag value into a list of (name, port, protocol).
> /// Format: "name1:port1:proto1,name2:port2:proto2"
> /// Trailing/missing fields default to None.
> pub fn parse_exposes_tag(s: &str) -> Vec<(String, Option<String>, Option<String>)>;
>
> /// Parse a `groundwork_depends_on` tag value into a list of (name, criticality).
> /// Format: "name1:crit1,name2:crit2"
> pub fn parse_depends_on_tag(s: &str) -> Vec<(String, Option<String>)>;
>
> /// Parse the JSON form: a JSON array of objects.
> pub fn parse_exposes_json(s: &str) -> anyhow::Result<Vec<(String, Option<String>, Option<String>)>>;
> pub fn parse_depends_on_json(s: &str) -> anyhow::Result<Vec<(String, Option<String>)>>;
> ```
>
> Inline unit tests with at least:
> - `"a:1:http,b:2:grpc"` → two entries.
> - `"a"` → one entry, name="a", others None.
> - JSON form with two entries.
>
> Run `cargo test --lib importers::terraform::parse` clean.

**Delegate prompt (half 2 — main parse function):**

> In the same file, implement `pub fn parse(hcl: &str) -> anyhow::Result<crate::catalog::CatalogDelta>`.
>
> Algorithm:
> 1. `let body: hcl::Body = hcl::from_str(hcl)?`
> 2. For each resource block, call `extract_tags`. Pull `groundwork_*` keys.
> 3. Branch on which `groundwork_*` keys are present:
>    - `groundwork_deployable`: this resource is a Deployable. Push DeployableInput; parse `groundwork_exposes`(_json) and `groundwork_depends_on`(_json) and push corresponding ServiceInput + ExposesInput / DependencyInput records.
>    - `groundwork_service`: this resource is a Service. Push ServiceInput { name: groundwork_service, type: groundwork_service_type, endpoint: groundwork_endpoint }.
>    - Both: it's a Deployable that itself is a Service (rare but legal). Emit both records.
>    - Neither: ignore.
> 4. Deduplicate by name.
> 5. Return the CatalogDelta.
>
> Inline tests covering the four fixture files.
>
> Run `cargo test --lib importers::terraform` clean.

- [ ] **Step 1B.1: Write fixtures (yourself)**
- [ ] **Step 1B.2: Delegate (half 1)**
- [ ] **Step 1B.3: Delegate (half 2)**
- [ ] **Step 1B.4: Verify, commit**

```bash
cargo test --lib importers::terraform 2>&1 | tail -20
git add groundwork/src/importers/terraform groundwork/tests/fixtures/terraform/
git commit -m "feat: terraform HCL parser (groundwork-tagged resources → CatalogDelta)"
```

---

## Slice 2 — Terraform emitter

### Task 2A: Implement `emit` **(delegate)**

**Files:** `groundwork/src/importers/terraform/emit.rs`

**Delegate prompt:**

> Implement `pub fn emit(snapshot: &crate::catalog::CatalogSnapshot) -> anyhow::Result<String>` and a private helper `pub fn emit_for_provider(snapshot, provider: &str) -> anyhow::Result<String>` where `provider` is "aws"|"gcp"|"azure".
>
> Algorithm:
> 1. For each Deployable in the snapshot, emit a resource block based on `provider`:
>    - aws → `resource "aws_ecs_service" "<deployable.name>" { name = "<deployable.name>"; tags = { ... } }`
>    - gcp → `resource "google_cloud_run_service" "<deployable.name>" { ... }`
>    - azure → `resource "azurerm_container_group" "<deployable.name>" { ... }`
> 2. The body should contain mostly placeholders (`# TODO: configure cluster, image, etc.`). The tags are the **only** part we round-trip.
> 3. For each Service that no Deployable exposes (managed/external), emit a placeholder `resource` block (e.g. for aws: `aws_db_instance` if type="database", else a comment).
> 4. Concatenate as one HCL string. Build the body via `hcl::Body::builder()` then serialise with `hcl::to_string`.
>
> Inline unit tests:
> - emit(snapshot with one deployable+service+exposes for provider=aws) reparses to same delta.
> - emit produces compilable HCL (run `terraform validate` is out of scope; just check `hcl::from_str` round-trips).
>
> Run `cargo test --lib importers::terraform::emit` clean.

- [ ] **Step 2A.1: Delegate**
- [ ] **Step 2A.2: Verify, commit**

---

## Slice 3 — HTTP routes

Mirror the Phase 2 / Phase 3 shape:

- `POST /import/terraform` body=HCL text → 200 + JSON ApplyReport
- `GET /export/terraform?provider=aws|gcp|azure[&deployable=<name>]` → 200 + HCL text

- [ ] **Step 3.1: Wire**
- [ ] **Step 3.2: Compile**
- [ ] **Step 3.3: Commit**

---

## Slice 4 — BDD scenarios + step defs

Reuse the import-fixture step from Phase 2 with a `terraform/` prefix; add an `I import the terraform fixture "..."` variant. Iterate parser/emitter until green.

- [ ] **Step 4.1: Step defs + scenarios run**
- [ ] **Step 4.2: Commit**

---

## Definition of done — Phase 4

- [ ] All scenarios in `import_terraform.feature` and `export_terraform.feature` pass.
- [ ] Round-trip property holds for the five fixtures (modulo resource scaffolding).
- [ ] `terraform validate` (manual) on a sample export YAML succeeds — even if `terraform plan` would fail because the resource bodies are placeholders.
- [ ] No regression in earlier phases.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| `hcl-rs` doesn't preserve comments or formatting on round-trip. | Out of scope. We only care about *catalogue* equivalence. |
| Tag values are strings only; richer types lost. | The JSON tag form (`groundwork_*_json`) is the escape hatch. |
| Different cloud providers have different resource shapes. | The emitter has a per-provider switch; only AWS resource shapes are first-class. GCP and Azure variants ship as best-effort with TODO comments. Real-world authors will edit the export anyway. |
| HCL's expression evaluation (variable references, locals) means `tags` could be a reference, not a literal map. | The parser reads literal maps only. References produce no records (silent skip + log). The README documents this as a known limitation. |
