use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_testing::bifrost::BifrostHarness;

const TABLE_NAME: &str = "scribe_journey_events";
const ROWS_PER_APPEND: usize = 4;

/// Proves three Scribe pods isolate tenants and retire force-sealed generations.
///
/// # Panics
///
/// Panics when the Postgres-backed journey cannot start, verify, or shut down.
#[tokio::test]
#[ignore = "requires the Postgres-backed Bifrost journey environment"]
async fn multi_scribe_three_pods_three_tenants_persists_and_isolates_rows() {
    let harness = BifrostHarness::start(3, 3)
        .await
        .expect("real Bifrost harness");
    let result = run_journey(&harness).await;
    let shutdown = harness.shutdown().await;
    shutdown.expect("Bifrost harness shutdown");
    result.expect("Scribe journey");
}

async fn run_journey(harness: &BifrostHarness) -> Result<(), Box<dyn Error + Send + Sync>> {
    let table = TableRef::new(BifrostNamespace::Bifrost, TABLE_NAME);
    let mut appends = Vec::new();
    for (pod_index, scribe) in harness.scribes().iter().enumerate() {
        for (tenant_index, tenant) in harness.tenants().iter().copied().enumerate() {
            let scribe = Arc::clone(scribe);
            let table = table.clone();
            appends.push(tokio::spawn(async move {
                let principal = Principal {
                    id: PrincipalId::new(Uuid::now_v7()),
                    kind: PrincipalKind::User,
                    tenant_id: tenant,
                    roles: Vec::new(),
                    effective_permissions: PermissionSet::new(),
                };
                let rows = make_batch(pod_index, tenant_index);
                let schema_fingerprint =
                    SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
                scribe
                    .append(ScribeAppend {
                        principal,
                        table,
                        rows,
                        schema_fingerprint,
                        request_id: RequestId::now_v7(),
                        batch_id: Uuid::now_v7(),
                        measured_wire_bytes: 0,
                    })
                    .await
            }));
        }
    }
    for append in appends {
        append.await??;
    }

    harness.force_seal_all().await?;
    assert!(harness.is_drained()?);
    assert!(
        harness.scribes().iter().all(|scribe| scribe
            .memtable_stats()
            .is_ok_and(|stats| stats.immutable_generations > 0)),
        "force-seal completion must retain published generations"
    );
    wait_for_force_seal_retirement(harness).await?;

    let operator_pool = harness.cluster().pg_fixture().operator_pool();
    let (file_count, row_count): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE namespace = 'vala.bifrost' AND table_name = $1",
    )
    .bind(TABLE_NAME)
    .fetch_one(operator_pool.pool())
    .await?;
    assert_eq!(row_count, i64::try_from(3 * 3 * ROWS_PER_APPEND)?);
    assert_eq!(file_count, 9);

    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT file_path FROM vala.file_list
          WHERE namespace = 'vala.bifrost' AND table_name = $1",
    )
    .bind(TABLE_NAME)
    .fetch_all(operator_pool.pool())
    .await?;
    for path in paths {
        harness.cluster().storage_operator().stat(&path).await?;
    }

    for tenant in harness.tenants().iter().copied() {
        let mut conn = harness.tenant_conn(tenant).await?;
        let visible: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE namespace = 'vala.bifrost' AND table_name = $1",
        )
        .bind(TABLE_NAME)
        .fetch_one(&mut **conn.transaction())
        .await?;
        assert_eq!(visible, i64::try_from(3 * ROWS_PER_APPEND)?);
    }
    Ok(())
}

/// Drive the production age lifecycle beyond grace and observe full retirement.
///
/// # Errors
///
/// Returns an error when Scribe inspection fails or retained generations do not
/// release before the bounded deadline.
async fn wait_for_force_seal_retirement(
    harness: &BifrostHarness,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for scribe in harness.scribes() {
            scribe.check_age(Instant::now() + Duration::from_secs(120));
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        let remaining = harness
            .scribes()
            .iter()
            .map(|scribe| {
                scribe
                    .memtable_stats()
                    .map(|stats| stats.immutable_generations)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum::<usize>();
        if remaining == 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("force-seal retained {remaining} generations").into());
        }
    }
}

fn make_batch(pod_index: usize, tenant_index: usize) -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]));
    let timestamp = chrono::Utc::now().timestamp_micros();
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(
                (0..ROWS_PER_APPEND)
                    .map(|row| {
                        i64::try_from(pod_index * 100 + tenant_index * 10 + row).unwrap_or(0)
                    })
                    .collect::<Vec<_>>(),
            )),
            Arc::new(
                TimestampMicrosecondArray::from(vec![timestamp; ROWS_PER_APPEND])
                    .with_timezone("UTC"),
            ),
        ],
    )
    .expect("valid journey batch")
}
