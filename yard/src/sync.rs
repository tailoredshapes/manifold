//! Data-sync recommender.
//!
//! Given a Groundwork dependency edge between two deployables, recommend the
//! right `DataSync.kind` for the equivalent test-env edge:
//!
//! - Event-based  → `push`   (publisher writes to a pipeline; consumer reads)
//! - API-based    → `pull`   (consumer pulls from publisher during setup)
//! - Shared DB    → `shared` (both envs talk to one data lake)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependencyEdge {
    Event,
    Api,
    SharedDb,
}

impl DependencyEdge {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "event" | "event-based" | "events" => Some(Self::Event),
            "api" | "api-based" | "rpc" | "http" | "rest" | "grpc" => Some(Self::Api),
            "shared_db" | "shared-db" | "shared" | "db" | "database" => Some(Self::SharedDb),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecommendedSync {
    pub kind: String,
    pub rationale: String,
}

pub fn recommend_sync(edge: DependencyEdge) -> RecommendedSync {
    match edge {
        DependencyEdge::Event => RecommendedSync {
            kind: "push".into(),
            rationale:
                "event-based dependency: publish events from source env into a pipeline the target env consumes"
                    .into(),
        },
        DependencyEdge::Api => RecommendedSync {
            kind: "pull".into(),
            rationale:
                "API-based dependency: target env pulls fixtures from source env during test setup"
                    .into(),
        },
        DependencyEdge::SharedDb => RecommendedSync {
            kind: "shared".into(),
            rationale:
                "shared-database dependency: both envs read/write the same test data lake"
                    .into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_recommends_push() {
        assert_eq!(recommend_sync(DependencyEdge::Event).kind, "push");
    }

    #[test]
    fn api_recommends_pull() {
        assert_eq!(recommend_sync(DependencyEdge::Api).kind, "pull");
    }

    #[test]
    fn shared_recommends_shared() {
        assert_eq!(recommend_sync(DependencyEdge::SharedDb).kind, "shared");
    }

    #[test]
    fn parse_aliases() {
        assert_eq!(DependencyEdge::parse("Event"), Some(DependencyEdge::Event));
        assert_eq!(DependencyEdge::parse("rest"), Some(DependencyEdge::Api));
        assert_eq!(
            DependencyEdge::parse("database"),
            Some(DependencyEdge::SharedDb)
        );
        assert_eq!(DependencyEdge::parse("smoke-signals"), None);
    }
}
