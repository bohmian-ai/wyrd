//! Oracle capability binary: the query read path.
//!
//! Proves Oracle query execution and its edges — published and fused
//! reconciliation, role-separated and distributed dispatch, multitenant
//! isolation and fairness, spill accounting, peer security, Scribe tail
//! fencing, cancellation and terminal recovery, audit relay, the public gRPC
//! frame-parity and resource-release journeys, the typed Vala route cut, and
//! the Oracle production-telemetry contract. `oracle_peer` additionally boots
//! the mixed-role peer composition and proves follower owners start clean and
//! settle through the production cluster shutdown path.
//!
//! Setup: Postgres plus a booted server; the peer journey stands up a
//! three-server follower cluster.
//!
//! Owning lane: `mise run test:bifrost:journey`, which runs this binary whole
//! with `--include-ignored --test-threads=1`.
//! `mise run test:bifrost:oracle-distributed-parity` also runs it whole, which
//! is how the follower parity case stays covered now that `oracle_peer` is a
//! module here rather than its own target. A test added to a module below runs
//! in both with no `mise.toml` edit.
//!
//! Out of scope: the write path that produces what these queries read
//! (`scribe`), compaction (`forge`), OTLP ingest (`otlp`), multi-pod load
//! (`cluster`), and server boot and mount (`server`).

#[path = "oracle_edge_journeys.rs"]
mod oracle_edge_journeys;
#[path = "oracle_peer.rs"]
mod oracle_peer;
