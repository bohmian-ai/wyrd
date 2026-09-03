//! Oracle capability binary: the query read path.
//!
//! Proves Oracle query execution and its edges — published and fused
//! reconciliation, role-separated and distributed dispatch, multitenant
//! isolation and fairness, spill accounting, peer security, Scribe tail
//! fencing, cancellation and terminal recovery, audit relay, the public gRPC
//! frame-parity and resource-release journeys, the typed Vala route cut, and
//! the Oracle production-telemetry contract.
//!
//! Setup: Postgres plus a booted server; the peer journey stands up a
//! three-server follower cluster.
//!
//! Owning lane: `mise run test:bifrost:journey:oracle` runs this binary whole
//! with `--include-ignored --test-threads=1`, and `mise run
//! test:bifrost:journey` runs it as one of the tier-1 capabilities. A test
//! added to a module below runs in both with no `mise.toml` edit.
//!
//! Out of scope: the write path that produces what these queries read
//! (`scribe`), compaction (`forge`), OTLP ingest (`otlp`), multi-pod load
//! (`cluster`), and server boot and mount (`server`).
//!
//! # Module layout
//!
//! `oracle_support` holds fixtures shared by more than one module and contains
//! no tests. Every other module owns one theme of the read path and holds the
//! helpers only it uses, so a journey can be read without reading the shared
//! module first.

mod analytical_activation;
mod analytical_inactive;
mod capacity;
mod convergence;
mod distributed;
mod grpc_surface;
mod layout;
mod observability;
mod peer;
mod peer_network;
mod published;
mod recovery;
mod spill;
mod support;
