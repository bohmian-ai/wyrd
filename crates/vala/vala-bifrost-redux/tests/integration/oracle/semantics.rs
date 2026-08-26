//! SQL semantics over published, cross-epoch, and fused live sources.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use arrow::array::{
    Array, ArrayRef, FixedSizeBinaryBuilder, Int32Array, Int64Array, StringArray,
    TimestampMicrosecondArray, UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vala_bifrost_redux::oracle::{
    Oracle, OracleConfig, TailTransportDirectory, TestPostgresOracleAudit,
};
use vala_bifrost_redux::schema::with_managed_columns;
use vala_bifrost_redux::scribe::memtable::Memtable;
use vala_bifrost_redux::scribe::seal_key::SealKey;
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailService, LocalTailReadTransport, ScribeTailReader, TailFenceConfig,
};
use vala_bifrost_redux::scribe::wal::{ScribeAppendMeta, WalLsn};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryTerminalOutcome, VisibilityMode,
};

use super::support::*;

/// Returns all non-null `UInt64` values for one named result column.
///
/// # Panics
///
/// Panics when the column is absent, has a different Arrow type, or contains
/// nulls.
fn uint64_values(result: &DecodedQuery, column: &str) -> Vec<u64> {
    let index = result.schema.index_of(column).expect("result column");
    result
        .batches
        .iter()
        .flat_map(|batch| {
            let values = batch
                .column(index)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("UInt64 result");
            assert_eq!(values.null_count(), 0);
            values.values().iter().copied().collect::<Vec<_>>()
        })
        .collect()
}

