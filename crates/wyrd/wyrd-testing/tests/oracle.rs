//! Oracle query / edge / public-gRPC journey binary.
//!
//! Surface-scoped test target carved out of the former monolithic `integration`
//! binary (T54). It holds the Oracle read-path journeys — published/fused/
//! role-separated/distributed/multitenant reconciliation, peer security, terminal
//! recovery, audit relay, the public gRPC frame-parity and resource-release
//! journeys, the typed Vala route cut, and the Oracle production-telemetry
//! contract. `oracle_edge_journeys` calls `crate::oracle_peer::prove_*` helpers,
//! so `oracle_peer.rs` is compiled into this binary as well — the same shared-file
//! coupling the former `integration` binary carried. `oracle_peer.rs` also remains
//! its own `oracle_peer` target; this pre-existing duplication is preserved, not
//! multiplied. Modules point at unchanged files via `#[path]`, so every test name
//! and `#[ignore]` gate is retained.

#[path = "oracle_edge_journeys.rs"]
mod oracle_edge_journeys;
