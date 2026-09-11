//! Public OTLP journeys for the canonical Bifrost signal tables.
//!
//! Every case here is a complete public path: a real client encodes an OTLP
//! export exactly as an OpenTelemetry exporter would, sends it through one of
//! the three exposed transports (HTTP protobuf, HTTP protobuf-JSON, gRPC),
//! crosses the real Scribe acknowledgment and publication boundaries, and reads
//! the stored row back through the public canonical SQL contract.
//!
//! Setup: Postgres plus a bound server. Lane:
//! `mise run test:bifrost:journey:otlp`.
//!
//! What this binary leaves to another: Scribe's own durability, admission and
//! restart semantics belong to the `scribe` binary; query planning, dispatch
//! and topology belong to `oracle`; the language SDK and agent projections of
//! the same canonical rows belong to `vala-sdk`'s `pg_bifrost_e2e`, the Python
//! and TypeScript journey suites, and `wyrd-mcp`'s `mcp` binary.

mod logs_export;
mod metrics_export;
mod negative;
mod support;
mod trace_export;
mod trace_export_http;
