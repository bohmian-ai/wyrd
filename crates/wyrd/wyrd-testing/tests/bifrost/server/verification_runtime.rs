//! Verification runtime journey on a role-separated cluster.
//!
//! The runner lives on an Oracle-only node with no local Scribe, so the only
//! way its result can reach Bifrost is the tenant SYSTEM token over gRPC to
//! the Scribe node's public ingest endpoint. The journey schedules one
//! binding-created run, lets the runtime execute and publish a non-empty Drift
//! result, and reads the summary and its feature rows back through the
//! server's own query entry, asserting every identity they carry. A second
//! journey runs the production Drift engine on the Scribe-only node, which
//! hosts no Oracle, so its observation read must be forwarded to a peer
//! Oracle under the tenant's SYSTEM Drift reader and audited there. A third
//! journey proves every built-in typed payload — verification summaries, Eval
//! items, gateway payloads, agent traces, and audit detail — reads back as the
//! producer's native JSON. A fourth journey sends raw Arrow IPC with malformed
//! Variant columns to built-ins and proves each is refused before any ACK.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{
    Array, ArrayRef, AsArray, BinaryArray, FixedSizeBinaryArray, Float64Array, Int32Array,
    ListArray, StringArray, StructArray, TimestampMicrosecondArray,
};
use arrow::compute::cast;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::json::writer::{JsonArray, WriterBuilder};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Datelike as _, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use url::Url;
use vala_bifrost_redux::oracle::AuthorizedQueryContext;
use vala_bifrost_redux::tables::{AgentTracesTable, DomainTable};
use vala_eval::executor::{EvalReport, SkipReason, TaskRunOutcome};
use vala_sql::queries::audit_staging::{append_audit, entry_hash};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::Bifrost;
use wyrd_client::bifrost::TableConfig;
use wyrd_queue::variant::{
    EncodedVariant, VariantColumnBuilder, VariantJsonEncoderFactory, variant_field,
    variant_storage_type,
};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::engines::{EngineOutcome, VerifierReport};
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::runner::EngineScript;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{GatewayAccess, PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, CardUid, VerificationResultId, VerificationRunId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, AuthMethod, BifrostQueryRequest, VARIANT_MAX_DEPTH,
    VARIANT_MAX_ENCODED_BYTES,
};
use wyrd_spec::vala::eval::TaskId;
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::result::AssertionResult;
use wyrd_spec::vala::managed_columns::{
    CARD_REF, CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
};
use wyrd_spec::vala::{AuditDetail, BifrostError, audit_detail_canonical_json};
use wyrd_spec::verification::{DriftWindow, VerificationVerdict};
use wyrd_sql::queries::verifier_runs::RunInput;
use wyrd_testing::bifrost::{BifrostClusterSpec, RawIngest, WyrdTestCluster, canonical_signals};
use wyrd_testing::verification::VerificationFixture;
use wyrd_testing::{Bootstrap, WyrdTestServer};

use super::query::{ServerJourneyError, audit_rows, scheduled_context};

/// Upper bound on the wait for the remote runner to settle the run.
const WAIT: Duration = Duration::from_secs(60);

/// One day, the receipt-clock shift that moves an ACK onto the next UTC day.
const DAY: Duration = Duration::from_secs(86_400);

