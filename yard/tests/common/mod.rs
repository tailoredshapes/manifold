//! Shared cucumber harness helpers for Yard's BDD suite. Yard pulls
//! Deployables out of Groundwork, ChangeRequests out of Cityhall, and Teams
//! out of Union — all three get tiny in-process stub servers here.

#![allow(dead_code)]

pub mod stub_cityhall;
pub mod stub_groundwork;
pub mod stub_union;
