//! Yard library — validators, estimator, sync recommender, run-history aggregator.
//!
//! `main.rs` consumes this library; the BDD harness reuses it in-process to
//! avoid duplicating planning logic.

pub mod cityhall_client;
pub mod estimator;
pub mod groundwork_client;
pub mod history;
pub mod sync;
pub mod validators;
