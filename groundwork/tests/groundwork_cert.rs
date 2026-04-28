use cucumber::{World, given, then, when};
use reqwest::Client;
use std::collections::HashMap;

#[derive(Debug, Default, World)]
pub struct GroundworkWorld {
    pub server_addr: Option<String>,
    pub ids: HashMap<String, String>,
    pub timestamps: HashMap<String, f64>,
    pub last_response_status: Option<u16>,
    pub last_response_body: Option<String>,
    pub last_response_content_type: Option<String>,
}

impl GroundworkWorld {
    pub fn base_url(&self) -> &str {
        self.server_addr.as_deref().expect("server not started")
    }

    pub fn resolve(&self, s: &str) -> String {
        let mut result = s.to_string();
        for (k, v) in &self.ids {
            result = result.replace(&format!("<ids.{k}>"), v);
        }
        for (k, v) in &self.timestamps {
            result = result.replace(&format!("<timestamps.{k}>"), &v.to_string());
        }
        result
    }
}

// ── Step implementations ──────────────────────────────────────────────────────
// TODO: implement these steps

#[given("a Groundwork server is running")]
async fn start_server(_world: &mut GroundworkWorld) {
    todo!("start in-process Groundwork server, store addr in world.server_addr")
}

#[given(regex = r#"I have registered application "(.+)""#)]
async fn register_one(_world: &mut GroundworkWorld, _name: String) {
    todo!("POST to /application/api, store id in world.ids[name]")
}

#[given("I have registered applications:")]
async fn register_many(_world: &mut GroundworkWorld, step: &cucumber::Step) {
    todo!("iterate step.table, POST each, store ids")
}

#[given(regex = r#"I capture the current timestamp as "(.+)""#)]
async fn capture_timestamp(_world: &mut GroundworkWorld, _key: String) {
    todo!("store current unix timestamp in world.timestamps[key]")
}

#[given(regex = r#"I update application "(.+)" with body (.+)"#)]
async fn update_app(_world: &mut GroundworkWorld, _name: String, _body: String) {
    todo!("PUT to /application/api/<id>")
}

#[when(regex = r#"I (GET|DELETE) "(.+)""#)]
async fn http_get_delete(_world: &mut GroundworkWorld, _method: String, _path: String) {
    todo!("send GET or DELETE, store status + body in world")
}

#[when(regex = r#"I POST to "(.+)" with body (.+)"#)]
async fn http_post(_world: &mut GroundworkWorld, _path: String, _body: String) {
    todo!("send POST, store status + body + id in world")
}

#[when(regex = r#"I PUT "(.+)" with body (.+)"#)]
async fn http_put(_world: &mut GroundworkWorld, _path: String, _body: String) {
    todo!("send PUT, store status + body in world")
}

#[when(regex = r#"I query the "(.+)" graph with: (.+)"#)]
async fn graphql_query(_world: &mut GroundworkWorld, _entity: String, _query: String) {
    todo!("POST GraphQL query, store status + body in world")
}

#[then(regex = r"the response status should be (\d+)")]
async fn check_status(_world: &mut GroundworkWorld, _expected: u16) {
    todo!("assert world.last_response_status == expected")
}

#[then(regex = r#"the response body should contain "(.+)""#)]
async fn body_contains(_world: &mut GroundworkWorld, _expected: String) {
    todo!("assert world.last_response_body contains expected")
}

#[then(r#"the response body should have an "id" field"#)]
async fn body_has_id(_world: &mut GroundworkWorld) {
    todo!("parse JSON, assert id field exists and is non-empty")
}

#[then("the response body should be a JSON array")]
async fn body_is_array(_world: &mut GroundworkWorld) {
    todo!("parse JSON, assert it's an array")
}

#[then(regex = r"the response array should have (\d+) items")]
async fn array_has_items(_world: &mut GroundworkWorld, _expected: usize) {
    todo!("parse JSON array, assert length")
}

#[then(regex = r#"the response content-type should contain "(.+)""#)]
async fn check_content_type(_world: &mut GroundworkWorld, _expected: String) {
    todo!("assert world.last_response_content_type contains expected")
}

#[then("there should be no GraphQL errors")]
async fn no_graphql_errors(_world: &mut GroundworkWorld) {
    todo!("parse JSON, assert no errors field")
}

#[then(regex = r#"the response data should contain "(.+)""#)]
async fn response_data_contains(_world: &mut GroundworkWorld, _expected: String) {
    todo!("assert body contains expected")
}

#[then("the response data description should be null")]
async fn response_data_description_null(_world: &mut GroundworkWorld) {
    todo!("parse GraphQL response, assert description is null")
}

#[tokio::main]
async fn main() {
    GroundworkWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run_and_exit("tests/features")
        .await;
}