/// A runner on a node without local Scribe publishes a binding-created Drift
/// result through the configured ingest endpoint and completes the run. Its
/// summary and both feature rows are queryable through the Oracle and carry
/// the exact tenant SYSTEM writer, Verifier, subject, owner, binding, run,
/// result, and one shared event time. Without an endpoint the same node
/// composes no runner.
///
/// # Errors
/// Returns cluster, seeding, runtime, publication, or query errors, or a
/// description of the first identity that does not match.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn runner_without_local_scribe_publishes_through_the_ingest_endpoint()
-> Result<(), ServerJourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated()).await?;
    let tenant = cluster.data_tenant_id();
    let oracle = cluster.server(0).ok_or("missing Oracle node")?;
    let scribe = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("the cluster composed no Scribe")?;
    if oracle.state().bifrost_ingest().is_some() {
        return Err("the runner node must have no local ingest".into());
    }

    let unrouted = VerificationRuntime::builder(oracle.state())
        .build()
        .ok_or("the runtime did not compose")?;
    if unrouted.composes(RuntimeCapability::Runner) {
        return Err("a node without Scribe or an endpoint composed a runner".into());
    }

    let seed = VerificationFixture::provision(oracle.state().postgres.wyrd(), tenant).await?;
    let (subject, _) = seed.service("remote-subject").await?;
    let (owner, owner_principal) = seed.service("remote-owner").await?;
    let verifier = seed.drift_verifier("remote-drift").await?;
    let binding = seed
        .bind_schedule(&owner, &subject, &verifier, "0 * * * *", Vec::new())
        .await?;
    // Activation arms the cursor at the next hour's boundary; the occurrence
    // is brought due in the database, which owns the schedule clock.
    seed.activate(owner_principal).await?;
    seed.make_binding_due(binding).await?;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(Some(
        serde_json::from_value(json!({
            "method": "Custom",
            "features": {
                "latency": { "feature": "latency", "score": 3.0, "threshold": 1.0, "verdict": "Drift" },
                "tokens": { "feature": "tokens", "score": 0.1, "threshold": 1.0, "verdict": "NoDrift" }
            },
            "verdict": "Drift"
        }))?,
    ))));
    let runtime = VerificationRuntime::builder(oracle.state())
        .ingest_endpoint(scribe.grpc_url().ok_or("missing Scribe gRPC URL")?)
        .engine_script(script)
        .build()
        .ok_or("the runtime did not compose")?;
    if !runtime.composes(RuntimeCapability::Runner) {
        return Err("an explicit ingest endpoint did not compose the runner".into());
    }
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));

    let deadline = tokio::time::Instant::now() + WAIT;
    let (run, row) = loop {
        if let Some(run) = seed.runs().await?.first().copied() {
            let row = seed.run(run).await?;
            if row.status != "pending" && row.status != "running" {
                break (run, row);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("the scheduled run never settled on the remote runner".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    let result = match (row.status.as_str(), row.result_id) {
        ("completed", Some(result)) => result,
        _ => return Err(format!("the run settled {row:?}").into()),
    };

    scribe.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let system = seed.system_principal();
    let identity = |alias: &str| {
        format!(
            "{alias}{RUN_ID} = '{run}' AND {alias}result_id = '{result}' \
             AND {alias}{PRINCIPAL_ID} = '{system}' AND {alias}{CARD_UID} = '{verifier}' \
             AND {alias}subject_card_uid = '{subject}' AND {alias}owner_card_uid = '{owner}' \
             AND {alias}binding_id = '{binding}'"
        )
    };
    let checks = [
        (
            format!("SELECT result_id FROM vala.verification.results WHERE {RUN_ID} = '{run}'"),
            1,
            "one published summary for the run",
        ),
        (
            format!(
                "SELECT result_id FROM vala.verification.results WHERE {}",
                identity("")
            ),
            1,
            "the summary carries every exact identity",
        ),
        (
            format!("SELECT result_id FROM vala.drift.result_features WHERE {RUN_ID} = '{run}'"),
            2,
            "two published feature rows for the run",
        ),
        (
            format!(
                "SELECT f.result_id FROM vala.drift.result_features f \
                 JOIN vala.verification.results r \
                   ON f.result_id = r.result_id AND f.{RUN_ID} = r.{RUN_ID} \
                  AND f.{WYRD_EVENT_TIME} = r.{WYRD_EVENT_TIME} \
                  AND f.{PRINCIPAL_ID} = r.{PRINCIPAL_ID} AND f.{CARD_UID} = r.{CARD_UID} \
                 WHERE {}",
                identity("f.")
            ),
            2,
            "every feature joins its summary on result, run, writer, Verifier, and event time",
        ),
    ];
    for (sql, expected, what) in checks {
        let rows = query_rows(oracle, tenant, sql).await?;
        if rows != expected {
            return Err(format!("{what}: expected {expected} rows, read {rows}").into());
        }
    }
    Ok(())
}

/// A runner on a pod without a local Oracle completes a real Drift run.
///
/// The Scribe-only node composes the runtime with its own ingest endpoint.
/// Its Custom Verifier's observation read is minted as the tenant SYSTEM
/// Drift reader and forwarded by Gate to a peer Oracle, which records one
/// audited read decision; the run completes with a published result. The
/// registered table is empty for the subject, so the run is inconclusive.
///
/// # Errors
/// Returns cluster, seeding, runtime, or audit errors, or a description of
/// the first expectation that does not hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn drift_runner_without_local_oracle_reads_through_a_peer() -> Result<(), ServerJourneyError>
{
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated()).await?;
    let tenant = cluster.data_tenant_id();
    let scribe = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("the cluster composed no Scribe")?;
    if scribe.state().bifrost.oracle().is_some() {
        return Err("the runner node must host no Oracle".into());
    }

    let seed = VerificationFixture::provision(scribe.state().postgres.wyrd(), tenant).await?;
    let (subject, _) = seed.service("forwarded-subject").await?;
    let verifier = seed
        .custom_drift_verifier("forwarded-drift", "score", 1.0, 0.5)
        .await?;
    scribe
        .ensure_builtin_table_for_test(tenant, "drift", "observations")
        .await?;
    let now = chrono::Utc::now();
    let run = seed
        .enqueue_direct(
            &verifier,
            &subject,
            wyrd_spec::verification::DriftWindow {
                start: now - chrono::Duration::hours(1),
                end: now,
            },
        )
        .await?;
    let reads = audit_rows(scribe, tenant, "bifrost.query.read_decision").await?;

    let runtime = VerificationRuntime::builder(scribe.state())
        .ingest_endpoint(scribe.grpc_url().ok_or("missing Scribe gRPC URL")?)
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + WAIT;
    let row = loop {
        let row = seed.run(run).await?;
        if row.status != "pending" && row.status != "running" && row.status != "retrying" {
            break row;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the Drift run never settled: {row:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    if row.status != "completed" || row.result_id.is_none() || row.attempts != 1 {
        return Err(format!("the forwarded Drift run settled {row:?}").into());
    }
    let after = audit_rows(scribe, tenant, "bifrost.query.read_decision").await?;
    if after != reads + 1 {
        return Err(format!("expected one audited peer read, counted {}", after - reads).into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// A monthly schedule whose current window opens at the start of the month.
///
/// The window closes at the Postgres instant the binding is made due, so an
/// observation whose event time is the start of the month or a minute in the
/// past falls inside the window the run analyzes; see
/// [`observed_in_current_month`].
const MONTHLY: &str = "0 0 1 * *";

/// The caller event time of the shared observation: one minute ago, or the
/// start of the current UTC month when that is later.
///
/// The window end is Postgres' `statement_timestamp()`, while a
/// server-stamped receipt reads the server's own clock. Whenever the database
/// clock trails the server's, a receipt can land after a window that closed
/// later in real time. Supplying the event time removes that cross-clock
/// comparison: the observation is inside `[start of month, due)` by
/// construction rather than by the race between the write's acknowledgement
/// and the due update.
///
/// # Errors
/// Returns a description when the month start cannot be represented.
fn observed_in_current_month(now: DateTime<Utc>) -> Result<DateTime<Utc>, ServerJourneyError> {
    let month_start = now
        .date_naive()
        .with_day(1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .ok_or("the current month has no first instant")?
        .and_utc();
    Ok(month_start.max(now - chrono::Duration::minutes(1)))
}

/// Two schedule bindings of one subject share one raw client observation and
/// stay independently filterable through their runs and results.
///
/// A Card-bound client Service authenticates with its own API key and writes
/// one Drift observation of itself (two tall feature rows) through the public
/// Bifrost facade. Two Custom Drift Verifiers, one that drifts on the
/// observed mean and one that does not, are bound to that subject. The
/// production runtime on the Oracle-only node executes both runs and
/// publishes through the Scribe endpoint. The observation table then holds
/// exactly the two client rows, attributed to the client principal and the
/// subject Card with no Verifier or binding column and no per-binding copy,
/// while each binding's single result is selected by its `binding_id` alone
/// and carries its own Verifier, run, and verdict.
///
/// # Errors
/// Returns cluster, seeding, client, runtime, or query errors, or a
/// description of the first expectation that does not hold.
///
/// # Panics
///
/// Panics only if `#[tokio::test]` cannot build its runtime; every
/// expectation failure is returned as an error instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn two_bindings_share_one_client_observation() -> Result<(), ServerJourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::role_separated()).await?;
    let tenant = cluster.data_tenant_id();
    let oracle = cluster.server(0).ok_or("missing Oracle node")?;
    let scribe = cluster
        .servers()
        .find(|server| server.bifrost_scribe().is_some())
        .ok_or("the cluster composed no Scribe")?;
    let seed = VerificationFixture::provision(oracle.state().postgres.wyrd(), tenant).await?;
    let Bootstrap::Machine {
        id: client,
        api_key,
        card_ref: subject_ref,
    } = scribe
        .bootstrap_service_in_tenant(tenant, "shared-subject", &["admin"])
        .await?
    else {
        return Err("a service bootstrap returned a user".into());
    };
    let subject = subject_ref
        .uid
        .clone()
        .ok_or("the subject Card has no UID")?;
    let drifting = seed
        .custom_drift_verifier("shared-drifting", "score", 1.0, 0.5)
        .await?;
    let steady = seed
        .custom_drift_verifier("shared-steady", "score", 1.0, 100.0)
        .await?;
    let mut bindings = Vec::new();
    for verifier in [&drifting, &steady] {
        let binding = seed
            .bind_schedule(&subject, &subject, verifier, MONTHLY, Vec::new())
            .await?;
        bindings.push((verifier.clone(), binding));
    }
    scribe
        .ensure_builtin_table_for_test(tenant, "drift", "observations")
        .await?;

    let record = format!("shared-{}", uuid::Uuid::now_v7());
    let writer = wyrd_client::bifrost::client_from_options(
        scribe.base_url(),
        Some(api_key.expose_secret()),
        scribe.grpc_url().as_deref(),
    )?;
    let uidless = CardRef {
        uid: None,
        ..subject_ref.clone()
    };
    Bifrost::connect(&writer)
        .await?
        .write_batch(
            "vala.drift.observations",
            &observation_batch(
                &record,
                &uidless.to_string(),
                observed_in_current_month(Utc::now())?,
            )?,
        )
        .await?;
    seed.activate(client).await?;
    for (_, binding) in &bindings {
        seed.make_binding_due(*binding).await?;
    }
    scribe.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let runtime = VerificationRuntime::builder(oracle.state())
        .ingest_endpoint(scribe.grpc_url().ok_or("missing Scribe gRPC URL")?)
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let runs = seed.runs().await?;
        let mut settled = 0;
        for run in &runs {
            let row = seed.run(*run).await?;
            if !matches!(row.status.as_str(), "pending" | "running" | "retrying") {
                if row.status != "completed" || row.result_id.is_none() {
                    return Err(format!("a binding run settled {row:?}").into());
                }
                settled += 1;
            }
        }
        if runs.len() == 2 && settled == 2 {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("expected two settled binding runs, saw {runs:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    scribe.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let observed = query_texts(
        oracle,
        tenant,
        format!(
            "SELECT {PRINCIPAL_ID}, {CARD_UID}, series FROM vala.drift.observations \
             WHERE record_id = '{record}' ORDER BY series"
        ),
    )
    .await?;
    let attributed = |series: &str| {
        vec![
            Some(client.to_string()),
            Some(subject.to_string()),
            Some(series.to_owned()),
        ]
    };
    if observed != vec![attributed("latency"), attributed("score")] {
        return Err(format!(
            "the observation must be exactly the client's two rows of the subject: {observed:?}"
        )
        .into());
    }
    for column in ["binding_id", "verifier_uid", "result_id"] {
        if query_texts(
            oracle,
            tenant,
            format!("SELECT {column} FROM vala.drift.observations"),
        )
        .await
        .is_ok()
        {
            return Err(format!("raw observations must carry no {column} column").into());
        }
    }

    let mut verdicts = Vec::new();
    for (verifier, binding) in &bindings {
        let rows = query_texts(
            oracle,
            tenant,
            format!(
                "SELECT {CARD_UID}, subject_card_uid, verdict, {RUN_ID} \
                 FROM vala.verification.results WHERE binding_id = '{binding}'"
            ),
        )
        .await?;
        let [row] = rows.as_slice() else {
            return Err(format!("binding {binding} must select one result, saw {rows:?}").into());
        };
        if row[0].as_deref() != Some(verifier.as_str())
            || row[1].as_deref() != Some(subject.as_str())
        {
            return Err(format!("binding {binding} selected a foreign result: {row:?}").into());
        }
        verdicts.push(row[2].clone());
    }
    if verdicts[0] == verdicts[1] {
        return Err(format!(
            "the drifting and steady bindings must reach their own verdicts over the \
             one shared observation: {verdicts:?}"
        )
        .into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// One Drift observation `record` of `card_ref` at `event_time`: a drifting
/// `score` and a `latency` feature, as the SDK's tall projection writes them,
/// with a caller `wyrd_event_time` that Scribe keeps verbatim.
///
/// # Errors
/// Returns an Arrow error when the batch cannot be assembled.
fn observation_batch(
    record: &str,
    card_ref: &str,
    event_time: DateTime<Utc>,
) -> Result<RecordBatch, ServerJourneyError> {
    let utc = || DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()));
    let schema = Arc::new(Schema::new(vec![
        Field::new("record_id", DataType::Utf8, false),
        Field::new("series", DataType::Utf8, false),
        Field::new("num_value", DataType::Float64, true),
        Field::new("str_value", DataType::Utf8, true),
        Field::new("session_id", DataType::Utf8, true),
        Field::new("created_at", utc(), false),
        Field::new(CARD_REF, DataType::Utf8, true),
        Field::new(WYRD_EVENT_TIME, utc(), false),
    ]));
    let now = Utc::now().timestamp_micros();
    let event = event_time.timestamp_micros();
    Ok(RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![record, record])),
            Arc::new(StringArray::from(vec!["score", "latency"])),
            Arc::new(Float64Array::from(vec![3.0, 120.0])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
            Arc::new(TimestampMicrosecondArray::from(vec![now, now]).with_timezone("UTC")),
            Arc::new(StringArray::from(vec![card_ref, card_ref])),
            Arc::new(TimestampMicrosecondArray::from(vec![event, event]).with_timezone("UTC")),
        ],
    )?)
}

/// The analytical layout of verification results across UTC days.
///
/// The tenant SYSTEM writer publishes three results through the public
/// Bifrost facade, flushing after each so every result is its own file: a
/// Drift result whose feature rows are acknowledged on one receipt day and
/// whose summary is acknowledged on the next, a Drift result whose event time
/// is the previous UTC day, and an Eval result. Each result's details are
/// retrieved by `result_id` alone from both daily partitions; the straddling
/// result's summary and details share one `wyrd_event_time` though their
/// `wyrd_ingested_at` fall on different days. Every published Parquet file of
/// the three tables carries a physical Bloom filter for its declared and
/// managed-floor columns, and Oracle's scan metrics show a day-range
/// predicate dropping the previous day's file and a `result_id` lookup
/// selecting strictly fewer files or row groups than a full scan.
///
/// # Errors
/// Returns cluster, seeding, minting, publication, telemetry, or query
/// errors, or a description of the first expectation that does not hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn result_layout_partitions_blooms_and_prunes_by_result() -> Result<(), ServerJourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let journey = ResultLayoutJourney::start(&cluster).await?;
    let now = Utc::now();
    let yesterday = now - chrono::Duration::days(1);
    let straddling = journey.publish(now, Report::Drift, true).await?;
    let previous_day = journey.publish(yesterday, Report::Drift, false).await?;
    let eval = journey.publish(now, Report::Eval, false).await?;
    cluster.refresh_oracle_snapshots().await?;

    for (result, expected) in [(&straddling, 2), (&previous_day, 2)] {
        let features = journey
            .texts(format!(
                "SELECT feature FROM vala.drift.result_features WHERE result_id = '{result}'"
            ))
            .await?;
        if features.len() != expected {
            return Err(format!(
                "result {result} must retrieve its {expected} feature rows by result_id, \
                 saw {features:?}"
            )
            .into());
        }
    }
    let items = journey
        .texts(format!(
            "SELECT task_id FROM vala.eval.result_items WHERE result_id = '{eval}'"
        ))
        .await?;
    if items != vec![vec![Some("gate".to_owned())]] {
        return Err(format!("the Eval result retrieves its one item: {items:?}").into());
    }

    let times = |table: &str| {
        format!(
            "SELECT CAST({WYRD_EVENT_TIME} AS BIGINT), CAST({WYRD_INGESTED_AT} AS BIGINT) \
             FROM {table} WHERE result_id = '{straddling}'"
        )
    };
    let summary = journey.texts(times("vala.verification.results")).await?;
    let details = journey.texts(times("vala.drift.result_features")).await?;
    let micros = |value: &Option<String>| -> Result<i64, ServerJourneyError> {
        Ok(value.as_deref().ok_or("null timestamp")?.parse()?)
    };
    let day = |micros: i64| micros.div_euclid(86_400_000_000);
    let [summary] = summary.as_slice() else {
        return Err(format!("one straddling summary expected, saw {summary:?}").into());
    };
    for detail in &details {
        if micros(&detail[0])? != micros(&summary[0])? {
            return Err(format!(
                "result and details must share one event time: {summary:?} {details:?}"
            )
            .into());
        }
        if day(micros(&detail[1])?) == day(micros(&summary[1])?) {
            return Err(format!(
                "the details and summary ACKs must fall on different receipt days: \
                 {summary:?} {details:?}"
            )
            .into());
        }
    }

    journey.assert_blooms()?;

    let unfiltered = journey
        .scan(
            "SELECT result_id FROM vala.verification.results".to_owned(),
            3,
        )
        .await?;
    let today = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .ok_or("midnight exists")?
        .and_utc()
        .to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    let current_day = journey
        .scan(
            format!(
                "SELECT result_id FROM vala.verification.results \
                 WHERE {WYRD_EVENT_TIME} >= TIMESTAMP '{today}'"
            ),
            2,
        )
        .await?;
    if current_day.files >= unfiltered.files {
        return Err(format!(
            "a day-range predicate must prune the previous day's partition: \
             current-day {current_day:?} unfiltered {unfiltered:?}"
        )
        .into());
    }
    let by_result = journey
        .scan(
            format!(
                "SELECT result_id FROM vala.verification.results WHERE result_id = '{straddling}'"
            ),
            1,
        )
        .await?;
    if !(by_result.files < unfiltered.files || by_result.row_groups < unfiltered.row_groups) {
        return Err(format!(
            "a result_id lookup must select strictly fewer files or row groups: \
             by-result {by_result:?} unfiltered {unfiltered:?}"
        )
        .into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Which engine report a published layout result maps.
#[derive(Debug, Clone, Copy)]
enum Report {
    /// A two-feature Drift report over a Drift window.
    Drift,
    /// A one-item Eval report over one committed record.
    Eval,
}

/// Oracle scan evidence for one query: files and row groups scanned.
#[derive(Debug, Clone, Copy)]
struct ScanEvidence {
    /// `oracle_query_files_scanned_total` delta.
    files: f64,
    /// `oracle_query_row_groups_scanned_total` delta.
    row_groups: f64,
}

/// The single-node cluster, tenant, and SYSTEM writer the layout journey
/// publishes and reads through.
///
/// Owns the identities every published result repeats — the subject and the
/// SYSTEM token's Verifier — and the server whose Scribe the writer reaches,
/// so each publication, query, and storage inspection reads them from one
/// place.
struct ResultLayoutJourney<'a> {
    /// The cluster whose telemetry and storage root the journey inspects.
    cluster: &'a WyrdTestCluster,
    /// The mixed node hosting both Scribe and Oracle.
    server: &'a WyrdTestServer,
    /// Tenant every result belongs to.
    tenant: DataTenantId,
    /// The verified subject.
    subject: CardUid,
    /// The UID-bearing Verifier the SYSTEM token is scoped to.
    verifier: CardRef,
    /// Public Bifrost facade authenticated as the tenant SYSTEM writer.
    bifrost: Bifrost,
}

impl<'a> ResultLayoutJourney<'a> {
    /// Provision the tenant's SYSTEM writer, a subject and a Verifier, the
    /// three result tables, and a SYSTEM-authenticated Bifrost facade.
    ///
    /// # Errors
    /// Returns a seeding, provisioning, minting, or connection error.
    async fn start(cluster: &'a WyrdTestCluster) -> Result<Self, ServerJourneyError> {
        let tenant = cluster.data_tenant_id();
        let server = cluster.server(0).ok_or("missing mixed node")?;
        let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
        let (subject, _) = seed.service("layout-subject").await?;
        let uid = seed.drift_verifier("layout-drift").await?;
        let verifier: CardRef = format!("default/Verifier/layout-drift@1.0.0#{uid}").parse()?;
        for (namespace, name) in [
            ("verification", "results"),
            ("drift", "result_features"),
            ("eval", "result_items"),
        ] {
            server
                .ensure_builtin_table_for_test(tenant, namespace, name)
                .await?;
        }
        let issuer = server
            .state()
            .auth
            .tenant_issuer()
            .ok_or("the server has no tenant issuer")?;
        let mut conn = server.state().postgres.wyrd().tenant_conn(tenant).await?;
        let token = issuer.issue_system_token(&mut conn, &verifier).await?;
        drop(conn);
        let client = wyrd_client::bifrost::client_from_options(
            server.base_url(),
            Some(token.access_token.expose_secret()),
            server.grpc_url().as_deref(),
        )?;
        Ok(Self {
            cluster,
            server,
            tenant,
            subject,
            verifier,
            bifrost: Bifrost::connect(&client).await?,
        })
    }

    /// Publish one result with `event_time` through the production payload
    /// builder and flush it into its own file; returns its `result_id`.
    ///
    /// With `straddle`, Scribe's receipt clock moves one day ahead after the
    /// detail batch is acknowledged and before the summary is sent, so the
    /// two ACKs fall on different UTC receipt days; the shift is cleared
    /// before returning.
    ///
    /// # Errors
    /// Returns a payload, write, or flush error.
    async fn publish(
        &self,
        event_time: DateTime<Utc>,
        report: Report,
        straddle: bool,
    ) -> Result<VerificationResultId, ServerJourneyError> {
        let (input, report) = match report {
            Report::Drift => (
                RunInput::DriftWindow(DriftWindow {
                    start: event_time - chrono::Duration::hours(1),
                    end: event_time,
                }),
                VerifierReport::Drift(Some(serde_json::from_value(json!({
                    "method": "Custom",
                    "features": {
                        "latency": { "feature": "latency", "score": 3.0, "threshold": 1.0, "verdict": "Drift" },
                        "tokens": { "feature": "tokens", "score": 0.1, "threshold": 1.0, "verdict": "NoDrift" }
                    },
                    "verdict": "Drift"
                }))?)),
            ),
            Report::Eval => (
                RunInput::EvalRecord {
                    record_id: format!("layout-{}", uuid::Uuid::now_v7()),
                    event_time,
                },
                VerifierReport::Eval {
                    report: EvalReport {
                        outcomes: vec![TaskRunOutcome::Skipped {
                            task_id: TaskId::new("gate")?,
                            reason: SkipReason::ConditionFalse,
                        }],
                    },
                    verdict: VerificationVerdict::Inconclusive,
                },
            ),
        };
        let result = VerificationResultId::new_v7();
        let verifier_ref = CardRef {
            uid: None,
            ..self.verifier.clone()
        }
        .to_string();
        let payload = ResultPayloadBuilder::new(
            ResultRun {
                run_id: VerificationRunId::new_v7(),
                verifier_version: "1.0.0",
                subject_card_uid: &self.subject,
                owner_card_uid: Some(&self.subject),
                binding_id: Some(BindingId::new_v7()),
                trigger: None,
                input: &input,
            },
            &verifier_ref,
            result,
            event_time,
            event_time,
            event_time,
        )
        .build(&report)?;
        let scribe = self
            .server
            .bifrost_scribe()
            .ok_or("the mixed node owns no Scribe")?;
        let batches = payload.batches();
        for (index, batch) in batches.iter().enumerate() {
            if straddle && index + 1 == batches.len() {
                scribe.shift_receipt_clock_for_test(DAY);
            }
            self.bifrost.write_batch(&batch.table, &batch.batch).await?;
        }
        scribe.shift_receipt_clock_for_test(Duration::ZERO);
        self.server.flush_bifrost().await?;
        Ok(result)
    }

    /// Every row of `sql` over published data, each column cast to text.
    ///
    /// # Errors
    /// Returns the context, query, or cast error.
    async fn texts(&self, sql: String) -> Result<Vec<Vec<Option<String>>>, ServerJourneyError> {
        query_texts(self.server, self.tenant, sql).await
    }

    /// Run `sql`, require `rows` result rows, and return the Oracle scan
    /// evidence the query alone produced.
    ///
    /// # Errors
    /// Returns the telemetry or query error, or a row-count mismatch.
    async fn scan(&self, sql: String, rows: usize) -> Result<ScanEvidence, ServerJourneyError> {
        let telemetry = self.cluster.telemetry();
        let checkpoint = telemetry.checkpoint()?;
        let read = self.texts(sql.clone()).await?;
        if read.len() != rows {
            return Err(format!("{sql} expected {rows} rows, read {read:?}").into());
        }
        let delta = telemetry.delta_since(&checkpoint)?;
        let sum = |family: &str| -> f64 {
            delta
                .metrics
                .iter()
                .filter(|sample| sample.family == family)
                .map(|sample| sample.value)
                .sum()
        };
        Ok(ScanEvidence {
            files: sum("oracle_query_files_scanned_total"),
            row_groups: sum("oracle_query_row_groups_scanned_total"),
        })
    }

    /// Require a physical Bloom filter on every declared and managed-floor
    /// Bloom column in every published file of the three result tables.
    ///
    /// Files are found under the cluster's storage root and classified by a
    /// column only their table carries; each table must have at least one.
    ///
    /// # Errors
    /// Returns a filesystem or Parquet error, or names the first table with
    /// no file or the first column chunk without a Bloom filter.
    fn assert_blooms(&self) -> Result<(), ServerJourneyError> {
        let floor = [RUN_ID, CARD_UID, PRINCIPAL_ID];
        let tables: [(&str, &str, &[&str]); 3] = [
            (
                "vala.verification.results",
                "execution_status",
                &["result_id", "subject_card_uid", "binding_id"],
            ),
            ("vala.drift.result_features", "feature", &["result_id"]),
            ("vala.eval.result_items", "task_id", &["result_id"]),
        ];
        let mut found = [0_usize; 3];
        let mut pending = vec![self.cluster.storage_root().to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory)? {
                let path = entry?.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path
                    .extension()
                    .is_none_or(|extension| extension != "parquet")
                {
                    continue;
                }
                let reader = SerializedFileReader::new(std::fs::File::open(&path)?)?;
                let metadata = reader.metadata();
                let columns = metadata.file_metadata().schema_descr().columns().to_vec();
                let has = |name: &str| columns.iter().any(|column| column.name() == name);
                let Some(index) = tables.iter().position(|(_, marker, _)| has(marker)) else {
                    continue;
                };
                found[index] += 1;
                let (table, _, declared) = tables[index];
                for group in metadata.row_groups() {
                    for name in declared.iter().chain(&floor) {
                        let chunk = group
                            .columns()
                            .iter()
                            .find(|chunk| chunk.column_descr().name() == *name)
                            .ok_or_else(|| format!("{table} file lacks column {name}"))?;
                        if chunk.bloom_filter_offset().is_none() {
                            return Err(format!(
                                "{table} file {} has no Bloom filter on {name}",
                                path.display()
                            )
                            .into());
                        }
                    }
                }
            }
        }
        for ((table, _, _), count) in tables.iter().zip(found) {
            if count == 0 {
                return Err(format!("no published Parquet file found for {table}").into());
            }
        }
        Ok(())
    }
}

/// Run `sql` through `server`'s scheduled query entry for `tenant` against
/// published data and return every row with each column cast to text.
///
/// # Errors
/// Returns the context, query, or cast error.
async fn query_texts(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    sql: String,
) -> Result<Vec<Vec<Option<String>>>, ServerJourneyError> {
    let mut batches = Vec::new();
    ScheduledQueryCaller::new(
        server.state().clone(),
        scheduled_context(tenant)?,
        CancellationToken::new(),
    )
    .run_with(
        BifrostQueryRequest {
            sql,
            deadline_ms: Some(30_000),
        },
        |batch| {
            batches.push(batch);
            Ok(())
        },
    )
    .await?;
    let mut rows = Vec::new();
    for batch in &batches {
        let columns = batch
            .columns()
            .iter()
            .map(|column| arrow::compute::cast(column, &DataType::Utf8))
            .collect::<Result<Vec<_>, _>>()?;
        for row in 0..batch.num_rows() {
            let mut values = Vec::with_capacity(columns.len());
            for column in &columns {
                let text = column
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or("a cast column is not text")?;
                values.push(text.is_valid(row).then(|| text.value(row).to_owned()));
            }
            rows.push(values);
        }
    }
    Ok(rows)
}

/// Run `sql` through `oracle`'s scheduled query entry for `tenant` against
/// published data and return the row count.
///
/// # Errors
/// Returns the context or query error.
async fn query_rows(
    oracle: &WyrdTestServer,
    tenant: DataTenantId,
    sql: String,
) -> Result<u64, ServerJourneyError> {
    let outcome = ScheduledQueryCaller::new(
        oracle.state().clone(),
        scheduled_context(tenant)?,
        CancellationToken::new(),
    )
    .run(BifrostQueryRequest {
        sql,
        deadline_ms: Some(30_000),
    })
    .await?;
    Ok(outcome.rows)
}

/// Built-in payloads keep their JSON types end to end through the real server.
///
/// One bound server takes every built-in producer whose payload is a typed
/// column: the production result builder publishes a scored Drift, an
/// unscored Drift, and an Eval result as the tenant SYSTEM writer; a real
/// gateway call is captured with its request and response payloads; an
/// agent trace is written through the public Bifrost facade; and an audit
/// decision with a structured detail is appended to staging and moved into
/// retained history by the server's own publisher. After a flush, whole
/// columns are read back through the server's query entry and every Variant
/// cell decodes to the producer's native JSON value. A scored Drift carries
/// only `drift_report`, an Eval only `eval_summary`, and an unscored Drift
/// neither. Every child of both summary Structs is projected with
/// `get_field` from the hot rows before the flush and the published rows
/// after it: an absent summary's children all read SQL null, including
/// `to_json` of its Variant, and a present summary's children read its
/// values. A second gateway call the upstream refuses is captured with no
/// resolved model: hot and published, both its `resolved_model` children
/// read SQL null while the resolved call's read its provider and model. The
/// retained audit row's `entry_hash` is recomputed from the stored columns,
/// with the decoded detail re-canonicalized, and matches.
///
/// # Errors
/// Returns server, seeding, gateway, publication, or query errors, or a
/// description of the first value that does not match its native source.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn typed_builtin_payloads_are_queryable() -> Result<(), ServerJourneyError> {
    let journey = TypedPayloadJourney::start().await?;

    let results = journey.publish_results().await?;
    let call = journey.capture_gateway_call().await?;
    let trace = journey.write_agent_trace().await?;
    let decision = journey.append_audit_decision().await?;
    let children_sql = format!(
        "SELECT result_id, drift_report['method'], drift_report['verdict'], \
         drift_report['features'], to_json(drift_report['features']) IS NULL, \
         eval_summary['total_tasks'], eval_summary['passed_tasks'], \
         eval_summary['failed_tasks'], eval_summary['pass_rate'], \
         eval_summary['duration_ms'] FROM vala.verification.results \
         WHERE result_id IN ('{}', '{}', '{}') ORDER BY result_id",
        results.scored_drift, results.unscored_drift, results.eval
    );
    let drift = |key: &str| results.drift_report[key].clone();
    let eval = |key: &str| results.eval_summary[key].clone();
    let mut expected_children = vec![
        vec![
            json!(results.scored_drift.to_string()),
            drift("method"),
            drift("verdict"),
            drift("features"),
            json!(false),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ],
        vec![
            json!(results.unscored_drift.to_string()),
            Value::Null,
            Value::Null,
            Value::Null,
            json!(true),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ],
        vec![
            json!(results.eval.to_string()),
            Value::Null,
            Value::Null,
            Value::Null,
            json!(true),
            eval("total_tasks"),
            eval("passed_tasks"),
            eval("failed_tasks"),
            eval("pass_rate"),
            eval("duration_ms"),
        ],
    ];
    expected_children.sort_by_key(|row| row[0].to_string());
    expect_eq(
        "hot summary Struct children",
        &journey.rows(children_sql.clone()).await?,
        &expected_children,
    )?;
    // Gateway capture: the resolved call carries both model Structs whole,
    // and the refused call's absent resolved model reads both children null.
    let models_sql = format!(
        "SELECT wyrd_request_id, requested_model['provider'], requested_model['model'], \
         resolved_model['provider'], resolved_model['model'] FROM vala.gateway.calls \
         WHERE wyrd_request_id IN ('{}', '{}') ORDER BY wyrd_request_id",
        call.request_id, call.unresolved_request_id
    );
    let mut expected_models = vec![
        vec![
            json!(call.request_id),
            json!("openai"),
            json!("gpt-4o"),
            json!("openai"),
            json!("gpt-4o"),
        ],
        vec![
            json!(call.unresolved_request_id),
            json!("openai"),
            json!("gpt-4o"),
            Value::Null,
            Value::Null,
        ],
    ];
    expected_models.sort_by_key(|row| row[0].to_string());
    // Capture lands after each caller has its answer, so wait for both rows.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let hot_models = loop {
        let rows = journey.rows(models_sql.clone()).await?;
        if rows.len() == expected_models.len() || tokio::time::Instant::now() >= deadline {
            break rows;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    expect_eq("hot gateway model children", &hot_models, &expected_models)?;
    journey.server.flush_bifrost().await?;
    journey.server.await_audit_published(journey.tenant).await?;
    expect_eq(
        "published summary Struct children",
        &journey.rows(children_sql).await?,
        &expected_children,
    )?;
    expect_eq(
        "published gateway model children",
        &journey.rows(models_sql).await?,
        &expected_models,
    )?;

    // Verification results: the two summary Structs are set exclusively.
    let summaries = journey
        .rows(format!(
            "SELECT result_id, drift_report, eval_summary FROM vala.verification.results \
             WHERE result_id IN ('{}', '{}', '{}')",
            results.scored_drift, results.unscored_drift, results.eval
        ))
        .await?;
    let summary = |result: &VerificationResultId| {
        summaries
            .iter()
            .find(|row| row[0] == json!(result.to_string()))
            .map(|row| (row[1].clone(), row[2].clone()))
            .ok_or_else(|| format!("no summary for {result}: {summaries:?}"))
    };
    expect_eq(
        "scored Drift summary",
        &summary(&results.scored_drift)?,
        &(results.drift_report.clone(), Value::Null),
    )?;
    expect_eq(
        "unscored Drift summary",
        &summary(&results.unscored_drift)?,
        &(Value::Null, Value::Null),
    )?;
    expect_eq(
        "Eval summary",
        &summary(&results.eval)?,
        &(Value::Null, results.eval_summary.clone()),
    )?;
    let children = journey
        .rows(format!(
            "SELECT drift_report['method'], drift_report['verdict'], drift_report['features'] \
             FROM vala.verification.results WHERE result_id = '{}'",
            results.scored_drift
        ))
        .await?;
    expect_eq(
        "drift_report children",
        &children,
        &vec![vec![
            results.drift_report["method"].clone(),
            results.drift_report["verdict"].clone(),
            results.drift_report["features"].clone(),
        ]],
    )?;
    let items = journey
        .rows(format!(
            "SELECT actual, expected FROM vala.eval.result_items WHERE result_id = '{}'",
            results.eval
        ))
        .await?;
    expect_eq("Eval item payloads", &items, &vec![results.item.clone()])?;

    // Gateway capture: both selected payloads decode to the call's JSON.
    let payloads = journey
        .rows(format!(
            "SELECT request_payload, response_payload FROM vala.gateway.calls \
             WHERE wyrd_request_id = '{}'",
            call.request_id
        ))
        .await?;
    let [payload] = payloads.as_slice() else {
        return Err(format!("one captured call expected: {payloads:?}").into());
    };
    expect_eq(
        "captured request messages",
        &payload[0]["messages"],
        &call.request["messages"],
    )?;
    expect_eq("captured response", &payload[1], &call.response)?;

    // Agent trace: messages and tool IO keep their JSON types.
    let traces = journey
        .rows(format!(
            "SELECT messages, tool_io FROM vala.dev.agent_traces \
             WHERE dev_session_id = '{}'",
            trace.session
        ))
        .await?;
    expect_eq(
        "agent trace payloads",
        &traces,
        &vec![vec![trace.messages, trace.tool_io]],
    )?;

    // Audit: the stored detail decodes to the hashed JSON, and the staging
    // writer's own hash over it at the stored chain position reproduces the
    // row's entry hash.
    let audit = journey
        .rows(format!(
            "SELECT seq, prev_hash, detail, entry_hash FROM vala.system.audit_log \
             WHERE operation = '{}'",
            decision.event.operation
        ))
        .await?;
    let [row] = audit.as_slice() else {
        return Err(format!("one retained decision expected: {audit:?}").into());
    };
    let [seq, Value::String(prev_hash), detail, stored_hash] = row.as_slice() else {
        return Err(format!("an audit row has the wrong columns: {row:?}").into());
    };
    expect_eq("audit detail", detail, &decision.detail)?;
    let recomputed = entry_hash(
        &hex::decode(prev_hash)?,
        seq.as_i64().ok_or("the audit seq is not an integer")?,
        &decision.event,
        None,
        Some(&serde_jcs::to_string(detail)?),
    );
    expect_eq(
        "audit entry hash",
        &json!(hex::encode(recomputed)),
        stored_hash,
    )?;

    journey.server.shutdown().await?;
    Ok(())
}

/// Raw Arrow IPC that bypasses every client-side Variant check reaches Scribe
/// admission for a non-signal built-in and a nested signal Variant, and each
/// malformed frame is refused with its exact catalogued error before any ACK.
///
/// `vala.dev.agent_traces` covers a missing and a foreign extension marker,
/// the Variant marker with foreign extension metadata, invalid bytes, one
/// container past the depth limit, a compact value nested 20,000 levels deep
/// that is refused for depth without taking the server down, and one byte
/// past the size limit. Malformed bytes below an over-deep container or
/// beside an over-deep sibling in either order are refused as invalid, and a
/// Decimal16 outside the exact numeric domain — fractional, past `u64::MAX`,
/// or a scale-zero value within `i64` — is refused for numeric range, also
/// beside an over-deep sibling. A `vala.metrics.points` frame whose absent
/// bucket set keeps a child value is refused as a partial Struct.
/// Precedence is pinned: an oversized value that is also
/// malformed reports its size, a wrong marker on one field outranks invalid
/// bytes in another, and an undeclared column or a non-Variant type change
/// outranks each of invalid, over-deep, and oversized Variant bytes. A JSON
/// row whose Variant holds an out-of-range integer beside an over-deep branch
/// is refused for numeric range in both key orders, and also when the integer
/// sits inside the over-deep branch, before any ACK.
/// `vala.traces.spans` covers the Variant nested in `events`, with a missing
/// marker and with foreign extension metadata, named by its top-level column.
/// A non-Variant type change stays a fingerprint mismatch. Afterward the same
/// server accepts the valid sentinel frames and only they are readable, so no
/// refused frame persisted a row; the accepted trace's scale-zero `u64::MAX`
/// decimal reads back with every digit.
///
/// # Errors
///
/// Returns the first frame whose outcome or stored rows differ.
///
/// # Panics
///
/// Panics only if `#[tokio::test]` cannot build its runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn builtin_variant_columns_are_refused_before_ack() -> Result<(), ServerJourneyError> {
    let admission = VariantAdmissionJourney::start().await?;
    let refused = admission.session("refused");
    let valid = || raw_variant(&EMPTY_METADATA, &[VARIANT_NULL]);
    let invalid = || raw_variant(&[0xff], &[0xff]);
    let storage_only =
        |name: &str, nullable: bool| Field::new(name, variant_storage_type(), nullable);
    let foreign = |name: &str| {
        storage_only(name, false).with_metadata(HashMap::from([(
            EXTENSION_NAME_KEY.to_owned(),
            "arrow.json".to_owned(),
        )]))
    };
    let foreign_metadata = |name: &str| {
        variant_field(name, false).with_metadata(HashMap::from([
            (
                EXTENSION_NAME_KEY.to_owned(),
                "arrow.parquet.variant".to_owned(),
            ),
            (
                EXTENSION_METADATA_KEY.to_owned(),
                r#"{"shredded":true}"#.to_owned(),
            ),
        ]))
    };
    let messages = || variant_field("messages", false);
    let tool_io = || variant_field("tool_io", true);
    let limit = usize::try_from(VARIANT_MAX_ENCODED_BYTES)?;
    let unsupported = |field: &str| BifrostError::UnsupportedType {
        field: field.to_owned(),
        data_type: variant_storage_type().to_string(),
    };
    let invalid_messages = BifrostError::VariantInvalidJson {
        field: "messages".to_owned(),
        row: 0,
        path: String::new(),
    };
    let out_of_range = |path: &str, numeric_kind: &str| BifrostError::VariantNumericOutOfRange {
        field: "messages".to_owned(),
        row: 0,
        path: path.to_owned(),
        numeric_kind: numeric_kind.to_owned(),
    };
    let field_names = (0..200)
        .map(|index| format!("f{index:03}"))
        .collect::<Vec<_>>();
    let many_names = variant_metadata(
        &field_names.iter().map(String::as_str).collect::<Vec<_>>(),
        true,
    )?;
    let mut large_string = primitive(VARIANT_LONG_STRING, &1000_u32.to_le_bytes());
    large_string.extend([b'x'; 1000]);
    let one_shared_child = variant_object(
        &(0..200).map(|id| (id, 0)).collect::<Vec<_>>(),
        &large_string,
    )?;
    let over_deep = nested_lists(VARIANT_MAX_DEPTH + 1)?;
    let deepest = nested_lists(VARIANT_MAX_DEPTH)?;
    EncodedVariant::from_bytes(&EMPTY_METADATA, &deepest)
        .map_err(|violation| format!("the depth-limit fixture is invalid: {violation:?}"))?;
    let too_deep = BifrostError::VariantTooDeep {
        field: "messages".to_owned(),
        row: 0,
        path: "/0".repeat(usize::try_from(VARIANT_MAX_DEPTH)?),
        depth: VARIANT_MAX_DEPTH + 1,
        limit: VARIANT_MAX_DEPTH,
    };

    let traces = [
        (
            "missing extension",
            (storage_only("messages", false), valid()?),
            unsupported("messages"),
        ),
        (
            "foreign extension",
            (foreign("messages"), valid()?),
            unsupported("messages"),
        ),
        (
            "foreign extension metadata",
            (foreign_metadata("messages"), valid()?),
            unsupported("messages"),
        ),
        (
            "invalid bytes",
            (messages(), invalid()?),
            invalid_messages.clone(),
        ),
        (
            "one container past the depth limit",
            (
                messages(),
                raw_variant(&EMPTY_METADATA, &nested_lists(VARIANT_MAX_DEPTH + 1)?)?,
            ),
            too_deep.clone(),
        ),
        (
            "a compact hostile depth",
            (
                messages(),
                raw_variant(&EMPTY_METADATA, &nested_lists(20_000)?)?,
            ),
            too_deep.clone(),
        ),
        (
            "malformed bytes below a container past the depth limit",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &(0..VARIANT_MAX_DEPTH + 5)
                        .try_fold(vec![VARIANT_MALFORMED], |value, _| variant_list(&[value]))?,
                )?,
            ),
            invalid_messages.clone(),
        ),
        (
            "objects whose fields share one child at every level",
            (
                messages(),
                raw_variant(&AB_METADATA, &shared_objects(VARIANT_MAX_DEPTH)?)?,
            ),
            invalid_messages.clone(),
        ),
        (
            "malformed bytes after an over-deep sibling",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &variant_list(&[over_deep.clone(), vec![VARIANT_MALFORMED]])?,
                )?,
            ),
            invalid_messages.clone(),
        ),
        (
            "malformed bytes before an over-deep sibling",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &variant_list(&[vec![VARIANT_MALFORMED], over_deep.clone()])?,
                )?,
            ),
            invalid_messages.clone(),
        ),
        (
            "many fields sharing one large child",
            (messages(), raw_variant(&many_names, &one_shared_child)?),
            invalid_messages.clone(),
        ),
        (
            "one field starting inside another",
            (
                messages(),
                raw_variant(
                    &AB_METADATA,
                    &variant_object(&[(0, 0), (1, 1)], &[0x0c, 0x00])?,
                )?,
            ),
            invalid_messages.clone(),
        ),
        (
            "two fields resolving to one name",
            (
                messages(),
                raw_variant(
                    &variant_metadata(&["a", "a"], false)?,
                    &variant_object(&[(0, 0), (1, 1)], &[VARIANT_NULL, VARIANT_NULL])?,
                )?,
            ),
            invalid_messages.clone(),
        ),
        (
            "a Decimal4",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &primitive(VARIANT_DECIMAL4, &[0, 5, 0, 0, 0]),
                )?,
            ),
            out_of_range("", "decimal"),
        ),
        (
            "a Decimal8",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &primitive(VARIANT_DECIMAL8, &[0, 5, 0, 0, 0, 0, 0, 0, 0]),
                )?,
            ),
            out_of_range("", "decimal"),
        ),
        (
            "a NaN double",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &primitive(VARIANT_DOUBLE, &f64::NAN.to_le_bytes()),
                )?,
            ),
            out_of_range("", "double"),
        ),
        (
            "an infinite float",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &primitive(VARIANT_FLOAT, &f32::INFINITY.to_le_bytes()),
                )?,
            ),
            out_of_range("", "double"),
        ),
        (
            "a fractional decimal",
            (
                messages(),
                raw_variant(&EMPTY_METADATA, &decimal16(12_345, 2))?,
            ),
            out_of_range("", "decimal"),
        ),
        (
            "a scale-zero decimal past u64::MAX",
            (
                messages(),
                raw_variant(&EMPTY_METADATA, &decimal16(i128::from(u64::MAX) + 1, 0))?,
            ),
            out_of_range("", "decimal"),
        ),
        (
            "a scale-zero decimal within i64",
            (messages(), raw_variant(&EMPTY_METADATA, &decimal16(5, 0))?),
            out_of_range("", "decimal"),
        ),
        (
            "a refused decimal after an over-deep sibling",
            (
                messages(),
                raw_variant(
                    &EMPTY_METADATA,
                    &variant_list(&[over_deep.clone(), decimal16(5, 0)])?,
                )?,
            ),
            out_of_range("/1", "decimal"),
        ),
        (
            "an oversized malformed value reports its size",
            (messages(), raw_variant(&[0xff], &vec![0xff; limit])?),
            BifrostError::VariantTooLarge {
                field: "messages".to_owned(),
                row: 0,
                bytes: VARIANT_MAX_ENCODED_BYTES + 1,
                limit: VARIANT_MAX_ENCODED_BYTES,
            },
        ),
    ];
    for (what, messages, expected) in traces {
        let frame = admission.trace_frame(&refused, messages, (tool_io(), valid()?))?;
        admission
            .refuse(what, AGENT_TRACES, &frame, &expected)
            .await?;
    }
    let frame = admission.trace_frame(
        &refused,
        (messages(), invalid()?),
        (storage_only("tool_io", true), valid()?),
    )?;
    admission
        .refuse(
            "a wrong marker outranks invalid bytes",
            AGENT_TRACES,
            &frame,
            &unsupported("tool_io"),
        )
        .await?;
    let large_model = |frame: RecordBatch| -> Result<RecordBatch, ServerJourneyError> {
        let fields = frame
            .schema()
            .fields()
            .iter()
            .map(|field| match field.name().as_str() {
                "model" => Arc::new(Field::new("model", DataType::LargeUtf8, false)),
                _ => Arc::clone(field),
            })
            .collect::<Vec<_>>();
        let mut columns = frame.columns().to_vec();
        columns[frame.schema().index_of("model")?] = cast(
            frame.column_by_name("model").ok_or("no model")?,
            &DataType::LargeUtf8,
        )?;
        Ok(RecordBatch::try_new(
            Arc::new(Schema::new(fields)),
            columns,
        )?)
    };
    let undeclared = |frame: RecordBatch| -> Result<RecordBatch, ServerJourneyError> {
        let mut fields = frame.schema().fields().to_vec();
        fields.push(Arc::new(Field::new("undeclared", DataType::Utf8, true)));
        let mut columns = frame.columns().to_vec();
        columns.push(Arc::new(StringArray::from(vec![None::<&str>])));
        Ok(RecordBatch::try_new(
            Arc::new(Schema::new(fields)),
            columns,
        )?)
    };
    let frame = admission.trace_frame(&refused, (messages(), valid()?), (tool_io(), valid()?))?;
    let error = admission
        .refusal(AGENT_TRACES, &large_model(frame)?)
        .await?;
    expect_eq(
        "a non-Variant type change",
        &error.code(),
        &"WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
    )?;
    let variant_defects = [
        ("invalid", invalid()?),
        (
            "over-deep",
            raw_variant(&EMPTY_METADATA, &nested_lists(VARIANT_MAX_DEPTH + 1)?)?,
        ),
        ("oversized", raw_variant(&[0xff], &vec![0xff; limit])?),
    ];
    for (defect, bytes) in variant_defects {
        let frame = || {
            admission.trace_frame(
                &refused,
                (messages(), Arc::clone(&bytes)),
                (tool_io(), valid()?),
            )
        };
        admission
            .refuse(
                &format!("an undeclared column outranks {defect} Variant bytes"),
                AGENT_TRACES,
                &undeclared(frame()?)?,
                &BifrostError::UndeclaredField {
                    field: "undeclared".to_owned(),
                    row: 0,
                },
            )
            .await?;
        let error = admission
            .refusal(AGENT_TRACES, &large_model(frame()?)?)
            .await?;
        expect_eq(
            &format!("a non-Variant type change outranks {defect} Variant bytes"),
            &error.code(),
            &"WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH",
        )?;
    }
    admission.refuse_json_rows().await?;

    let refused_spans = admission.spans_frame(&refused);
    let event_storage =
        with_event_attributes(&refused_spans, storage_only("attributes", false), valid()?)?;
    let events_type = event_storage
        .schema()
        .field_with_name("events")?
        .data_type()
        .to_string();
    admission
        .refuse(
            "a nested marker",
            SPANS,
            &event_storage,
            &BifrostError::UnsupportedType {
                field: "events".to_owned(),
                data_type: events_type,
            },
        )
        .await?;
    let event_metadata =
        with_event_attributes(&refused_spans, foreign_metadata("attributes"), valid()?)?;
    let events_type = event_metadata
        .schema()
        .field_with_name("events")?
        .data_type()
        .to_string();
    admission
        .refuse(
            "nested foreign extension metadata",
            SPANS,
            &event_metadata,
            &BifrostError::UnsupportedType {
                field: "events".to_owned(),
                data_type: events_type,
            },
        )
        .await?;
    let event_bytes = with_event_attributes(
        &refused_spans,
        variant_field("attributes", false),
        invalid()?,
    )?;
    admission
        .refuse(
            "nested invalid bytes",
            SPANS,
            &event_bytes,
            &BifrostError::VariantInvalidJson {
                field: "events".to_owned(),
                row: 0,
                path: String::new(),
            },
        )
        .await?;

    admission
        .refuse(
            "a null bucket set whose offset child is still set",
            POINTS,
            &admission.points_frame(&refused, true)?,
            &BifrostError::SchemaParse {
                detail: "row 0: positive_buckets.offset must be null exactly when \
                         positive_buckets is null"
                    .to_owned(),
            },
        )
        .await?;

    // The accepted trace carries the one decimal form the exact numeric domain
    // admits, which must read back with every digit, and a finite float.
    let accepted = admission.session("accepted");
    let sentinel = admission.trace_frame(
        &accepted,
        (
            messages(),
            raw_variant(&EMPTY_METADATA, &decimal16(i128::from(u64::MAX), 0))?,
        ),
        (
            tool_io(),
            raw_variant(
                &EMPTY_METADATA,
                &primitive(VARIANT_FLOAT, &1.5_f32.to_le_bytes()),
            )?,
        ),
    )?;
    admission.accept(AGENT_TRACES, &sentinel).await?;
    admission
        .accept(SPANS, &admission.spans_frame(&accepted))
        .await?;
    admission
        .accept(POINTS, &admission.points_frame(&accepted, false)?)
        .await?;
    admission.journey.server.flush_bifrost().await?;

    let suffix = &admission.run;
    expect_eq(
        "stored agent traces",
        &admission
            .journey
            .rows(format!(
                "SELECT dev_session_id, count(*) FROM {AGENT_TRACES} \
                 WHERE dev_session_id LIKE '%{suffix}' GROUP BY dev_session_id"
            ))
            .await?,
        &vec![vec![json!(accepted), json!(1)]],
    )?;
    expect_eq(
        "the accepted u64::MAX decimal and finite float",
        &admission
            .journey
            .rows(format!(
                "SELECT messages, tool_io FROM {AGENT_TRACES} \
                 WHERE dev_session_id = '{accepted}'"
            ))
            .await?,
        &vec![vec![json!(u64::MAX), json!(1.5)]],
    )?;
    expect_eq(
        "stored metric points",
        &admission
            .journey
            .rows(format!(
                "SELECT scope_name, count(*) FROM {POINTS} \
                 WHERE scope_name LIKE '%{suffix}' GROUP BY scope_name"
            ))
            .await?,
        &vec![vec![json!(accepted), json!(3)]],
    )?;
    expect_eq(
        "stored spans",
        &admission
            .journey
            .rows(format!(
                "SELECT scope_name, count(*) FROM {SPANS} \
                 WHERE scope_name LIKE '%{suffix}' GROUP BY scope_name"
            ))
            .await?,
        &vec![vec![json!(accepted), json!(2)]],
    )?;

    admission.journey.server.shutdown().await?;
    Ok(())
}

