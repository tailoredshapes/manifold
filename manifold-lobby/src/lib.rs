//! Lobby — advisory tier and cross-program coordination surface for Manifold.
//!
//! Pure read-and-derive over the federated graph: subscribes to the other four
//! meshlettes' event streams (or polls their /graph endpoints when no stream
//! is configured) and surfaces persistent, derived concerns as *advisories*.
//! Lobby itself owns the advisory entity and its lifecycle, plus programs,
//! reservations, saved views, and comments — but no underlying facts.
//!
//! See docs/superpowers/specs/2026-05-13-lobby-design.md for the full design.

pub mod engine;
pub mod rules;
pub mod snapshot;
pub mod sources;
pub mod state;
