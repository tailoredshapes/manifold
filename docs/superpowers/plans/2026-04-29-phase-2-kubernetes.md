# Phase 2 — Kubernetes Import / Export

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps are bite-sized; many are good candidates for delegation to Qwen via `claude-code-delegate`. Tag each step with **(delegate)** when it is suitable for a junior implementation. Steps without that tag are integration work; do them yourself.

**Pre-requisite:** Phase 1 complete. The catalogue must already have Deployable, Service, Exposes, Dependency entities.

**Goal:** Add bidirectional integration between the Groundwork catalogue and Kubernetes manifests:
- `POST /import/kubernetes` parses a YAML manifest (one or more documents, `Deployment` and/or `Service` kinds) into Deployable + Service + Exposes records and applies them via the existing repos.
- `GET /export/kubernetes?deployable=<name>&namespace=<ns>` renders a `CatalogSnapshot` slice as Kubernetes YAML.

**Architecture:** A self-contained `groundwork::importers::k8s` module with `parse(&str) -> CatalogDelta` and `emit(&CatalogSnapshot) -> String`. A new `groundwork::catalog` module defines `CatalogDelta`, `CatalogSnapshot`, and `apply_delta(&CatalogDelta, &Repos) -> Result<ApplyReport>`. HTTP routes in `main.rs` glue them together.