/// The shared verdict table the typed-payload journey publishes to.
const RESULTS: &str = "vala.verification.results";

/// The non-signal built-in the Variant admission journey writes.
const AGENT_TRACES: &str = "vala.dev.agent_traces";

/// The signal built-in whose nested `events` Variant the journey writes.
const SPANS: &str = "vala.traces.spans";

/// Arrow field-metadata key naming a field's extension type.
const EXTENSION_NAME_KEY: &str = "ARROW:extension:name";

/// Arrow field-metadata key carrying a field's extension parameters.
const EXTENSION_METADATA_KEY: &str = "ARROW:extension:metadata";

/// Variant metadata with an empty key dictionary.
const EMPTY_METADATA: [u8; 3] = [0x01, 0x00, 0x00];

/// Variant metadata with the sorted key dictionary `["a", "b"]`.
const AB_METADATA: [u8; 7] = [0x11, 2, 0, 1, 2, b'a', b'b'];

/// The Variant null primitive value.
const VARIANT_NULL: u8 = 0x00;

/// Variant array header: four-byte offsets and a one-byte element count.
const VARIANT_ARRAY_HEADER: u8 = 0x0f;

/// Variant primitive type of a Double.
const VARIANT_DOUBLE: u8 = 7;

/// Variant primitive type of a Decimal4.
const VARIANT_DECIMAL4: u8 = 8;

