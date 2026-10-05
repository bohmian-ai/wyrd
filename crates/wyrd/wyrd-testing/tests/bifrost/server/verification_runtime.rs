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
//! producer's native JSON.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{
    Array, ArrayRef, AsArray, FixedSizeBinaryArray, Float64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::{DataType, Field, Float64Type, Int32Type, Int64Type, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Datelike as _, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use url::Url;
use vala_bifrost_redux::oracle::AuthorizedQueryContext;
use vala_bifrost_redux::tables::{AgentTracesTable, DomainTable};
use vala_eval::executor::{EvalReport, SkipReason, TaskRunOutcome};
use vala_sql::queries::audit_staging::append_audit;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_client::Bifrost;
use wyrd_queue::variant::{
    EncodedVariant, VariantColumnBuilder, variant_cell_to_json, variant_storage_type,
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
use wyrd_spec::ids::{BindingId, CardUid, VerificationResultId, VerificationRunId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome, AuthMethod, BifrostQueryRequest};
use wyrd_spec::vala::eval::TaskId;
use wyrd_spec::vala::eval::operator::ComparisonOperator;
use wyrd_spec::vala::eval::result::AssertionResult;
use wyrd_spec::vala::managed_columns::{
    CARD_REF, CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
};
use wyrd_spec::vala::{AuditDetail, audit_detail_canonical_json};
use wyrd_spec::verification::{DriftWindow, VerificationVerdict};
use wyrd_sql::queries::verifier_runs::RunInput;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
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
/// neither. The retained audit row's `entry_hash` is recomputed from the
/// stored columns, with the decoded detail re-canonicalized, and matches.
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
    journey.server.flush_bifrost().await?;
    journey.server.await_audit_published(journey.tenant).await?;

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

    // Audit: the stored detail decodes to the hashed JSON, and the stored
    // columns reproduce the row's own entry hash.
    let audit = journey
        .rows(format!(
            "SELECT {} FROM vala.system.audit_log WHERE operation = '{}'",
            RetainedAuditRow::COLUMNS,
            decision.operation
        ))
        .await?;
    let [audit] = audit.as_slice() else {
        return Err(format!("one retained decision expected: {audit:?}").into());
    };
    let audit = RetainedAuditRow::from_row(audit)?;
    expect_eq("audit detail", &audit.detail, &decision.detail)?;
    expect_eq(
        "audit entry hash",
        &audit.recomputed_entry_hash()?,
        &audit.entry_hash,
    )?;

    // Canonical signal payloads (OTLP attributes on vala.traces.spans,
    // vala.logs.records, and the metrics tables) are asserted here once those
    // tables store Variant attributes.

    journey.server.shutdown().await?;
    Ok(())
}

/// Fail with `what` and both values unless `actual` equals `expected`.
///
/// # Errors
/// Returns a description naming `what` when the values differ.
fn expect_eq<T: PartialEq + std::fmt::Debug>(
    what: &str,
    actual: &T,
    expected: &T,
) -> Result<(), ServerJourneyError> {
    if actual == expected {
        return Ok(());
    }
    Err(format!("{what} differs:\n  stored   {actual:?}\n  expected {expected:?}").into())
}

/// Render one query cell as the JSON value it carries.
///
/// A Variant cell decodes to its JSON value, a Struct becomes an object of
/// its children, text becomes a string, and integers and floats become
/// numbers. A null cell is JSON `null`.
///
/// # Errors
/// Returns an error for a Variant cell that does not decode or a column type
/// this journey does not read.
fn cell_json(column: &dyn Array, row: usize) -> Result<Value, ServerJourneyError> {
    if column.is_null(row) {
        return Ok(Value::Null);
    }
    if column.data_type() == &variant_storage_type() {
        return variant_cell_to_json(column, row)
            .map_err(|violation| format!("a Variant cell does not decode: {violation:?}").into());
    }
    Ok(match column.data_type() {
        DataType::Utf8 => json!(column.as_string::<i32>().value(row)),
        DataType::Int32 => json!(column.as_primitive::<Int32Type>().value(row)),
        DataType::Int64 => json!(column.as_primitive::<Int64Type>().value(row)),
        DataType::Float64 => json!(column.as_primitive::<Float64Type>().value(row)),
        DataType::Struct(fields) => {
            let children = column.as_struct();
            let mut object = serde_json::Map::new();
            for (field, child) in fields.iter().zip(children.columns()) {
                object.insert(field.name().clone(), cell_json(child.as_ref(), row)?);
            }
            Value::Object(object)
        }
        other => return Err(format!("the journey reads no {other} column").into()),
    })
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

/// The captured gateway call and the JSON it sent and received.
struct CapturedCall {
    /// Request id the gateway answered with, which the captured row carries.
    request_id: String,
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
    /// Operation name unique to this journey.
    operation: String,
    /// The detail's canonical JSON, the value the entry hash covers.
    detail: Value,
}

/// One retained `vala.system.audit_log` row, decoded to JSON values.
///
/// Owns the stored hash-chain inputs so the entry hash can be recomputed from
/// exactly what retained history stores.
#[derive(Debug)]
struct RetainedAuditRow {
    /// The row's stored columns, in [`Self::COLUMNS`] order.
    cells: Vec<Value>,
    /// The stored `entry_hash` as lowercase hex.
    entry_hash: Value,
    /// The stored detail, decoded from its Variant.
    detail: Value,
}

impl RetainedAuditRow {
    /// Projection that reads every hash input plus the stored entry hash.
    const COLUMNS: &'static str = "seq, prev_hash, request_id, trace_id, operation, resource, \
         audit_card_ref, audit_principal_id, principal_kind, permission, outcome, detail, \
         credential_id, entry_hash";

    /// Take one decoded row in [`Self::COLUMNS`] order.
    ///
    /// # Errors
    /// Returns an error when the row does not have every projected column.
    fn from_row(row: &[Value]) -> Result<Self, ServerJourneyError> {
        let [.., detail, _, entry_hash] = row else {
            return Err(format!("an audit row is missing columns: {row:?}").into());
        };
        if row.len() != 14 {
            return Err(format!("an audit row has {} columns: {row:?}", row.len()).into());
        }
        Ok(Self {
            cells: row.to_vec(),
            entry_hash: entry_hash.clone(),
            detail: detail.clone(),
        })
    }

    /// Recompute the SHA-256 entry hash from the stored columns.
    ///
    /// The preimage is the previous hash bytes, the big-endian sequence, then
    /// each field length-prefixed in staging order: request id, optional
    /// trace id, operation, resource, optional card ref, the principal UUID
    /// bytes, principal kind, permission, outcome, the optional detail's
    /// canonical JSON, and the optional credential id. An optional field is
    /// a `0` byte when absent and a `1` byte before its length-prefixed text.
    ///
    /// # Errors
    /// Returns an error when a stored column has the wrong JSON type, the
    /// previous hash is not hex, or the principal is not a UUID.
    fn recomputed_entry_hash(&self) -> Result<Value, ServerJourneyError> {
        let text = |index: usize| -> Result<&str, ServerJourneyError> {
            self.cells[index]
                .as_str()
                .ok_or_else(|| format!("audit column {index} is not text: {:?}", self.cells).into())
        };
        let optional = |index: usize| -> Result<Option<&str>, ServerJourneyError> {
            if self.cells[index].is_null() {
                return Ok(None);
            }
            text(index).map(Some)
        };
        let seq = self.cells[0]
            .as_i64()
            .ok_or("the audit seq is not an integer")?;
        let detail = if self.detail.is_null() {
            None
        } else {
            Some(serde_jcs::to_string(&self.detail)?)
        };
        let mut preimage = hex::decode(text(1)?)?;
        preimage.extend_from_slice(&seq.to_be_bytes());
        push_text(&mut preimage, text(2)?);
        push_optional(&mut preimage, optional(3)?);
        push_text(&mut preimage, text(4)?);
        push_text(&mut preimage, text(5)?);
        push_optional(&mut preimage, optional(6)?);
        preimage.extend_from_slice(uuid::Uuid::parse_str(text(7)?)?.as_bytes());
        push_text(&mut preimage, text(8)?);
        push_text(&mut preimage, text(9)?);
        push_text(&mut preimage, text(10)?);
        push_optional(&mut preimage, detail.as_deref());
        push_optional(&mut preimage, optional(12)?);
        Ok(json!(hex::encode(Sha256::digest(&preimage))))
    }
}

/// Append `value` to an audit hash preimage with its big-endian `u64` length.
fn push_text(preimage: &mut Vec<u8>, value: &str) {
    preimage.extend_from_slice(&(value.len() as u64).to_be_bytes());
    preimage.extend_from_slice(value.as_bytes());
}

/// Append an optional audit hash input: `0` when absent, else `1` and the
/// length-prefixed text.
fn push_optional(preimage: &mut Vec<u8>, value: Option<&str>) {
    match value {
        None => preimage.push(0),
        Some(value) => {
            preimage.push(1);
            push_text(preimage, value);
        }
    }
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
    /// upstream that answers every chat completion.
    ///
    /// # Errors
    /// Returns a mock URL or server start error.
    async fn start() -> Result<Self, ServerJourneyError> {
        let upstream = MockServer::start().await;
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
        Ok(CapturedCall {
            request_id,
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
            operation.clone(),
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
            operation,
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
        let mut rows = Vec::new();
        for batch in &batches {
            for row in 0..batch.num_rows() {
                rows.push(
                    batch
                        .columns()
                        .iter()
                        .map(|column| cell_json(column.as_ref(), row))
                        .collect::<Result<_, _>>()?,
                );
            }
        }
        Ok(rows)
    }
}
