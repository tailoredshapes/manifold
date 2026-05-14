//! In-memory state Lobby uses across the HTTP layer and the derivation engine.

use meshql_core::Repository;
use std::sync::Arc;

#[derive(Clone)]
pub struct Entity {
    pub repo: Arc<dyn Repository>,
    pub searcher: Arc<dyn meshql_core::Searcher>,
}

#[derive(Clone)]
pub struct AppState {
    pub advisory: Entity,
    pub program: Entity,
    pub program_membership: Entity,
    pub lifecycle_entry: Entity,
    pub saved_view: Entity,
    pub comment: Entity,
}