/// Variant primitive type of a Decimal8.
const VARIANT_DECIMAL8: u8 = 9;

/// Variant primitive header of a Decimal16 (primitive type 10).
const VARIANT_DECIMAL16: u8 = 10 << 2;

/// Variant primitive type of a Float.
const VARIANT_FLOAT: u8 = 14;

/// Variant primitive type of a string with a four-byte length.
const VARIANT_LONG_STRING: u8 = 16;

/// A Variant primitive header naming no primitive type, so it is malformed.
const VARIANT_MALFORMED: u8 = 31 << 2;

/// The canonical metric points ledger the journey writes raw frames to.
const POINTS: &str = "vala.metrics.points";

/// One bound server and an admin ingest door that sends raw Arrow IPC frames.
struct VariantAdmissionJourney {
    /// Server, tenant, and published-row reader.
    journey: TypedPayloadJourney,
    /// Ingest transport that sends frames exactly as built.
    ingest: RawIngest,
    /// Admin client the public Bifrost facade writes JSON rows through.
    client: wyrd_client::WyrdClient,
    /// The described `vala.traces.spans` user schema.
    spans: SchemaRef,
    /// The described `vala.metrics.points` user schema.
    points: SchemaRef,
    /// Suffix that scopes this run's sessions and span scopes.
    run: String,
}

