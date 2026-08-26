//! Oracle journeys — Published and fused reconciliation reads, and role-separated dispatch.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use wyrd_spec::vala::api::VisibilityMode;
use wyrd_testing::bifrost::BifrostClusterSpec;

use crate::support::*;

/// Proves exact PublishedOnly rows and a validated terminal through the public SDK.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_published_journey() {
    public_roundtrip(
        BifrostClusterSpec::one_mixed(),
        VisibilityMode::PublishedOnly,
        true,
        0,
        None,
        false,
    )
    .await
    .expect("PublishedOnly journey");
}

/// Proves a Fused query drains a live tonic tail without a Scribe flush.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_fused_reconcile_journey() {
    public_roundtrip(
        BifrostClusterSpec::one_mixed(),
        VisibilityMode::Fused,
        false,
        0,
        Some("bifrost_oracle_tail_pages_total"),
        false,
    )
    .await
    .expect("Fused journey");
}

/// Proves ingest-only gRPC write and query-only HTTP/Arrow read routing.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_oracle_role_separated_journey() {
    public_roundtrip(
        BifrostClusterSpec::role_separated(),
        VisibilityMode::Fused,
        false,
        2,
        Some("bifrost_oracle_tail_pages_total"),
        false,
    )
    .await
    .expect("role-separated journey");
}
