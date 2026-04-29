//! Cityhall library — bylaw resolver, plan compiler, Mermaid Gantt emitter.
//!
//! The binary in `main.rs` consumes this library; the BDD harness reuses it to
//! construct in-process test servers without duplicating the planning logic.

pub mod bylaw;
pub mod gantt;
pub mod groundwork_client;
pub mod plan;