impl VariantAdmissionJourney {
    /// Start the server, provision both built-ins, and connect an admin
    /// ingest transport.
    ///
    /// # Errors
    ///
    /// Returns a start, bootstrap, provisioning, client, or describe error.
    async fn start() -> Result<Self, ServerJourneyError> {
        let journey = TypedPayloadJourney::start().await?;
        for (namespace, name) in [
            ("dev", "agent_traces"),
            ("traces", "spans"),
            ("metrics", "points"),
        ] {
            journey
                .server
                .ensure_builtin_table_for_test(journey.tenant, namespace, name)
                .await?;
        }
        let Bootstrap::Machine { api_key, .. } = journey
            .server
            .bootstrap_service("variant_admission_writer", &["admin"])
            .await?
        else {
            return Err("a service bootstrap returned a user".into());
        };
        let client = wyrd_client::bifrost::client_from_options(
            journey.server.base_url(),
            Some(api_key.expose_secret()),
            journey.server.grpc_url().as_deref(),
        )?;
        let spans = Arc::clone(TableConfig::describe(&client, SPANS).await?.user_schema());
        let points = Arc::clone(TableConfig::describe(&client, POINTS).await?.user_schema());
        Ok(Self {
            ingest: RawIngest::connect(&client).await?,
            client,
            journey,
            spans,
            points,
            run: uuid::Uuid::now_v7().simple().to_string(),
        })
    }

