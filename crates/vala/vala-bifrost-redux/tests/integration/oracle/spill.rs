//! Configured memory spills to scratch and still returns an exact count.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use std::time::{Duration, Instant};
use vala_bifrost_redux::oracle::{OracleConfig, TailTransportDirectory, TestPostgresOracleAudit};
use wyrd_spec::vala::api::QueryTerminalOutcome;

use super::support::*;

/// A real hot Parquet scan crosses the configured reconciliation ceiling.
#[tokio::test]
async fn oracle_configured_memory_spills_and_preserves_exact_count() {
    let fixture = OracleFixture::new("oracle_spill").await;
    let rows = (0_i64..1_024)
        .map(|value| (value, fixture.tenant))
        .collect::<Vec<_>>();
    let decoded_batch_bytes = fixture.seed_hot_rows(&rows).await.memory_bytes;
    let reconciliation_limit_bytes = decoded_batch_bytes.saturating_mul(3) / 4;
    assert!(reconciliation_limit_bytes > 0);
    assert!(
        decoded_batch_bytes > reconciliation_limit_bytes,
        "fixture must cross the configured in-memory ceiling"
    );
    let oracle = fixture
        .oracle_with_memory(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
            reconciliation_limit_bytes,
        )
        .await;
    let result = published_query(
        &oracle,
        &fixture,
        format!("SELECT count(*) AS total FROM {}", fixture.table.fqn()),
    )
    .await;
    assert_eq!(result.terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(int64_values(&result, "total"), [1_024]);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}
