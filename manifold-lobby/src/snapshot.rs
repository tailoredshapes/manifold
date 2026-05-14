//! Snapshot of the federated graph at a moment in time.
//!
//! The derivation engine periodically pulls this from the four primary
//! meshlettes (via /graph GraphQL queries) and feeds it to each derivation
//! rule. Pulling a fresh snapshot is the polling-equivalent of consuming an
//! event stream: every change between snapshots will surface in the next
//! derivation pass. When merkql-notify (or any other push event source) is
//! later wired in, the engine swaps the polling loop for stream consumption
//! without rule code changing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone)]
pub struct GraphSnapshot {
    pub deployables: Vec<Deployable>,
    pub services: Vec<Service>,
    pub dependencies: Vec<Dependency>,
    pub exposes: Vec<Exposes>,
    pub contracts: Vec<Contract>,
    pub change_requests: Vec<ChangeRequest>,
    pub deployment_plans: Vec<DeploymentPlan>,
    pub test_environments: Vec<TestEnvironment>,
    pub data_syncs: Vec<DataSync>,
    pub work_orders: Vec<WorkOrder>,
    pub teams: Vec<Team>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Deployable {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub team_id: Option<String>,
    pub deployment_status: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub id: String,
    pub deployable_id: String,
    pub service_id: String,
    pub criticality: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Exposes {
    pub id: String,
    pub deployable_id: String,
    pub service_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: String,
    pub service_id: String,
    pub format: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChangeRequest {
    pub id: String,
    pub summary: Option<String>,
    pub status: Option<String>,
    pub tier: Option<String>,
    pub target_deployables: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct DeploymentPlan {
    pub id: String,
    pub change_request_id: String,
    pub steps: Vec<PlanStepLite>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct PlanStepLite {
    pub deployable_id: String,
    pub deployable_name: String,
    pub order: usize,
    #[serde(default)]
    pub window_start: Option<String>,
    #[serde(default)]
    pub window_end: Option<String>,
    #[serde(default)]
    pub test_environment_id: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TestEnvironment {
    pub id: String,
    pub name: String,
    pub kind: Option<String>,
    pub deployable_id: Option<String>,
    pub watershed: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DataSync {
    pub id: String,
    pub source_env_id: Option<String>,
    pub target_env_id: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WorkOrder {
    pub id: String,
    pub status: Option<String>,
    pub deployable_id: Option<String>,
    pub team_id: Option<String>,
    pub blocker: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
}