    /// Require that each JSON agent-trace row is refused before any ACK with
    /// the exact Variant error the revision-13 order selects.
    ///
    /// Numeric range outranks depth in both key orders and inside the
    /// over-depth container; valid JSON nested 129 and 10,000 levels deep
    /// is too deep rather than invalid; and a `messages` value whose built
    /// bytes already exceed the size limit reports size beside a refused
    /// number or an over-depth sibling. Each row goes through the public
    /// facade's JSON-row path with its integer token intact, and the refusal
    /// surfaces from the flush before any frame is acknowledged.
    ///
    /// # Errors
    ///
    /// Returns a connect, describe, or enqueue error, or a description when
    /// a row is accepted or refused with any other error.
    async fn refuse_json_rows(&self) -> Result<(), ServerJourneyError> {
        let nested = |inner: &str, levels: usize| {
            format!("{}{inner}{}", "[".repeat(levels), "]".repeat(levels))
        };
        let deep = nested("1", 65);
        let numeric = |path: String| BifrostError::VariantNumericOutOfRange {
            field: "messages".to_owned(),
            row: 0,
            path,
            numeric_kind: "integer".to_owned(),
        };
        let too_deep = BifrostError::VariantTooDeep {
            field: "messages".to_owned(),
            row: 0,
            path: "/0".repeat(64),
            depth: VARIANT_MAX_DEPTH + 1,
            limit: VARIANT_MAX_DEPTH,
        };
        let big = format!(
            r#""{}""#,
            "x".repeat(usize::try_from(VARIANT_MAX_ENCODED_BYTES)?)
        );
        // The exact byte count is whatever the shared owner built before
        // refusing; the facade must surface that size error unchanged.
        let too_large = |messages: String| -> Result<(String, BifrostError), ServerJourneyError> {
            let error = EncodedVariant::from_json_text(&messages)
                .err()
                .ok_or("the oversized row encodes")?
                .into_error("messages", 0);
            expect_eq(
                "the oversized row's error",
                &error.code(),
                &"WYRD_VALA_413_VARIANT_TOO_LARGE",
            )?;
            Ok((messages, error))
        };
        let cases = [
            (
                format!(r#"{{"a": 18446744073709551616, "b": {deep}}}"#),
                numeric("/a".to_owned()),
            ),
            (
                format!(r#"{{"a": {deep}, "b": 18446744073709551616}}"#),
                numeric("/b".to_owned()),
            ),
            (
                format!(r#"{{"a": {}}}"#, nested("18446744073709551616", 65)),
                numeric(format!("/a{}", "/0".repeat(65))),
            ),
            (nested("1", 129), too_deep.clone()),
            (nested("1", 10_000), too_deep),
            too_large(format!(
                r#"{{"big": {big}, "other": 18446744073709551616}}"#
            ))?,
            too_large(format!(r#"{{"big": {big}, "other": {deep}}}"#))?,
        ];
        for (messages, expected) in cases {
            let bifrost = Bifrost::connect(&self.client).await?;
            let table = bifrost.writer_table(AGENT_TRACES).await?;
            let fields = table
                .user_schema()
                .fields()
                .iter()
                .map(|field| {
                    let value = match field.data_type() {
                        _ if field.name() == "messages" => messages.clone(),
                        _ if field.is_nullable() => "null".to_owned(),
                        DataType::Timestamp(..) => format!("\"{}\"", Utc::now().to_rfc3339()),
                        _ => format!("\"{}\"", self.session("refused")),
                    };
                    format!("{}: {value}", Value::from(field.name().as_str()))
                })
                .collect::<Vec<_>>();
            bifrost.insert_into(
                &table,
                format!("{{{}}}", fields.join(", ")).into_bytes(),
                wyrd_client::bifrost::Correlation::default(),
            )?;
            let refused = bifrost
                .flush()
                .await
                .map(|()| "the row was accepted".to_owned())
                .map_err(|error| WyrdError::from(&error));
            let what = format!("JSON row refused as {}", expected.code());
            match refused {
                Err(WyrdError::Vala { error }) => expect_eq(&what, &error, &expected)?,
                other => return Err(format!("{what}: {other:?}").into()),
            }
        }
        Ok(())
    }

    /// Name one session or span scope of this run.
    fn session(&self, outcome: &str) -> String {
        format!("{outcome}-{}", self.run)
    }

    /// One agent trace under `session` with the given Variant columns.
    ///
    /// Every other column takes a fixed valid value, so a refusal can come
    /// only from the two supplied fields or a deliberate schema change.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the columns do not form a batch.
    fn trace_frame(
        &self,
        session: &str,
        messages: (Field, ArrayRef),
        tool_io: (Field, ArrayRef),
    ) -> Result<RecordBatch, ServerJourneyError> {
        let now = Utc::now().timestamp_micros();
        let at = || Arc::new(TimestampMicrosecondArray::from(vec![now]).with_timezone("UTC"));
        let text = |value: &str| Arc::new(StringArray::from(vec![value])) as ArrayRef;
        let mut fields = AgentTracesTable::arrow_fields();
        let columns: Vec<ArrayRef> = vec![
            text(session),
            text("wyrd"),
            Arc::new(StringArray::from(vec![None::<&str>])),
            text("main"),
            text("variant-admission"),
            Arc::new(FixedSizeBinaryArray::new_null(16, 1)),
            Arc::new(FixedSizeBinaryArray::new_null(8, 1)),
            text("assistant"),
            text("gpt-4o"),
            text("openai"),
            messages.1,
            tool_io.1,
            at(),
            at(),
        ];
        for field in [messages.0, tool_io.0] {
            let index = fields
                .iter()
                .position(|declared| declared.name() == field.name())
                .ok_or("an agent trace field is undeclared")?;
            fields[index] = field;
        }
        Ok(RecordBatch::try_new(
            Arc::new(Schema::new(fields)),
            columns,
        )?)
    }

    /// The canonical two-span fixture under span scope `scope`.
    fn spans_frame(&self, scope: &str) -> RecordBatch {
        canonical_signals::spans(
            &self.spans,
            scope,
            Utc::now().timestamp_nanos_opt().unwrap_or(0),
        )
    }

    /// The canonical three-point fixture under scope `scope`.
    ///
    /// With `partial`, the first point's absent `positive_buckets` keeps a
    /// set `offset` child, a value no metric point can represent.
    ///
    /// # Errors
    ///
    /// Returns an error when the bucket column is not the declared Struct or
    /// the rebuilt batch does not assemble.
    fn points_frame(&self, scope: &str, partial: bool) -> Result<RecordBatch, ServerJourneyError> {
        let points = canonical_signals::points(
            &self.points,
            scope,
            Utc::now().timestamp_nanos_opt().unwrap_or(0),
        );
        if !partial {
            return Ok(points);
        }
        let index = points.schema().index_of("positive_buckets")?;
        let (fields, mut children, nulls) = points
            .column(index)
            .as_struct_opt()
            .ok_or("positive_buckets is a Struct")?
            .clone()
            .into_parts();
        children[0] = Arc::new(Int32Array::from(vec![0; points.num_rows()]));
        let mut columns = points.columns().to_vec();
        columns[index] = Arc::new(StructArray::try_new(fields, children, nulls)?);
        Ok(RecordBatch::try_new(points.schema(), columns)?)
    }

    /// Send `batch` as one raw IPC frame and return the refusal it earns.
    ///
    /// # Errors
    ///
    /// Returns an encoding error, or a description when the frame is acked.
    async fn refusal(
        &self,
        table: &str,
        batch: &RecordBatch,
    ) -> Result<WyrdError, ServerJourneyError> {
        match self
            .ingest
            .insert(table, uuid::Uuid::now_v7(), ipc(batch)?)
            .await
        {
            Ok(request) => Err(format!("{table} acked malformed frame {request}").into()),
            Err(error) => Ok(error),
        }
    }

    /// Require that `batch` is refused with exactly `expected`.
    ///
    /// # Errors
    ///
    /// Returns a description naming `what` when the frame is acked or the
    /// refusal is any other error.
    async fn refuse(
        &self,
        what: &str,
        table: &str,
        batch: &RecordBatch,
        expected: &BifrostError,
    ) -> Result<(), ServerJourneyError> {
        match self.refusal(table, batch).await? {
            WyrdError::Vala { error } => expect_eq(what, &error, expected),
            other => Err(format!("{what} was refused as {other:?}").into()),
        }
    }

    /// Require that `batch` is acknowledged.
    ///
    /// # Errors
    ///
    /// Returns the encoding error or the server's refusal.
    async fn accept(&self, table: &str, batch: &RecordBatch) -> Result<(), ServerJourneyError> {
        self.ingest
            .insert(table, uuid::Uuid::now_v7(), ipc(batch)?)
            .await?;
        Ok(())
    }
}

/// One Variant storage cell holding exactly `metadata` and `value`.
///
/// The bytes are not validated, which is what lets a frame carry a Variant
/// no client constructor would produce.
///
/// # Errors
///
/// Returns the Arrow error when the storage struct does not assemble.
fn raw_variant(metadata: &[u8], value: &[u8]) -> Result<ArrayRef, ServerJourneyError> {
    let DataType::Struct(children) = variant_storage_type() else {
        return Err("Variant storage is not a struct".into());
    };
    Ok(Arc::new(StructArray::try_new(
        children,
        vec![
            Arc::new(BinaryArray::from_vec(vec![metadata])),
            Arc::new(BinaryArray::from_vec(vec![value])),
        ],
        None,
    )?))
}

/// Variant value bytes for `depth` single-element lists around a null.
///
/// # Errors
///
/// Returns an error when the encoding outgrows a four-byte offset.
fn nested_lists(depth: u32) -> Result<Vec<u8>, ServerJourneyError> {
    (0..depth).try_fold(vec![VARIANT_NULL], |value, _| variant_list(&[value]))
}

/// Variant value bytes for a list of the already-encoded `items`.
///
/// The list uses four-byte offsets, so the encoding stays valid at any depth
/// the journey needs. The items are not validated, which lets a list carry a
/// malformed element.
///
/// # Errors
///
/// Returns an error when the list outgrows a one-byte count or a four-byte
/// offset.
fn variant_list(items: &[Vec<u8>]) -> Result<Vec<u8>, ServerJourneyError> {
    let mut list = vec![VARIANT_ARRAY_HEADER, u8::try_from(items.len())?];
    let mut offset = 0_u32;
    list.extend(offset.to_le_bytes());
    for item in items {
        offset = offset
            .checked_add(u32::try_from(item.len())?)
            .ok_or("the list outgrows a four-byte offset")?;
        list.extend(offset.to_le_bytes());
    }
    list.extend(items.concat());
    Ok(list)
}

/// Variant value bytes for `levels` objects whose fields `a` and `b` both
/// point at the same child, ending in a null.
///
/// A few bytes per level describe 2^`levels` nodes, the shape a hostile
/// writer uses to make a naive walk run forever. Keys come from
/// [`AB_METADATA`].
///
/// # Errors
///
/// Returns an error when the chain outgrows a two-byte offset.
fn shared_objects(levels: u32) -> Result<Vec<u8>, ServerJourneyError> {
    (0..levels).try_fold(vec![VARIANT_NULL], |child, _| {
        variant_object(&[(0, 0), (1, 0)], &child)
    })
}

/// Variant value bytes for an object with one-byte field ids and two-byte
/// offsets.
///
/// `fields` pairs each field id with its value's start offset inside
/// `values`, which follow the offsets; the end offset is `values.len()`.
/// Nothing is validated, so fields may share or overlap bytes.
///
/// # Errors
///
/// Returns an error when the object outgrows a one-byte count or a two-byte
/// offset.
fn variant_object(fields: &[(u8, u16)], values: &[u8]) -> Result<Vec<u8>, ServerJourneyError> {
    // Object header: two-byte offsets, one-byte field ids, one-byte count.
    let mut object = vec![0x06, u8::try_from(fields.len())?];
    object.extend(fields.iter().map(|(id, _)| id));
    for (_, offset) in fields {
        object.extend(offset.to_le_bytes());
    }
    object.extend(u16::try_from(values.len())?.to_le_bytes());
    object.extend_from_slice(values);
    Ok(object)
}

/// Variant metadata naming `names` with four-byte offsets.
///
/// The sorted-names flag is set when `sorted` is true; nothing checks that
/// claim, so a fixture can name one key twice.
///
/// # Errors
///
/// Returns an error when the dictionary outgrows a four-byte offset.
fn variant_metadata(names: &[&str], sorted: bool) -> Result<Vec<u8>, ServerJourneyError> {
    let mut metadata = vec![if sorted { 0xd1 } else { 0xc1 }];
    metadata.extend(u32::try_from(names.len())?.to_le_bytes());
    let mut offset = 0_u32;
    metadata.extend(offset.to_le_bytes());
    for name in names {
        offset += u32::try_from(name.len())?;
        metadata.extend(offset.to_le_bytes());
    }
    metadata.extend(names.concat().into_bytes());
    Ok(metadata)
}

/// Variant value bytes for one primitive of `type_id` with `payload`.
fn primitive(type_id: u8, payload: &[u8]) -> Vec<u8> {
    let mut value = vec![type_id << 2];
    value.extend_from_slice(payload);
    value
}

/// Variant value bytes for one Decimal16 primitive.
fn decimal16(integer: i128, scale: u8) -> Vec<u8> {
    let mut value = vec![VARIANT_DECIMAL16, scale];
    value.extend(integer.to_le_bytes());
    value
}

/// Replace the `attributes` Variant of every `events` element in `spans`.
///
/// The supplied field and values stand in for the declared child, and the
/// list and top-level field are rebuilt around them unchanged otherwise.
///
/// # Errors
///
/// Returns an error when `events` is not a list of structs with an
/// `attributes` child, or the rebuilt arrays do not assemble.
fn with_event_attributes(
    spans: &RecordBatch,
    attributes: Field,
    values: ArrayRef,
) -> Result<RecordBatch, ServerJourneyError> {
    let schema = spans.schema();
    let index = schema.index_of("events")?;
    let events = spans
        .column(index)
        .as_list_opt::<i32>()
        .ok_or("events is a list")?;
    let DataType::List(element) = events.data_type() else {
        return Err("events is a list".into());
    };
    let (children, mut columns, nulls) = events
        .values()
        .as_struct_opt()
        .ok_or("an event is a struct")?
        .clone()
        .into_parts();
    let (position, _) = children
        .find("attributes")
        .ok_or("an event has attributes")?;
    let mut children = children.iter().cloned().collect::<Vec<_>>();
    children[position] = Arc::new(attributes);
    columns[position] = values;
    let elements = StructArray::try_new(children.into(), columns, nulls)?;
    let element = Arc::new(
        element
            .as_ref()
            .clone()
            .with_data_type(elements.data_type().clone()),
    );
    let events = ListArray::try_new(
        element,
        events.offsets().clone(),
        Arc::new(elements),
        events.nulls().cloned(),
    )?;
    let mut fields = schema.fields().iter().cloned().collect::<Vec<_>>();
    fields[index] = Arc::new(
        schema
            .field(index)
            .clone()
            .with_data_type(events.data_type().clone()),
    );
    let mut columns = spans.columns().to_vec();
    columns[index] = Arc::new(events);
    Ok(RecordBatch::try_new(
        Arc::new(Schema::new(fields)),
        columns,
    )?)
}

/// Encode `batch` as one Arrow IPC stream.
///
/// # Errors
///
/// Returns the Arrow error when the stream does not encode.
fn ipc(batch: &RecordBatch) -> Result<Vec<u8>, ServerJourneyError> {
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, batch.schema().as_ref())?;
    writer.write(batch)?;
    writer.finish()?;
    drop(writer);
    Ok(bytes)
}

/// Fail with `what` and both values unless `actual` equals `expected`.
///
/// # Errors
/// Returns a description naming `what` when the values differ.
fn expect_eq<T: PartialEq + Debug>(
    what: &str,
    actual: &T,
    expected: &T,
) -> Result<(), ServerJourneyError> {
    if actual == expected {
        return Ok(());
    }
    Err(format!("{what} differs:\n  stored   {actual:?}\n  expected {expected:?}").into())
}

/// Results the journey published and the native values they must read back as.
struct PublishedResults {
    /// A Drift result whose report scored two features.
    scored_drift: VerificationResultId,
    /// A Drift result whose window could not be scored.
    unscored_drift: VerificationResultId,
    /// An Eval result with one ran assertion.
    eval: VerificationResultId,
    /// The scored report's `drift_report` as JSON: method, features, verdict.
    drift_report: Value,
    /// The Eval report's workflow summary as JSON.
    eval_summary: Value,
    /// The one Eval item's `actual` and `expected` values.
    item: Vec<Value>,
}

/// The captured gateway calls and the JSON the resolved one sent and received.
struct CapturedCall {
    /// Request id the gateway answered with, which the captured row carries.
    request_id: String,
    /// Request id of a second call the upstream refused, so its captured row
    /// has no resolved model.
    unresolved_request_id: String,
    /// Body the caller sent.
    request: Value,
    /// Body the upstream provider answered with.
    response: Value,
}

/// The written agent trace and its native payloads.
struct WrittenTrace {
    /// Session id identifying the one written row.
    session: String,
    /// The trace's message list.
    messages: Value,
    /// The trace's tool input and output.
    tool_io: Value,
}

/// The appended audit decision and its native detail.
struct AppendedDecision {
    /// The staged event whose entry hash the retained row must reproduce.
    event: AuditEvent,
    /// The detail's canonical JSON, the value the entry hash covers.
    detail: Value,
}

/// One bound server with a mock gateway upstream, and the tenant whose
/// built-in payloads the typed-payload journey writes and reads.
struct TypedPayloadJourney {
    /// Bound server whose gateway adapters target `upstream`.
    server: WyrdTestServer,
    /// Mock `OpenAI` provider answering the captured call.
    upstream: MockServer,
    /// Tenant every row belongs to.
    tenant: DataTenantId,
}

impl TypedPayloadJourney {
    /// Buffered Chat Completions answer the mock upstream returns.
    fn completion() -> Value {
        json!({
            "id": "chatcmpl-typed",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-4o",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop", "logprobs": null}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15},
        })
    }

    /// Start a bound server whose gateway providers resolve to a mock
    /// upstream that answers every chat completion, except that one asking
    /// for a single completion token is refused, so no model resolves it.
    ///
    /// # Errors
    /// Returns a mock URL or server start error.
    async fn start() -> Result<Self, ServerJourneyError> {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_partial_json(json!({"max_completion_tokens": 1})))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {"message": "refused", "type": "invalid_request_error"},
            })))
            .with_priority(1)
            .mount(&upstream)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Self::completion()))
            .mount(&upstream)
            .await;
        let server = Box::pin(
            WyrdTestServer::builder()
                .with_gateway_provider_root_for_test(Url::parse(&upstream.uri())?)
                .start_bound(),
        )
        .await?;
        let tenant = server.data_tenant_id();
        Ok(Self {
            server,
            upstream,
            tenant,
        })
    }

    /// Publish a scored Drift, an unscored Drift, and an Eval result through
    /// the production payload builder as the tenant SYSTEM writer.
    ///
    /// Before the Eval result is written, a partial copy of its summary is
    /// refused by the same writer.
    ///
    /// # Errors
    /// Returns a seeding, minting, payload, or write error.
    async fn publish_results(&self) -> Result<PublishedResults, ServerJourneyError> {
        let seed = VerificationFixture::provision(self.server.state().postgres.wyrd(), self.tenant)
            .await?;
        let (subject, _) = seed.service("typed-subject").await?;
        let uid = seed.drift_verifier("typed-drift").await?;
        let verifier: CardRef = format!("default/Verifier/typed-drift@1.0.0#{uid}").parse()?;
        for (namespace, name) in [
            ("verification", "results"),
            ("drift", "result_features"),
            ("eval", "result_items"),
        ] {
            self.server
                .ensure_builtin_table_for_test(self.tenant, namespace, name)
                .await?;
        }
        let issuer = self
            .server
            .state()
            .auth
            .tenant_issuer()
            .ok_or("the server has no tenant issuer")?;
        let mut conn = self
            .server
            .state()
            .postgres
            .wyrd()
            .tenant_conn(self.tenant)
            .await?;
        let token = issuer.issue_system_token(&mut conn, &verifier).await?;
        drop(conn);
        let bifrost = Bifrost::connect(&wyrd_client::bifrost::client_from_options(
            self.server.base_url(),
            Some(token.access_token.expose_secret()),
            self.server.grpc_url().as_deref(),
        )?)
        .await?;
        let verifier_ref = CardRef {
            uid: None,
            ..verifier
        }
        .to_string();

        let now = Utc::now();
        let window = RunInput::DriftWindow(DriftWindow {
            start: now - chrono::Duration::hours(1),
            end: now,
        });
        let scored = VerifierReport::Drift(Some(serde_json::from_value(json!({
            "method": "Custom",
            "features": {
                "latency": { "feature": "latency", "score": 3.0, "threshold": 1.0, "verdict": "Drift" },
                "tokens": { "feature": "tokens", "score": 0.1, "threshold": 1.0, "verdict": "NoDrift" }
            },
            "verdict": "Drift"
        }))?));
        let VerifierReport::Drift(Some(drift)) = &scored else {
            return Err("the scored report is not a Drift report".into());
        };
        let drift_report = json!({
            "method": "Custom",
            "features": serde_json::to_value(&drift.features)?,
            "verdict": "Drift",
        });
        let actual = json!({"answer": "yes", "scores": [1, 2.5, null], "nested": {"ok": true}});
        let expected = json!({"answer": "yes"});
        let eval_report = EvalReport {
            outcomes: vec![TaskRunOutcome::Ran(Box::new(AssertionResult {
                task_id: TaskId::new("answer")?,
                passed: true,
                actual: Some(actual.clone()),
                expected: expected.clone(),
                operator: ComparisonOperator::Equals,
                message: None,
                stage: 0,
                started_at: now,
                duration_ms: 7,
            }))],
        };
        let eval_summary = serde_json::to_value(eval_report.workflow_summary())?;
        let eval_input = RunInput::EvalRecord {
            record_id: format!("typed-{}", uuid::Uuid::now_v7()),
            event_time: now,
        };
        let eval = VerifierReport::Eval {
            report: eval_report,
            verdict: VerificationVerdict::Passed,
        };

        let mut published = Vec::new();
        for (input, report) in [
            (&window, &scored),
            (&window, &VerifierReport::Drift(None)),
            (&eval_input, &eval),
        ] {
            let result = VerificationResultId::new_v7();
            let payload = ResultPayloadBuilder::new(
                ResultRun {
                    run_id: VerificationRunId::new_v7(),
                    verifier_version: "1.0.0",
                    subject_card_uid: &subject,
                    owner_card_uid: Some(&subject),
                    binding_id: Some(BindingId::new_v7()),
                    trigger: None,
                    input,
                },
                &verifier_ref,
                result,
                now,
                now,
                now,
            )
            .build(report)?;
            for batch in payload.batches() {
                if batch.table == RESULTS && matches!(report, VerifierReport::Eval { .. }) {
                    Self::refuse_partial_summary(&bifrost, &batch.batch).await?;
                }
                bifrost.write_batch(&batch.table, &batch.batch).await?;
            }
            published.push(result);
        }
        let [scored_drift, unscored_drift, eval] = published[..] else {
            return Err("three results were not published".into());
        };
        Ok(PublishedResults {
            scored_drift,
            unscored_drift,
            eval,
            drift_report,
            eval_summary,
            item: vec![actual, expected],
        })
    }

    /// Require that a copy of the Eval `results` batch whose present
    /// `eval_summary` lacks `total_tasks` is refused before any ACK.
    ///
    /// The caller writes the intact batch afterward, and the journey's hot and
    /// published reads then find only intact rows.
    ///
    /// # Errors
    ///
    /// Returns an error when the summary is not the declared Struct, the copy
    /// does not assemble, or the write is acked or refused with another error.
    async fn refuse_partial_summary(
        bifrost: &Bifrost,
        results: &RecordBatch,
    ) -> Result<(), ServerJourneyError> {
        let index = results.schema().index_of("eval_summary")?;
        let (fields, mut children, nulls) = results
            .column(index)
            .as_struct_opt()
            .ok_or("eval_summary is a Struct")?
            .clone()
            .into_parts();
        children[0] = arrow::array::new_null_array(children[0].data_type(), results.num_rows());
        let mut columns = results.columns().to_vec();
        columns[index] = Arc::new(StructArray::try_new(fields, children, nulls)?);
        let partial = RecordBatch::try_new(results.schema(), columns)?;
        match bifrost.write_batch(RESULTS, &partial).await {
            Ok(()) => Err("a partial eval_summary was acked".into()),
            Err(error) => expect_eq(
                "a partial eval_summary",
                &WyrdError::from(&error).code(),
                &"WYRD_VALA_400_SCHEMA_PARSE",
            ),
        }
    }

    /// Configure a provider and a request-and-response payload capture
    /// policy, then invoke one chat completion as an ordinary caller.
    ///
    /// # Errors
    /// Returns a seeding, exchange, administration, or call error.
    async fn capture_gateway_call(&self) -> Result<CapturedCall, ServerJourneyError> {
        self.server
            .seed_role(
                "typed_invoker",
                &[Permission::gateway_invoke(GatewayAccess::Provider {
                    provider: "openai".parse()?,
                })],
            )
            .await?;
        let admin = self.exchange("typed_gateway_admin", &["admin"]).await?;
        let caller = self
            .exchange("typed_gateway_caller", &["typed_invoker"])
            .await?;
        let http = reqwest::Client::new();
        let base = self.server.base_url().ok_or("the server is not bound")?;
        for (route, body) in [
            (
                "provider-credentials/openai-key",
                json!({
                    "name": "openai-key",
                    "provider": "openai",
                    "source": {"managed_secret": {"secret": "sk-typed-upstream"}},
                }),
            ),
            (
                "provider-deployments/gpt-4o",
                json!({
                    "name": "gpt-4o",
                    "model": {"provider": "openai", "model": "gpt-4o"},
                    "adapter": "openai",
                    "auth": {"bearer": {"credential": "openai-key"}},
                    "capabilities": ["chat_completions"],
                    "routing_weight": 1,
                }),
            ),
            (
                "capture-policy",
                json!({"mode": "payload", "payload_fields": ["request", "response"]}),
            ),
        ] {
            let response = http
                .put(format!("{base}/v1/admin/gateway/{route}"))
                .header("x-wyrd-access-token", format!("Bearer {admin}"))
                .json(&body)
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(
                    format!("{route}: {} {}", response.status(), response.text().await?).into(),
                );
            }
        }
        let request = json!({
            "model": "openai/gpt-4o",
            "max_completion_tokens": 16,
            "messages": [
                {"role": "system", "content": "be brief"},
                {"role": "user", "content": "hi"},
            ],
        });
        let answer = http
            .post(format!("{base}/v1/chat/completions"))
            .header("authorization", format!("Bearer {caller}"))
            .json(&request)
            .send()
            .await?;
        if answer.status().as_u16() != 200 {
            return Err(format!(
                "the call failed: {} {}",
                answer.status(),
                answer.text().await?
            )
            .into());
        }
        let request_id = answer
            .headers()
            .get("wyrd-request-id")
            .and_then(|value| value.to_str().ok())
            .ok_or("the answer carries no request id")?
            .to_owned();
        if self
            .upstream
            .received_requests()
            .await
            .is_none_or(|calls| calls.is_empty())
        {
            return Err("the call never reached the mock upstream".into());
        }
        let refused = http
            .post(format!("{base}/v1/chat/completions"))
            .header("authorization", format!("Bearer {caller}"))
            .json(&json!({
                "model": "openai/gpt-4o",
                "max_completion_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await?;
        if refused.status().is_success() {
            return Err("the upstream refusal reached the caller as a success".into());
        }
        let unresolved_request_id = refused
            .headers()
            .get("wyrd-request-id")
            .and_then(|value| value.to_str().ok())
            .ok_or("the refused answer carries no request id")?
            .to_owned();
        Ok(CapturedCall {
            request_id,
            unresolved_request_id,
            request,
            response: Self::completion(),
        })
    }

    /// Bootstrap a service holding `roles` and exchange its key for a token.
    ///
    /// # Errors
    /// Returns the bootstrap or exchange error, or a user bootstrap.
    async fn exchange(&self, name: &str, roles: &[&str]) -> Result<String, ServerJourneyError> {
        let Bootstrap::Machine { api_key, .. } = self.server.bootstrap_service(name, roles).await?
        else {
            return Err("a service bootstrap returned a user".into());
        };
        Ok(self.server.exchange_api_key(&api_key).await?)
    }

    /// Write one agent trace with nested message and tool payloads through
    /// the public Bifrost facade as a tenant administrator.
    ///
    /// # Errors
    /// Returns a bootstrap, provisioning, encoding, or write error.
    async fn write_agent_trace(&self) -> Result<WrittenTrace, ServerJourneyError> {
        let Bootstrap::Machine {
            api_key, card_ref, ..
        } = self
            .server
            .bootstrap_service("typed_trace_writer", &["admin"])
            .await?
        else {
            return Err("a service bootstrap returned a user".into());
        };
        self.server
            .ensure_builtin_table_for_test(self.tenant, "dev", "agent_traces")
            .await?;
        let session = format!("typed-{}", uuid::Uuid::now_v7());
        let messages = json!([
            {"role": "user", "content": "find wyrd"},
            {"role": "assistant", "content": null,
             "tool_calls": [{"name": "search", "arguments": {"q": "wyrd", "limit": 3}}]},
        ]);
        let tool_io =
            json!({"search": {"input": {"q": "wyrd"}, "output": [1, 2.5, true, null, "x"]}});
        let variant = |value: &Value| -> Result<ArrayRef, ServerJourneyError> {
            let encoded = EncodedVariant::from_json(value)
                .map_err(|violation| format!("the payload does not encode: {violation:?}"))?;
            let mut builder = VariantColumnBuilder::with_capacity(1);
            builder.append(&encoded);
            Ok(builder.finish())
        };
        let utc = || DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into()));
        let now = Utc::now().timestamp_micros();
        let at = || Arc::new(TimestampMicrosecondArray::from(vec![now]).with_timezone("UTC"));
        let text = |value: &str| Arc::new(StringArray::from(vec![value])) as ArrayRef;
        let mut fields = AgentTracesTable::arrow_fields();
        fields.push(Field::new(CARD_REF, DataType::Utf8, true));
        fields.push(Field::new(WYRD_EVENT_TIME, utc(), false));
        let uidless = CardRef {
            uid: None,
            ..card_ref
        };
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(fields)),
            vec![
                text(&session),
                text("wyrd"),
                Arc::new(StringArray::from(vec![None::<&str>])),
                text("main"),
                text("typed-run"),
                Arc::new(FixedSizeBinaryArray::new_null(16, 1)),
                Arc::new(FixedSizeBinaryArray::new_null(8, 1)),
                text("assistant"),
                text("gpt-4o"),
                text("openai"),
                variant(&messages)?,
                variant(&tool_io)?,
                at(),
                at(),
                text(&uidless.to_string()),
                at(),
            ],
        )?;
        let writer = wyrd_client::bifrost::client_from_options(
            self.server.base_url(),
            Some(api_key.expose_secret()),
            self.server.grpc_url().as_deref(),
        )?;
        Bifrost::connect(&writer)
            .await?
            .write_batch("vala.dev.agent_traces", &batch)
            .await?;
        Ok(WrittenTrace {
            session,
            messages,
            tool_io,
        })
    }

    /// Append one allowed decision with a structured detail through the
    /// production staging writer; the server's publisher retains it.
    ///
    /// # Errors
    /// Returns the Postgres failure the append or commit raised.
    async fn append_audit_decision(&self) -> Result<AppendedDecision, ServerJourneyError> {
        let operation = format!("journey.typed_detail.{}", uuid::Uuid::now_v7().simple());
        let detail = AuditDetail::OracleAdmissionRecovery {
            expired_lease_count: 2,
            active_lease_count: 1,
            interactive_slots: 3,
            analytical_slots: 4,
            total_slots: 7,
        };
        let event = AuditEvent::new(
            RequestId::now_v7(),
            None,
            operation,
            "vala.oracle.admission".to_owned(),
            None,
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKindTag::User,
            "bifrost:query:read".to_owned(),
            AuditOutcome::Allowed,
        )
        .with_detail(detail.clone());
        let mut conn = self.server.tenant_conn_for(self.tenant).await?;
        append_audit(&mut conn, &event).await?;
        conn.commit().await?;
        Ok(AppendedDecision {
            event,
            detail: serde_json::from_str(&audit_detail_canonical_json(&detail))?,
        })
    }

    /// Every row of `sql` over published data, each cell as JSON.
    ///
    /// The read holds gateway payload-read authority beside the query read,
    /// so the sensitive gateway payload columns are reachable.
    ///
    /// # Errors
    /// Returns the context, query, or decode error.
    async fn rows(&self, sql: String) -> Result<Vec<Vec<Value>>, ServerJourneyError> {
        let permission = Permission::bifrost_query_read();
        let context = AuthorizedQueryContext::try_new(
            Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                self.tenant,
                Vec::new(),
                PermissionSet::from_iter([permission.clone(), Permission::gateway_payload_read()]),
            ),
            self.tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            permission,
        )?;
        let mut batches = Vec::new();
        ScheduledQueryCaller::new(
            self.server.state().clone(),
            context,
            CancellationToken::new(),
        )
        .run_with(
            BifrostQueryRequest {
                sql,
                deadline_ms: Some(30_000),
            },
            |batch| {
                batches.push(batch);
                Ok(())
            },
        )
        .await?;
        let Some(schema) = batches.first().map(RecordBatch::schema) else {
            return Ok(Vec::new());
        };
        let mut bytes = Vec::new();
        let mut writer = WriterBuilder::new()
            .with_explicit_nulls(true)
            .with_encoder_factory(Arc::new(VariantJsonEncoderFactory))
            .build::<_, JsonArray>(&mut bytes);
        for batch in &batches {
            writer.write(batch)?;
        }
        writer.finish()?;
        drop(writer);
        let objects: Vec<serde_json::Map<String, Value>> = serde_json::from_slice(&bytes)?;
        Ok(objects
            .into_iter()
            .map(|mut object| {
                schema
                    .fields()
                    .iter()
                    .map(|field| object.remove(field.name()).unwrap_or(Value::Null))
                    .collect()
            })
            .collect())
    }
}