/// `PublishedOnly` runs the locked SQL operator matrix over real hot Parquet.
///
/// Boots the expensive Oracle fixture exactly once, then drives the full
/// read-operator matrix through three cohesive assertion helpers that share the
/// booted `oracle`/`fixture` read-only, and finally confirms the read-only
/// provider rejects `DELETE`, the persisted plan classifications match, and the
/// Oracle shuts down cleanly. Split into helpers to keep each unit focused (and
/// under the line ceiling) without paying for a second fixture boot; the helpers
/// preserve every original assertion and value.
#[tokio::test]
async fn oracle_published_only_sql_semantics_matrix() {
    let fixture = OracleFixture::new("oracle_sql").await;
    let _ = fixture
        .seed_hot_rows(&[
            (7, fixture.tenant),
            (2, fixture.tenant),
            (7, fixture.tenant),
        ])
        .await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let table = fixture.table.fqn();
    assert_projection_predicate_aggregate_distinct(&oracle, &fixture, &table).await;
    assert_window_sorted_empty(&oracle, &fixture, &table).await;
    assert_join_matrix(&oracle, &fixture, &table).await;
    let delete = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("DELETE FROM {table} WHERE value = 7"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await;
    assert!(
        delete.is_err(),
        "read-only Oracle provider must reject DELETE"
    );
    assert_sql_matrix_classes(&fixture).await;
    shutdown_oracle(&oracle).await;
}

/// Production Oracle consumes both cross-epoch Scribe-shaped generations.
#[tokio::test]
async fn oracle_reads_both_automatic_cross_epoch_artifacts() {
    let fixture = OracleFixture::new("scribe_cross_epoch").await;
    let first = fixture
        .seed_hot_rows_at_epoch(
            "scribe-018f7ca27a4d7cc198a797fdd1f15101-epoch-1-shard-0-wal-1-1-00000.parquet",
            &[(11, fixture.tenant)],
            1,
        )
        .await;
    let second = fixture
        .seed_hot_rows_at_epoch(
            "scribe-018f7ca27a4d7cc198a797fdd1f15101-epoch-2-shard-0-wal-1-1-00000.parquet",
            &[(22, fixture.tenant)],
            2,
        )
        .await;
    assert_ne!(first.file_path, second.file_path);
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let result = published_query(
        &oracle,
        &fixture,
        format!("SELECT value FROM {} ORDER BY value", fixture.table.fqn()),
    )
    .await;
    assert_eq!(int64_values(&result, "value"), [11, 22]);
    assert_query_succeeded(&result);
    shutdown_oracle(&oracle).await;
}

/// Asserts one decoded matrix query terminated in `Success` with no error.
///
/// Replaces the original single post-matrix terminal-outcome loop with a
/// per-query check invoked by each matrix helper; the assertion (outcome is
/// `Success`, terminal error absent) is identical for every query.
fn assert_query_succeeded(result: &DecodedQuery) {
    assert_eq!(result.terminal.outcome, QueryTerminalOutcome::Success);
    assert!(result.terminal.error.is_none());
}

/// Asserts the projection, predicate, aggregate, and distinct matrix queries.
///
/// Runs the four simplest read operators against the seeded hot `table` and
/// verifies their schemas, values, and row counts, and that each query
/// terminates in `Success`. Borrows the once-booted `oracle`/`fixture`
/// read-only; `table` is the fully-qualified table name.
async fn assert_projection_predicate_aggregate_distinct(
    oracle: &Oracle,
    fixture: &OracleFixture,
    table: &str,
) {
    let projection = published_query(oracle, fixture, format!("SELECT value FROM {table}")).await;
    assert_eq!(projection.schema.fields().len(), 1);
    assert_eq!(projection.schema.field(0).name(), "value");
    assert_eq!(projection.schema.field(0).data_type(), &DataType::Int64);
    let mut projection_values = int64_values(&projection, "value");
    projection_values.sort_unstable();
    assert_eq!(projection_values, [2, 7, 7]);
    assert_eq!(projection.terminal.row_count, 3);
    assert_query_succeeded(&projection);

    let predicate = published_query(
        oracle,
        fixture,
        format!("SELECT value FROM {table} WHERE value >= 7 ORDER BY value LIMIT 1"),
    )
    .await;
    assert_eq!(int64_values(&predicate, "value"), [7]);
    assert_eq!(predicate.terminal.row_count, 1);
    assert_query_succeeded(&predicate);

    let aggregate = published_query(
        oracle,
        fixture,
        format!("SELECT count(*) AS total FROM {table}"),
    )
    .await;
    assert_eq!(aggregate.schema.field(0).name(), "total");
    assert_eq!(int64_values(&aggregate, "total"), [3]);
    assert_query_succeeded(&aggregate);

    let distinct = published_query(
        oracle,
        fixture,
        format!("SELECT DISTINCT value FROM {table} ORDER BY value"),
    )
    .await;
    assert_eq!(int64_values(&distinct, "value"), [2, 7]);
    assert_query_succeeded(&distinct);
}

/// Asserts the window, descending-sort, and empty-result matrix queries.
///
/// Exercises the window function, an `ORDER BY ... DESC LIMIT` sort, and a
/// predicate that matches no rows, verifying schemas, values, row counts, and
/// that each query terminates in `Success`. Borrows the once-booted
/// `oracle`/`fixture` read-only; `table` is the fully-qualified table name.
async fn assert_window_sorted_empty(oracle: &Oracle, fixture: &OracleFixture, table: &str) {
    let window = published_query(
        oracle,
        fixture,
        format!(
            "SELECT value, row_number() OVER (ORDER BY value) AS ordinal \
             FROM {table} ORDER BY ordinal"
        ),
    )
    .await;
    assert_eq!(window.schema.fields().len(), 2);
    assert_eq!(int64_values(&window, "value"), [2, 7, 7]);
    assert_eq!(uint64_values(&window, "ordinal"), [1, 2, 3]);
    assert_query_succeeded(&window);

    let sorted = published_query(
        oracle,
        fixture,
        format!("SELECT value FROM {table} ORDER BY value DESC LIMIT 2"),
    )
    .await;
    assert_eq!(int64_values(&sorted, "value"), [7, 7]);
    assert_eq!(sorted.terminal.row_count, 2);
    assert_query_succeeded(&sorted);

    let empty = published_query(
        oracle,
        fixture,
        format!("SELECT value FROM {table} WHERE false"),
    )
    .await;
    assert_eq!(empty.schema.fields().len(), 1);
    assert_eq!(empty.schema.field(0).name(), "value");
    assert!(empty.batches.iter().all(|batch| batch.num_rows() == 0));
    assert_eq!(empty.terminal.row_count, 0);
    assert_query_succeeded(&empty);
}

/// Asserts the self-join matrix query and its terminal outcome.
///
/// Runs the self-join over `table`, verifying its two-column schema, both value
/// columns, row count, and that it terminates in `Success`. Borrows the
/// once-booted `oracle`/`fixture` read-only; `table` is the fully-qualified
/// table name.
async fn assert_join_matrix(oracle: &Oracle, fixture: &OracleFixture, table: &str) {
    let joined = published_query(
        oracle,
        fixture,
        format!(
            "SELECT a.value AS left_value, b.value AS right_value \
             FROM {table} a JOIN {table} b ON a.value = b.value \
             ORDER BY left_value, right_value"
        ),
    )
    .await;
    assert_eq!(joined.schema.fields().len(), 2);
    assert_eq!(int64_values(&joined, "left_value"), [2, 7, 7, 7, 7]);
    assert_eq!(int64_values(&joined, "right_value"), [2, 7, 7, 7, 7]);
    assert_eq!(joined.terminal.row_count, 5);
    assert_query_succeeded(&joined);
}

/// Verifies the optimized-plan class persisted for every SQL matrix query.
async fn assert_sql_matrix_classes(fixture: &OracleFixture) {
    let mut conn = fixture
        .pg
        .tenant_conn_for(fixture.tenant)
        .await
        .expect("tenant conn");
    let classes: Vec<String> = sqlx::query_scalar(
        "SELECT detail::jsonb->>'query_class' FROM vala.audit_outbox \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND operation = 'bifrost.query.read_decision' ORDER BY seq",
    )
    .fetch_all(&mut **conn.transaction())
    .await
    .expect("optimized-plan classifications");
    conn.commit().await.expect("classification read commit");
    assert_eq!(
        classes,
        [
            "interactive",
            "interactive",
            "analytical",
            "analytical",
            "analytical",
            "interactive",
            "interactive",
            "analytical"
        ]
    );
}

/// Fused discovers and drains a real Scribe stream before the first seal.
#[tokio::test]
pub(crate) async fn oracle_fused_live_only_real_scribe_and_degraded_policy() {
    let fixture = OracleFixture::new("oracle_live").await;
    let tails = live_only_tail_directory(&fixture);
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            tails,
            OracleConfig::default(),
        )
        .await;
    let fused = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("live-only Fused query");
    let fused_terminal = terminal(fused).await;
    assert_eq!(fused_terminal.outcome, QueryTerminalOutcome::Success);
    assert_eq!(fused_terminal.row_count, 1);
    oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
    assert_degraded_live_policy(&fixture).await;
}

