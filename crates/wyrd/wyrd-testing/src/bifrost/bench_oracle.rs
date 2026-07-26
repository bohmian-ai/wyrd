//! Real Oracle hot-tail benchmark execution over the shared Bifrost harness.

use std::sync::Arc;
use std::time::Instant;

use crate::bifrost::BifrostHarness;
use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::contracts::{Scribe, ScribeAppend};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::seal_key::EventDay;
use vala_bifrost_redux::scribe::tail_rpc::FetchLiveTailRequest;
use vala_bifrost_redux::scribe::wal::WalLsn;
use wyrd_bench::{BifrostLane, BifrostScenario, NegativeFlowReport};
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;

type BenchError = Box<dyn std::error::Error + Send + Sync>;

/// Run one typed Oracle hot-tail scenario.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    if scenario.lane != BifrostLane::Oracle {
        return Err("Oracle adapter received a non-Oracle scenario".into());
    }
    let harness = BifrostHarness::start(
        usize::try_from(scenario.pods)?,
        usize::try_from(scenario.tenants)?,
    )
    .await?;
    let started = Instant::now();
    let tenant = *harness.tenants().first().ok_or("Oracle needs one tenant")?;
    let scribe = harness.scribes().first().ok_or("Oracle needs one Scribe")?;
    let table = TableRef::new(BifrostNamespace::Bifrost, "bifrost_bench_oracle");
    let day = chrono::Utc::now().date_naive();
    let rows = make_batch(day, usize::try_from(scenario.items_per_request)?, 1)?;
    let row_count = rows.num_rows();
    let fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
    let batch_id = uuid::Uuid::now_v7();
    let admission = scribe
        .append_durable(ScribeAppend {
            principal: Principal {
                id: PrincipalId::new(uuid::Uuid::now_v7()),
                kind: PrincipalKind::User,
                tenant_id: tenant,
                roles: Vec::new(),
                effective_permissions: PermissionSet::new(),
            },
            table: table.clone(),
            rows,
            schema_fingerprint: fingerprint,
            request_id: RequestId::now_v7(),
            batch_id,
            measured_wire_bytes: 0,
        })
        .await?;
    let tail = scribe.tail_service()?;
    let binding = TenantTableBinding::resolve((tenant, table.clone()))?;
    let hot = tail
        .fetch_hot_batches(FetchLiveTailRequest {
            binding: binding.clone(),
            target_stream: tail.stream(),
            start_day: EventDay::new(day),
            end_day: EventDay::new(day),
            after_lsn: WalLsn::ZERO,
            required_columns: vec!["value".to_owned()],
        })
        .await?;
    let returned_rows = hot.iter().map(|batch| batch.rows.num_rows()).sum::<usize>();
    let negative_flows = if scenario.require_negative_flows {
        let invalid = tail
            .fetch_hot_batches(FetchLiveTailRequest {
                binding: binding.clone(),
                target_stream: tail.stream(),
                start_day: EventDay::new(day),
                end_day: EventDay::new(day),
                after_lsn: WalLsn::ZERO,
                required_columns: vec!["missing_benchmark_column".to_owned()],
            })
            .await;
        NegativeFlowReport::executed(["invalid_oracle_projection_is_rejected"], invalid.is_err())
    } else {
        NegativeFlowReport::skipped()
    };
    let verified = admission.batch_id == batch_id
        && admission.rows_accepted == u64::try_from(row_count)?
        && !hot.is_empty()
        && returned_rows == row_count
        && negative_flows.passed;
    let mut envelope =
        wyrd_bench::BifrostReportEnvelope::new(scenario, super::bench_report::readiness(verified));
    envelope.negative_flows = negative_flows;
    let report = super::bench_report::LaneExecutionReport {
        envelope,
        elapsed_us: u64::try_from(started.elapsed().as_micros())?,
        verified,
        rows: u64::try_from(returned_rows)?,
    };
    super::bench_report::emit_report(&report, "oracle")?;
    harness.shutdown().await?;
    if !verified {
        return Err("Oracle benchmark verification failed".into());
    }
    Ok(())
}

fn make_batch(
    day: chrono::NaiveDate,
    rows: usize,
    sequence: i64,
) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".to_owned().into())),
            false,
        ),
    ]));
    let now = day
        .and_hms_opt(12, 0, 0)
        .expect("valid benchmark day")
        .and_utc()
        .timestamp_micros();
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(
                (0..rows)
                    .map(|index| sequence.saturating_add(i64::try_from(index).unwrap_or(i64::MAX)))
                    .collect::<Vec<_>>(),
            )),
            Arc::new(TimestampMicrosecondArray::from(vec![now; rows]).with_timezone("UTC")),
        ],
    )
}