**Tech Stack:** Add `serde_yaml = "0.9"` and `kube` types from a minimal local model (avoid the full `kube-rs` dependency — we only need to deserialise; we don't talk to a cluster). Custom small types are fine.

---

## File map

| File | Operation |
|---|---|
| `groundwork/Cargo.toml` | Modify — add `serde_yaml = "0.9"` |
| `groundwork/src/catalog.rs` | Create — `CatalogDelta`, `CatalogSnapshot`, `apply_delta` |
| `groundwork/src/importers/mod.rs` | Create — `pub mod k8s; pub mod ansible; pub mod terraform;` (only `k8s` exists yet) |
| `groundwork/src/importers/k8s.rs` | Create — parser + emitter |
| `groundwork/src/importers/k8s/types.rs` | Create — minimal k8s manifest types (Deployment, Service, ObjectMeta, etc.) |
| `groundwork/src/importers/k8s/parse.rs` | Create — `parse(&str) -> Result<CatalogDelta>` |
| `groundwork/src/importers/k8s/emit.rs` | Create — `emit(&CatalogSnapshot) -> Result<String>` |
| `groundwork/src/main.rs` | Modify — add routes, hand the `Repos` bundle to handlers |
| `groundwork/tests/features/import_kubernetes.feature` | Create — BDD scenarios for import |
| `groundwork/tests/features/export_kubernetes.feature` | Create — BDD scenarios for export |
| `groundwork/tests/fixtures/k8s/deployment_only.yaml` | Create — golden fixture |
| `groundwork/tests/fixtures/k8s/deployment_and_service.yaml` | Create — golden fixture |
| `groundwork/tests/fixtures/k8s/multi_doc.yaml` | Create — golden fixture |
| `groundwork/tests/fixtures/k8s/managed_external.yaml` | Create — Service with no selector |
| `groundwork/tests/fixtures/k8s/multi_container.yaml` | Create — Deployment with multiple containers |
| `groundwork/tests/groundwork_cert.rs` | Modify — new step defs for fixture loading + import asserts |

---

## Cross-cutting types

### `CatalogDelta`

```rust
// groundwork/src/catalog.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogDelta {
    pub deployables:  Vec<DeployableInput>,
    pub services:     Vec<ServiceInput>,
    pub exposes:      Vec<ExposesInput>,
    pub dependencies: Vec<DependencyInput>,
    pub contracts:    Vec<ContractInput>,
    pub slas:         Vec<SlaInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployableInput {
    pub name: String,
    pub description: Option<String>,
    pub repo_url:    Option<String>,
    pub team:        Option<String>,
    /// k8s namespace, ansible group, terraform module — preserved as free-form metadata
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInput {
    pub name: String,
    pub r#type: Option<String>,   // "api", "database", "queue", ...
    pub description: Option<String>,
    pub endpoint:    Option<String>,
}

/// Refers to deployable + service by **name**, not UUID. apply_delta resolves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposesInput {
    pub deployable_name: String,
    pub service_name:    String,
    pub port:     Option<String>,
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInput {
    pub deployable_name: String,
    pub service_name:    String,
    pub protocol:    Option<String>,
    pub auth_method: Option<String>,
    pub criticality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractInput {
    pub service_name: String,
    pub spec_url: Option<String>,
    pub version:  Option<String>,
    pub format:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaInput {
    pub service_name: String,
    pub contract_version: Option<String>,
    pub metric: Option<String>,
    pub target: Option<String>,
    pub window: Option<String>,
}
```

### `apply_delta`

```rust
pub struct Repos {
    pub deployable:  Arc<dyn meshql_core::Repository>,
    pub service:     Arc<dyn meshql_core::Repository>,
    pub exposes:     Arc<dyn meshql_core::Repository>,
    pub dependency:  Arc<dyn meshql_core::Repository>,
    pub contract:    Arc<dyn meshql_core::Repository>,
    pub sla:         Arc<dyn meshql_core::Repository>,
    pub deployable_searcher: Arc<dyn meshql_core::Searcher>,
    pub service_searcher:    Arc<dyn meshql_core::Searcher>,
}

#[derive(Debug, Default, Serialize)]
pub struct ApplyReport {
    pub created_deployables: Vec<String>,
    pub created_services:    Vec<String>,
    pub created_exposes:     usize,
    pub created_dependencies: usize,
    pub skipped_existing_deployables: Vec<String>,
    pub skipped_existing_services:    Vec<String>,
}

pub async fn apply_delta(delta: &CatalogDelta, repos: &Repos) -> anyhow::Result<ApplyReport>;
```

`apply_delta` is **idempotent on names**: if a Deployable or Service with the same `name` already exists, reuse its ID; do not modify other fields. Exposes/Dependency are matched by `(deployable_id, service_id)` pair.

---

## BDD: import scenarios

### Fixture `deployment_only.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: checkout
  namespace: payments
  labels:
    app.kubernetes.io/name: checkout
    app.kubernetes.io/team: payments-team
  annotations:
    groundwork.io/repo: https://github.com/acme/checkout
    groundwork.io/description: Checkout flow service
spec:
  replicas: 3
  selector:
    matchLabels:
      app: checkout
  template:
    metadata:
      labels:
        app: checkout
    spec:
      containers:
        - name: checkout
          image: ghcr.io/acme/checkout:v1.2.3
          ports:
            - containerPort: 8080
              name: http
              protocol: TCP
```

### Fixture `deployment_and_service.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: orders
  namespace: orders
  labels:
    app: orders
spec:
  selector:
    matchLabels:
      app: orders
  template:
    metadata:
      labels:
        app: orders
    spec:
      containers:
        - name: orders
          image: ghcr.io/acme/orders:v0.9.0
          ports:
            - containerPort: 8080
              name: http
              protocol: TCP
---
apiVersion: v1
kind: Service
metadata:
  name: orders-api
  namespace: orders
spec:
  type: ClusterIP
  selector:
    app: orders
  ports:
    - port: 80
      targetPort: 8080
      protocol: TCP
      name: http
```

### Fixture `managed_external.yaml`

```yaml
apiVersion: v1
kind: Service
metadata:
  name: payments-stripe
  namespace: payments
  annotations:
    groundwork.io/managed: "true"
spec:
  type: ExternalName
  externalName: api.stripe.com
  ports:
    - port: 443
      protocol: TCP
```

### Fixture `multi_doc.yaml`

Two Deployments + two Services, all in one file separated by `---`.

### Fixture `multi_container.yaml`

A single Deployment with two containers (e.g. `app` and `sidecar`). Importer must produce **one** Deployable.

### Feature file `import_kubernetes.feature`

```gherkin
Feature: Kubernetes manifest import

  Background:
    Given a Groundwork server is running

  Scenario: Import a Deployment-only manifest creates a Deployable
    When I import the kubernetes fixture "deployment_only.yaml"
    Then the response status should be 200
    And the response body should contain "created_deployables"
    When I GET "/deployable/api"
    Then the response array should contain a deployable named "checkout"
    And that deployable's "team" should be "payments-team"
    And that deployable's "repo_url" should be "https://github.com/acme/checkout"
    And that deployable's "description" should be "Checkout flow service"
    And that deployable's "origin" should be "payments"

  Scenario: Import a Deployment + Service creates Deployable, Service, and Exposes
    When I import the kubernetes fixture "deployment_and_service.yaml"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should contain a deployable named "orders"
    When I GET "/service/api"
    Then the response array should contain a service named "orders-api"
    When I query the "exposes" graph with: { getAll { deployable_id service_id port protocol } }
    Then there should be no GraphQL errors
    And the response data should describe a link from "orders" to "orders-api" with port "8080" and protocol "http"

  Scenario: Multi-document YAML imports all objects
    When I import the kubernetes fixture "multi_doc.yaml"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should have at least 2 items
    When I GET "/service/api"
    Then the response array should have at least 2 items

  Scenario: Service with no selector creates a Service with no Exposes (managed/external)
    When I import the kubernetes fixture "managed_external.yaml"
    Then the response status should be 200
    When I GET "/service/api"
    Then the response array should contain a service named "payments-stripe"
    When I query the "exposes" graph with: { getByServiceId(service_id: "<service_id_of:payments-stripe>") { id } }
    Then the response data array should have 0 items

  Scenario: Multi-container Deployment yields one Deployable
    When I import the kubernetes fixture "multi_container.yaml"
    Then the response status should be 200
    When I GET "/deployable/api"
    Then the response array should contain exactly one deployable named "sidecar-host"

  Scenario: Re-import is idempotent
    When I import the kubernetes fixture "deployment_and_service.yaml"
    And I import the kubernetes fixture "deployment_and_service.yaml"
    When I GET "/deployable/api"
    Then the response array should have 1 item with name "orders"
    When I GET "/service/api"
    Then the response array should have 1 item with name "orders-api"
```

The step `<service_id_of:NAME>` is a placeholder syntax our resolver must support — extend `GroundworkWorld::resolve` to accept `<service_id_of:foo>` and look up the ID by name from the live server. Spec it explicitly in slice 0 below.

---

## BDD: export scenarios

### Feature file `export_kubernetes.feature`

```gherkin
Feature: Kubernetes manifest export

  Background:
    Given a Groundwork server is running
    And I have registered deployable "billing"
    And I have registered service "billing-api"
    And I have recorded that "billing" exposes "billing-api" on port 8080 protocol "http"

  Scenario: Export a deployable that exposes one service
    When I GET "/export/kubernetes?deployable=billing"
    Then the response status should be 200
    And the response content-type should contain "yaml"
    And the response body should contain "kind: Deployment"
    And the response body should contain "name: billing"
    And the response body should contain "kind: Service"
    And the response body should contain "name: billing-api"
    And the response body should contain "containerPort: 8080"

  Scenario: Export a deployable with multiple exposed services
    Given I have registered service "billing-grpc"
    And I have recorded that "billing" exposes "billing-grpc" on port 9090 protocol "grpc"
    When I GET "/export/kubernetes?deployable=billing"
    Then the response status should be 200
    And the response body should contain "name: billing-api"
    And the response body should contain "name: billing-grpc"
    And the response body should contain "containerPort: 9090"

  Scenario: Round-trip — import then export reproduces the input shape
    Given I have cleared the catalogue
    And I import the kubernetes fixture "deployment_and_service.yaml"
    When I GET "/export/kubernetes?deployable=orders"
    Then the response status should be 200
    And the exported YAML should round-trip with the original fixture
```

The "round-trip with the original fixture" assertion compares **structurally** (after parsing both back into our internal types), not byte-for-byte, because YAML formatters are noisy. Spec it via:

```rust
async fn yaml_round_trip(world: &mut GroundworkWorld, fixture_name: String) {
    let original_yaml = std::fs::read_to_string(format!("tests/fixtures/k8s/{fixture_name}")).unwrap();
    let original_delta = importers::k8s::parse(&original_yaml).unwrap();
    let exported_yaml = world.last_response_body.as_deref().unwrap();
    let reparsed_delta = importers::k8s::parse(exported_yaml).unwrap();
    assert_eq!(
        normalize_for_compare(&original_delta),
        normalize_for_compare(&reparsed_delta),
    );
}
```

Where `normalize_for_compare` sorts vecs by name and clears `origin` if not preserved.

---

## Slice 0 — preflight

- [ ] **Step 0.1: Confirm Phase 1 is on `main`**

```bash
git log --oneline -5
```

Expected: top commits include the Phase 1 commits (drop tech_stack, rename, add Exposes).

- [ ] **Step 0.2: Add `serde_yaml` to Cargo.toml**

```toml
[dependencies]
# ... existing ...
serde_yaml = "0.9"
```

```bash
cargo build 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 0.3: Commit**

```bash
git add groundwork/Cargo.toml groundwork/Cargo.lock
git commit -m "chore: add serde_yaml dependency for k8s import/export"
```

---

## Slice 1 — `CatalogDelta` and `apply_delta`

### Task 1A: Define types **(delegate)**

**Delegate prompt:**

> Create the file `groundwork/src/catalog.rs` containing the Rust types `CatalogDelta`, `DeployableInput`, `ServiceInput`, `ExposesInput`, `DependencyInput`, `ContractInput`, `SlaInput`, plus a stub `pub async fn apply_delta(...)` that returns `unimplemented!()`. The exact shape is in the master plan section "Cross-cutting types". Use `serde::{Deserialize, Serialize}`. Ensure `CatalogDelta` derives `Default`. Add `pub mod catalog;` to `groundwork/src/main.rs` (or `lib.rs` if one exists; main.rs is currently the only crate root). Run `cargo check` and confirm it compiles.

- [ ] **Step 1A.1: Delegate the type definitions**

- [ ] **Step 1A.2: Verify `cargo check` clean**

```bash
cargo check 2>&1 | tail -10
```

- [ ] **Step 1A.3: Commit**

```bash
git add groundwork/src/catalog.rs groundwork/src/main.rs
git commit -m "feat: add CatalogDelta types (apply_delta stubbed)"
```

### Task 1B: Implement `apply_delta`

**Files:** `groundwork/src/catalog.rs`

`apply_delta` must:
1. For each `DeployableInput`: search by `payload.name`. If found, record its ID. Else create.
2. For each `ServiceInput`: same.
3. For each `ExposesInput`: resolve `deployable_name` and `service_name` to IDs. Search exposes by `(deployable_id, service_id)`. If absent, create.
4. For each `DependencyInput`: same shape as exposes.
5. Contract/Sla: skipped in Phase 2 (filed as Phase 5+ work; leave the function arms as no-ops with a TODO).

**Delegate this in two parts:**

- [ ] **Step 1B.1: Helper `find_or_create_by_name` (delegate)**

> Implement `async fn find_or_create_by_name(searcher, repo, name: &str, payload: serde_json::Value) -> anyhow::Result<(String, bool)>` returning `(id, was_created)`. Use the `Searcher::search_for_one` (or whatever the meshql-core API is — look at how main.rs uses Searcher) with a `Stash` of `{"payload.name": name}`. If not found, call `repo.create(payload)` and return its id. Read `meshql-core` source under `/tank/repos/tailoredshapes/meshql-rs/meshql-core/` to find the exact trait method names. Write a small unit test in `catalog.rs` that constructs a fake repo, but if mocking the trait is awkward, skip the unit test and rely on the BDD coverage.

- [ ] **Step 1B.2: Wire `apply_delta` body**

Use the helper repeatedly. Resolve names → IDs in two passes (first all deployables + services, then all exposes/dependencies that reference them). Return an `ApplyReport`.

- [ ] **Step 1B.3: Verify with a unit test in `catalog.rs`**

A small test that constructs a `CatalogDelta` with one deployable and one service, points `apply_delta` at an in-memory SQLite-backed `Repos`, then asserts the rows are created. Code for the in-memory repo bootstrap is duplicated from `groundwork_cert.rs::make_pool`.

```bash
cargo test --lib 2>&1 | tail -10
```

- [ ] **Step 1B.4: Commit**

```bash
git add groundwork/src/catalog.rs
git commit -m "feat: apply_delta resolves natural keys and creates rows idempotently"
```

---

## Slice 2 — k8s parser

### Task 2A: Define minimal k8s types **(delegate)**

**Files:** `groundwork/src/importers/mod.rs`, `groundwork/src/importers/k8s/mod.rs`, `groundwork/src/importers/k8s/types.rs`

**Delegate prompt:**

> Create `groundwork/src/importers/mod.rs` with `pub mod k8s;`. Create `groundwork/src/importers/k8s/mod.rs` with `pub mod types; pub mod parse; pub mod emit; pub use parse::parse; pub use emit::emit;`.
>
> Create `groundwork/src/importers/k8s/types.rs`. Define minimal serde types matching kubernetes manifests we care about:
>
> ```rust
> #[derive(Debug, Deserialize, Serialize)]
> pub struct KubeObject {
>     #[serde(rename = "apiVersion")] pub api_version: String,
>     pub kind: String,
>     pub metadata: ObjectMeta,
>     pub spec: serde_yaml::Value, // we deserialise spec lazily
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct ObjectMeta {
>     pub name: String,
>     #[serde(default)] pub namespace: Option<String>,
>     #[serde(default)] pub labels: std::collections::BTreeMap<String, String>,
>     #[serde(default)] pub annotations: std::collections::BTreeMap<String, String>,
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct DeploymentSpec {
>     #[serde(default)] pub selector: Selector,
>     pub template: PodTemplate,
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct Selector { #[serde(default, rename = "matchLabels")] pub match_labels: std::collections::BTreeMap<String, String> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct PodTemplate { pub metadata: ObjectMeta, pub spec: PodSpec }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct PodSpec { pub containers: Vec<Container> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct Container {
>     pub name: String,
>     #[serde(default)] pub image: Option<String>,
>     #[serde(default)] pub ports: Vec<ContainerPort>,
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct ContainerPort {
>     #[serde(rename = "containerPort")] pub container_port: u16,
>     #[serde(default)] pub name: Option<String>,
>     #[serde(default)] pub protocol: Option<String>,
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct ServiceSpec {
>     #[serde(default)] pub r#type: Option<String>,
>     #[serde(default)] pub selector: std::collections::BTreeMap<String, String>,
>     #[serde(default)] pub ports: Vec<ServicePort>,
>     #[serde(default)] pub external_name: Option<String>,
> }
>
> #[derive(Debug, Deserialize, Serialize, Default)]
> pub struct ServicePort {
>     pub port: u16,
>     #[serde(default, rename = "targetPort")] pub target_port: Option<serde_yaml::Value>,
>     #[serde(default)] pub protocol: Option<String>,
>     #[serde(default)] pub name: Option<String>,
> }
> ```
>
> Run `cargo check` and confirm clean.

- [ ] **Step 2A.1: Delegate**
- [ ] **Step 2A.2: `cargo check` clean**
- [ ] **Step 2A.3: Commit**

### Task 2B: Implement `parse(&str) -> Result<CatalogDelta>` **(delegate)**

**Files:** `groundwork/src/importers/k8s/parse.rs`

**Delegate prompt:**

> Implement `pub fn parse(yaml: &str) -> anyhow::Result<crate::catalog::CatalogDelta>` in `groundwork/src/importers/k8s/parse.rs`.
>
> Algorithm:
> 1. Use `serde_yaml::Deserializer::from_str(yaml)` to handle multi-document YAML.
> 2. For each document, deserialise as `KubeObject` (defined in types.rs).
> 3. For `kind: Deployment`:
>    - Push `DeployableInput { name: meta.name, description: meta.annotations["groundwork.io/description"].clone(), repo_url: meta.annotations["groundwork.io/repo"].clone(), team: meta.labels["app.kubernetes.io/team"].clone(), origin: meta.namespace.clone() }`.
>    - Drill into spec → DeploymentSpec → template.spec.containers, collect all containerPort entries. Stash them keyed by Deployment name + each podTemplate label.
> 4. For `kind: Service`:
>    - Push `ServiceInput { name: meta.name, r#type: spec.type.clone(), description: meta.annotations["groundwork.io/description"].clone(), endpoint: spec.external_name.clone() }`.
>    - If spec.selector is non-empty, find any Deployment whose podTemplate.metadata.labels is a superset of selector. For each match, push an `ExposesInput { deployable_name: deployment.name, service_name: service.name, port: first containerPort that matches selector port (or first containerPort by default), protocol: lowercase(port.protocol.unwrap_or("TCP")) }`.
> 5. Return the assembled CatalogDelta.
>
> Notes:
> - Deserialise spec lazily: `let dspec: DeploymentSpec = serde_yaml::from_value(obj.spec.clone())?;`
> - The protocol field on ExposesInput should be the **port name** if the container port has one ("http", "grpc", etc.), else the lowercased protocol (`"tcp"`).
> - Write rustdoc comments on the public function describing inputs/outputs and edge cases.
> - Include unit tests inline (`#[cfg(test)] mod tests { ... }`) for at least:
>   - parsing `deployment_only.yaml`
>   - parsing `deployment_and_service.yaml`
>   - parsing `managed_external.yaml`
>   - parsing `multi_doc.yaml`
>
> Each unit test reads from `tests/fixtures/k8s/<file>.yaml` via `include_str!`.
>
> Run `cargo test --lib importers::k8s::parse` and confirm all tests pass.

- [ ] **Step 2B.1: Write fixture files** (yourself, not delegated — these are golden specs)

Save the four fixture YAML files listed above to `groundwork/tests/fixtures/k8s/`.

- [ ] **Step 2B.2: Delegate the parser**

- [ ] **Step 2B.3: Verify, commit**

```bash
cargo test --lib importers::k8s::parse 2>&1 | tail -15
git add groundwork/src/importers groundwork/tests/fixtures/k8s/
git commit -m "feat: kubernetes manifest parser (Deployment + Service → CatalogDelta)"
```

---

## Slice 3 — k8s emitter

### Task 3A: Implement `emit(&CatalogSnapshot) -> Result<String>` **(delegate)**

**Files:** `groundwork/src/importers/k8s/emit.rs`

**Delegate prompt:**

> Implement `pub fn emit(snapshot: &crate::catalog::CatalogSnapshot) -> anyhow::Result<String>` in `groundwork/src/importers/k8s/emit.rs`.
>
> Algorithm:
> 1. For each Deployable in the snapshot, produce a `KubeObject { kind: "Deployment", apiVersion: "apps/v1", metadata: ObjectMeta { name: dep.name, namespace: dep.origin (or "default"), labels: { "app.kubernetes.io/name": dep.name, "app.kubernetes.io/team": dep.team if Some }, annotations: { "groundwork.io/repo": dep.repo_url, "groundwork.io/description": dep.description } }, spec: serde_yaml::to_value(DeploymentSpec { selector: { match_labels: {"app": dep.name} }, template: PodTemplate { metadata: ObjectMeta { labels: {"app": dep.name} }, spec: PodSpec { containers: [Container { name: dep.name, ports: <derived from Exposes> }] } }) }`.
> 2. For each Exposes pointing at this deployable, add a containerPort to that Deployment's container. Port = exposes.port or 8080. protocol = "TCP".
> 3. For each Service in the snapshot, produce a `KubeObject { kind: "Service", apiVersion: "v1", metadata: ObjectMeta { name: svc.name, namespace: <inferred>, ... }, spec: ServiceSpec { type: svc.r#type, selector: {"app": <name of Deployable that exposes this service>}, ports: [...] } }`. If no Deployable exposes the service, omit the selector and set `externalName` to `svc.endpoint` if present.
> 4. Concatenate all KubeObjects with `---` separators using `serde_yaml::to_string` per object.
>
> Write inline unit tests:
> - `emit` of a snapshot with one Deployable + one Service + one Exposes round-trips through `parse` and matches.
> - `emit` of a snapshot with a Service that no Deployable exposes produces a Service with no selector.
>
> Run `cargo test --lib importers::k8s` and confirm all tests pass.

- [ ] **Step 3A.1: Delegate the emitter**

- [ ] **Step 3A.2: Verify, commit**

```bash
cargo test --lib importers::k8s 2>&1 | tail -15
git add groundwork/src/importers/k8s/emit.rs
git commit -m "feat: kubernetes manifest emitter (CatalogSnapshot → YAML)"
```

---

## Slice 4 — HTTP routes

### Task 4A: Wire `/import/kubernetes`

**Files:** `groundwork/src/main.rs`

- [ ] **Step 4A.1: Build a `Repos` bundle in `main()`**

Replace the per-entity `let application = make_entity(...)` lines with a single bundle construction so the bundle can be passed to handlers. Keep individual variables for the existing graphlette/restlette wiring.

```rust
let repos = std::sync::Arc::new(catalog::Repos {
    deployable: deployable.repo.clone(),
    service:    service.repo.clone(),
    exposes:    exposes.repo.clone(),
    dependency: dependency.repo.clone(),
    contract:   contract.repo.clone(),
    sla:        sla.repo.clone(),
    deployable_searcher: deployable.searcher.clone(),
    service_searcher:    service.searcher.clone(),
});
```

(Add `Clone` derive to `Repos` if needed; the underlying `Arc`s are cheap to clone.)

- [ ] **Step 4A.2: Add the import handler**

```rust
async fn import_kubernetes(
    State(repos): State<Arc<catalog::Repos>>,
    body: String,
) -> Response {
    match importers::k8s::parse(&body) {
        Ok(delta) => match catalog::apply_delta(&delta, &repos).await {
            Ok(report) => (
                axum::http::StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                serde_json::to_string(&report).unwrap_or_default(),
            ).into_response(),
            Err(e) => (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("apply error: {e}"),
            ).into_response(),
        },
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            format!("parse error: {e}"),
        ).into_response(),
    }
}
```

Mount it:

```rust
let extra = Router::new()
    // ... existing routes ...
    .route("/import/kubernetes", post(import_kubernetes))
    .with_state(repos.clone())
    // ... rest ...
```

### Task 4B: Wire `/export/kubernetes`

- [ ] **Step 4B.1: Snapshot helper**

In `catalog.rs`, add:

```rust
pub async fn snapshot(repos: &Repos, deployable_filter: Option<&str>) -> anyhow::Result<CatalogSnapshot>;
```

Searches all repos using `Searcher::search_for_many` with empty stash, filters deployables by name if requested, and only includes services exposed/depended on by the filtered deployables (or all if no filter).

- [ ] **Step 4B.2: Add the export handler**

```rust
async fn export_kubernetes(
    State(repos): State<Arc<catalog::Repos>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let filter = params.get("deployable").map(String::as_str);
    match catalog::snapshot(&repos, filter).await {
        Ok(snap) => match importers::k8s::emit(&snap) {
            Ok(yaml) => (
                axum::http::StatusCode::OK,
                [(header::CONTENT_TYPE, "application/yaml")],
                yaml,
            ).into_response(),
            Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("emit: {e}")).into_response(),
        },
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("snapshot: {e}")).into_response(),
    }
}
```

- [ ] **Step 4B.3: Compile-check**

```bash
cargo build 2>&1 | tail -15
```

- [ ] **Step 4B.4: Commit**

```bash
git add groundwork/src/main.rs groundwork/src/catalog.rs
git commit -m "feat: HTTP routes /import/kubernetes and /export/kubernetes"
```

---

## Slice 5 — BDD scenarios + step defs

### Task 5A: Step defs for fixture loading + import asserts

**Files:** `groundwork/tests/groundwork_cert.rs`

- [ ] **Step 5A.1: Add steps**

```rust
#[when(regex = r#"^I import the kubernetes fixture "(.+)"$"#)]
async fn import_k8s_fixture(world: &mut GroundworkWorld, fixture: String) {
    let path = format!("tests/fixtures/k8s/{fixture}");
    let yaml = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path} missing: {e}"));
    let url = format!("{}/import/kubernetes", world.base_url());
    let resp = world.client.post(&url).body(yaml).send().await.expect("request failed");
    let status = resp.status().as_u16();
    let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).map(String::from);
    let body = resp.text().await.unwrap_or_default();
    world.store_response(status, body, ct);
}

#[then(regex = r#"^the response array should contain a deployable named "(.+)"$"#)]
async fn array_contains_deployable_named(world: &mut GroundworkWorld, name: String) {
    let body = world.last_response_body.as_deref().unwrap_or("");
    let arr: serde_json::Value = serde_json::from_str(body).expect("not JSON");
    let arr = arr.as_array().expect("not array");
    let found = arr.iter().any(|item| item.pointer("/payload/name").and_then(|v| v.as_str()) == Some(name.as_str()));
    assert!(found, "no deployable named {name:?} in {body}");
    // Stash for follow-up steps:
    let item = arr.iter().find(|item| item.pointer("/payload/name").and_then(|v| v.as_str()) == Some(name.as_str())).unwrap();
    world.last_subject = Some(item.clone());
}

#[then(regex = r#"^that deployable's "(.+)" should be "(.+)"$"#)]
async fn last_subject_field_eq(world: &mut GroundworkWorld, field: String, expected: String) {
    let item = world.last_subject.as_ref().expect("no subject");
    let actual = item.pointer(&format!("/payload/{field}")).and_then(|v| v.as_str()).unwrap_or("");
    assert_eq!(actual, expected, "field {field} mismatch: got {actual:?}, want {expected:?}");
}
```

Add `last_subject: Option<serde_json::Value>` to `GroundworkWorld`.

Mirror similar helpers for service.

- [ ] **Step 5A.2: Extend `world.resolve` to handle `<service_id_of:NAME>`**

```rust
// inside resolve():
// For each <service_id_of:NAME>, look up the cached id_by_name map.
```

This requires a side-channel: when scenarios register entities via `Given I have registered service "..."`, store `(name → id)` in a `HashMap<String, String>` keyed by entity. Then resolve `<service_id_of:NAME>` from that map.

A simpler approach: use the existing `world.ids` map but namespace it by entity. Step defs that register services already do this in v0.1 — just confirm and reuse.

- [ ] **Step 5A.3: Compile-check**

```bash
cargo test --test groundwork_cert -- --help 2>&1 | tail -5
```

(Just to surface compile errors without running the suite yet.)

### Task 5B: Run the import scenarios

- [ ] **Step 5B.1: Run, expect to surface bugs**

```bash
cargo test --test groundwork_cert 2>&1 | tail -40
```

Iterate parser, emitter, apply_delta until the scenarios pass.

- [ ] **Step 5B.2: Commit**

```bash
git add -A
git commit -m "test: BDD scenarios for kubernetes import (and step defs)"
```

### Task 5C: Run the export scenarios

- [ ] **Step 5C.1: Add export step defs**

Add `the exported YAML should round-trip with the original fixture` step (per the spec above).

- [ ] **Step 5C.2: Run, iterate**

```bash
cargo test --test groundwork_cert 2>&1 | tail -40
```

- [ ] **Step 5C.3: Commit**

```bash
git add -A
git commit -m "test: BDD scenarios for kubernetes export with round-trip"
```

---

## Definition of done — Phase 2

- [ ] All cucumber scenarios in `import_kubernetes.feature` and `export_kubernetes.feature` pass.
- [ ] `cargo test --lib importers::k8s` passes (parser + emitter unit tests).
- [ ] `POST /import/kubernetes` works against a real running server with a real `kubectl get -o yaml` output of any toy cluster.
- [ ] `GET /export/kubernetes?deployable=<name>` returns valid YAML that `kubectl apply --dry-run=client -f -` accepts (manual verification).
- [ ] Round-trip property `parse(emit(snapshot)) == snapshot` (modulo IDs and field ordering) holds for all four fixtures.
- [ ] No regression: all Phase 1 scenarios still pass.

---

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| `serde_yaml`'s output formatting differs from `kubectl`'s. | The round-trip test compares parsed structs, not YAML text. The export tells the user "this is what we have"; whether it byte-matches the input is not the contract. |
| Selector → Deployment matching has corner cases (e.g. selector with multiple labels, partial matches, no match). | First implementation matches *exactly* (selector ⊆ deployment.podTemplate.labels). Edge cases get TODO comments. Add a fixture `selector_no_match.yaml` later if real-world usage surfaces issues. |
| `ContainerPort.protocol` of "TCP" lower-cased to "tcp" when our catalogue uses "http"/"grpc" semantics from port name. | The plan above explicitly resolves this: prefer port `name` ("http", "grpc") if set; otherwise lowercase `protocol`. |
| `Service.spec.selector` is empty (managed/external) but the catalogue still expects an Exposes link. | `apply_delta` does not invent Exposes records. The `managed_external.yaml` fixture pins this behaviour. |
| `kube-rs` would be a heavier but more correct dependency. | Skipped on purpose: we don't talk to a cluster, we just (de)serialise YAML. Custom types keep the dep tree small and the surface predictable. |
