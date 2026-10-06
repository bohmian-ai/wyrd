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
//! Oracle under the tenant's SYSTEM Drift reader and audited there.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Array, Float64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use chrono::{DateTime, Datelike as _, Utc};
use parquet::file::reader::{FileReader, SerializedFileReader};
use secrecy::ExposeSecret;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use vala_eval::executor::{EvalReport, SkipReason, TaskRunOutcome};
use wyrd_client::Bifrost;
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::engines::{EngineOutcome, VerifierReport};
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::runner::EngineScript;
use wyrd_spec::DataTenantId;
use wyrd_spec::ids::{BindingId, CardUid, VerificationResultId, VerificationRunId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::eval::TaskId;
use wyrd_spec::vala::managed_columns::{
    CARD_REF, CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT,
};
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
    // The Oracles boot before the Scribe joins, so their membership must
    // include it before a read can see the decision still in its live tail.
    cluster.refresh_oracle_snapshots().await?;
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
    // The runner node hosts no Oracle, so the peer that serves the read
    // stages its decision; the cluster barrier drains every Oracle first.
    cluster.await_audit_published(tenant).await?;
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
    cluster.await_audit_published(tenant).await?;
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