/// Builds one real local Scribe tail containing a single live-only row.
fn live_only_tail_directory(fixture: &OracleFixture) -> Arc<TailTransportDirectory> {
    let day = vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1);
    let stream = StreamIdentity::new(NodeId::new(uuid::Uuid::now_v7()), WriterEpoch::new(7));
    let memtable = Arc::new(Memtable::new());
    let key = SealKey::new(fixture.tenant, fixture.table.clone(), day);
    let schema = Arc::new(Schema::new(with_managed_columns(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )])));
    let mut batch_ids = FixedSizeBinaryBuilder::with_capacity(1, 16);
    batch_ids
        .append_value(uuid::Uuid::now_v7().as_bytes())
        .expect("batch id");
    let rows = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(vec![11])) as ArrayRef,
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![None::<String>])),
            Arc::new(StringArray::from(vec![uuid::Uuid::now_v7().to_string()])),
            Arc::new(StringArray::from(vec![RequestId::now_v7().to_string()])),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_000]).with_timezone("UTC")),
            Arc::new(TimestampMicrosecondArray::from(vec![1_000_001]).with_timezone("UTC")),
            Arc::new(batch_ids.finish()),
            Arc::new(Int32Array::from(vec![0])),
            Arc::new(StringArray::from(vec![fixture.tenant.to_string()])),
        ],
    )
    .expect("live physical batch");
    memtable
        .insert(
            &key,
            audit_event("oracle.fixture.live"),
            ScribeAppendMeta {
                batch_id: [7; 16],
                schema_fingerprint: [0; 32],
                data_digest: [0; 32],
                data_len: 0,
                payload_digest: [0; 32],
                payload_len: 0,
                slice_index: 0,
                slice_count: 1,
                rows_accepted: 1,
                wal_lsn_min: WalLsn::new(1),
                wal_lsn_max: WalLsn::new(1),
                seal_key: key.as_path_components(),
            },
            rows,
        )
        .expect("live append");
    let reader = Arc::new(ScribeTailReader::new(
        Arc::new(FetchLiveTailService::new(
            stream,
            memtable,
            vala_bifrost_redux::scribe::tail_resources_for_test(),
        )),
        TailFenceConfig::default(),
    ));
    let tails = Arc::new(TailTransportDirectory::default());
    tails.insert_live_stream(
        fixture.table.fqn(),
        wyrd_spec::vala::api::NodeId::new(stream.node_id.as_uuid()),
        u64::try_from(stream.writer_epoch.as_i64()).expect("epoch"),
        vala_bifrost_redux::partition_fixtures::day_partition(1970, 1, 1).to_wire(),
        Arc::new(LocalTailReadTransport::new(reader)),
    );
    tails
}

/// Verifies strict rejection and degraded completion when no live route exists.
async fn assert_degraded_live_policy(fixture: &OracleFixture) {
    let degraded_oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;
    let strict = degraded_oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect_err("strict requires a live route");
    assert_eq!(strict, BifrostError::QueryVisibilityUnavailable);
    let degraded = degraded_oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT * FROM {}", fixture.table.fqn()),
                visibility: VisibilityMode::Fused,
                freshness: FreshnessPolicy::AllowDegraded,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("degraded query");
    assert_eq!(
        terminal(degraded).await.outcome,
        QueryTerminalOutcome::Degraded
    );
    degraded_oracle
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await;
}
