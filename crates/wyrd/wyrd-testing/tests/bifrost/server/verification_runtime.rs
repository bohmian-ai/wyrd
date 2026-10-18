//! Verification runtime journey on a role-separated cluster.
//!
//! The runner lives on an Oracle-only node with no local Scribe, so the only
//! way its result can reach Bifrost is the gateway capture writer's peer
//! ingest RPC to the Scribe node. The journey schedules one
//! binding-created run, lets the runtime execute and publish a non-empty Drift
//! result, and reads the summary and its feature rows back through the
//! server's own query entry, asserting every identity they carry. A second
//! journey runs the production Drift engine on the Scribe-only node, which
//! hosts no Oracle, so its observation read must be forwarded to a peer
//! Oracle under the tenant's tokenless SYSTEM read authority and audited
//! there. A third
//! journey proves every built-in typed payload — verification summaries, drift
//! features, gateway payloads, and audit detail — reads back as the producer's
//! native JSON. A fourth journey sends raw Arrow IPC with malformed Variant
//! columns to built-ins and proves each is refused before any ACK.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use wyrd_client::bifrost::BifrostOptions;

use arrow::array::{
    Array, ArrayRef, AsArray, BinaryArray, Float64Array, Int32Array, ListArray, StringArray,
    StructArray, TimestampMicrosecondArray,
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
use vala_bifrost_redux::catalog::{TableRef, TableUid};
use vala_bifrost_redux::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use vala_bifrost_redux::oracle::AuthorizedQueryContext;
use vala_bifrost_redux::scribe::ScribeImpl;
use wyrd_client::Bifrost;
use wyrd_client::bifrost::TableConfig;
use wyrd_queue::variant::{EncodedVariant, WyrdJsonEncoderFactory};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::scribe_outbox::{ScribeSink, ScribeWrite};
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::engines::EngineOutcome;
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::runner::EngineScript;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId, PrincipalKindTag};
use wyrd_spec::card::verifier::Check;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, VerificationResultId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, AuthMethod, BifrostQueryRequest, VARIANT_MAX_DEPTH,
    VARIANT_MAX_ENCODED_BYTES,
};
use wyrd_spec::vala::managed_columns::{
    ACT_PRINCIPAL_ID, CARD_UID, RUN_ID, SUB_PRINCIPAL_ID, WYRD_INGESTED_AT,
};
use wyrd_spec::vala::{AuditDetail, BifrostError, audit_detail_canonical_json};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    RequirementDetail, RequirementOutcome, RequirementResult, VerificationOutcome,
};
use wyrd_spec::verification::{DriftReport, DriftVerdict, DriftWindow, VerificationVerdict};
use wyrd_testing::bifrost::{BifrostClusterSpec, RawIngest, WyrdTestCluster, canonical_signals};
use wyrd_testing::gateway_capture::GatewayCapture;
use wyrd_testing::verification::{ExecutionRow, VerificationFixture};
use wyrd_testing::{Bootstrap, WyrdTestServer};
use wyrd_types::variant::{variant_field, variant_storage_type};

use super::query::{ServerJourneyError, audit_rows, scheduled_context};

/// Upper bound on the wait for the remote runner to settle the run.
const WAIT: Duration = Duration::from_mins(1);

/// One day, the receipt-clock shift that moves an ACK onto the next UTC day.
const DAY: Duration = Duration::from_hours(24);

/// A Custom Drift report that scored `latency` as drifting and `tokens` as
/// steady, the two-feature detail every published Drift result here carries.
///
/// # Errors
/// Returns a decode error when the fixed report does not deserialize.
fn two_feature_report() -> Result<DriftReport, ServerJourneyError> {
    Ok(serde_json::from_value(json!({
        "method": "Custom",
        "features": {
            "latency": { "feature": "latency", "score": 3.0, "threshold": 1.0, "verdict": "Drift" },
            "tokens": { "feature": "tokens", "score": 0.1, "threshold": 1.0, "verdict": "NoDrift" }
        },
        "verdict": "Drift"
    }))?)
}

/// A one-requirement outcome whose `drift` requirement carries `report`.
///
/// A drifting report fails the requirement and the outcome, a steady one
/// passes both, and no report leaves both inconclusive with no detail, as an
/// unscorable window does. The requirement's `check` is a stand-in; result
/// publication reads only its detail.
///
/// # Errors
/// Returns an error when the requirement id or check does not decode.
fn drift_outcome(report: Option<DriftReport>) -> Result<VerificationOutcome, ServerJourneyError> {
    let (outcome, verdict) = match report.as_ref().map(|report| report.verdict) {
        Some(DriftVerdict::Drift) => (RequirementOutcome::Failed, VerificationVerdict::Failed),
        Some(DriftVerdict::NoDrift) => (RequirementOutcome::Passed, VerificationVerdict::Passed),
        _ => (
            RequirementOutcome::Inconclusive,
            VerificationVerdict::Inconclusive,
        ),
    };
    Ok(VerificationOutcome {
        verdict,
        requirements: vec![RequirementResult {
            id: TaskId::new("drift")?,
            step: None,
            statement: "The window does not drift.".to_owned(),
            outcome,
            reason: "scored by the journey".to_owned(),
            actual: None,
            citations: Vec::new(),
            detail: report.map(RequirementDetail::Drift),
            check: serde_json::from_value::<Check>(json!({
                "kind": "assert", "path": "inputs.x", "operator": "equals", "expected": true
            }))?,
            provenance: None,
        }],
        steps: Vec::new(),
        evidence: std::collections::BTreeMap::new(),
    })
}

/// Poll `seed` until its first execution leaves every status in `waiting`,
/// returning that execution and its settled row.
///
/// # Errors
/// Returns a fixture error, or a description when no execution settles
/// within [`WAIT`].
async fn first_settled(
    seed: &VerificationFixture,
    waiting: &[&str],
) -> Result<(ExecutionId, ExecutionRow), ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        if let Some(execution) = seed.executions().await?.first().copied() {
            let row = seed.execution(execution).await?;
            if !waiting.contains(&row.status.as_str()) {
                return Ok((execution, row));
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("no execution settled".into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A runner on a node without local Scribe publishes a binding-created Drift
/// result through the Scribe outbox's peer ingest RPC and completes
/// the run. Its summary and both feature rows are queryable through the
/// Oracle and carry the exact tenant SYSTEM principal, Verifier, subject,
/// owner, binding, run, result, and one shared event time.
///
/// # Errors
/// Returns cluster, seeding, runtime, publication, or query errors, or a
/// description of the first identity that does not match.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn runner_without_local_scribe_publishes_through_a_peer_scribe()
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
    script.push(EngineOutcome::Completed(drift_outcome(Some(
        two_feature_report()?,
    ))?));
    let runtime = VerificationRuntime::builder(oracle.state())
        .engine_script(script)
        .build()
        .ok_or("the runtime did not compose")?;
    if !runtime.composes(RuntimeCapability::Runner) {
        return Err("a node reaching a peer Scribe did not compose the runner".into());
    }
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));

    let (_, row) = first_settled(&seed, &["pending", "running"]).await?;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    let ("completed", Some(result)) = (row.status.as_str(), row.result_id) else {
        return Err(format!("the run settled {row:?}").into());
    };

    // The runner stages its result on its own outbox after settling the run;
    // it reaches the peer Scribe only once that outbox drains.
    let unwritten = oracle
        .state()
        .scribe_outbox
        .settle(std::time::Instant::now() + WAIT)
        .await;
    if unwritten != 0 {
        return Err(format!("{unwritten} results never reached the peer Scribe").into());
    }
    scribe.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    let system = seed.system_principal();
    let identity = |alias: &str| {
        format!(
            "{alias}{RUN_ID} IS NULL AND {alias}result_id = '{result}' \
             AND {alias}{ACT_PRINCIPAL_ID} = '{system}' AND {alias}verifier_card_uid = '{verifier}' \
             AND {alias}{CARD_UID} = '{subject}' AND {alias}owner_card_uid = '{owner}' \
             AND {alias}binding_id = '{binding}'"
        )
    };
    let checks = [
        (
            format!("SELECT result_id FROM vala.verification.results WHERE result_id = '{result}'"),
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
            format!(
                "SELECT result_id FROM vala.drift.result_features WHERE result_id = '{result}'"
            ),
            2,
            "two published feature rows for the run",
        ),
        (
            format!(
                "SELECT f.result_id FROM vala.drift.result_features f \
                 JOIN vala.verification.results r \
                   ON f.result_id = r.result_id \
                  AND f.{ACT_PRINCIPAL_ID} = r.{ACT_PRINCIPAL_ID} AND f.{CARD_UID} = r.{CARD_UID} \
                 WHERE {}",
                identity("f.")
            ),
            2,
            "every feature joins its summary on result, run, writer, and Verifier",
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

/// A runner on a pod without a local Oracle completes a real scheduled Drift
/// execution.
///
/// The Scribe-only node composes the runtime over its own Scribe. A due
/// monthly binding's Custom Verifier reads its `score` samples under the
/// binding owner's recovered authority with SYSTEM as actor; the read is
/// forwarded to a peer Oracle, which records one audited read decision, and
/// the execution completes with a published result on its first attempt.
/// The registered table is empty for the subject, so the result is
/// inconclusive.
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
    let (owner, owner_principal) = seed.service("forwarded-owner").await?;
    let verifier = seed
        .custom_drift_verifier("forwarded-drift", "score", 1.0, 0.5)
        .await?;
    scribe
        .ensure_builtin_table_for_test(tenant, "drift", "observations")
        .await?;
    let binding = seed
        .bind_schedule(&owner, &subject, &verifier, MONTHLY, Vec::new())
        .await?;
    seed.activate(owner_principal).await?;
    seed.make_binding_due(binding).await?;
    // The runner node hosts no Oracle, so the peer that serves the read
    // stages its decision; the cluster barrier drains every Oracle first.
    cluster.await_audit_retained().await?;
    let reads = audit_rows(scribe, tenant, "bifrost.query.read_decision").await?;

    let runtime = VerificationRuntime::builder(scribe.state())
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let (_, row) = first_settled(&seed, &["pending", "running", "retrying"]).await?;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    if row.status != "completed" || row.result_id.is_none() || row.attempts != 1 {
        return Err(format!("the forwarded Drift run settled {row:?}").into());
    }
    cluster.await_audit_retained().await?;
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
/// publishes through a peer Scribe. The observation table then holds
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
    Bifrost::connect_with(&writer, BifrostOptions::default())
        .await?
        .write_batch(
            "vala.drift.observations",
            &observation_batch(&record, &subject, observed_in_current_month(Utc::now())?)?,
        )
        .await?;
    seed.activate(client).await?;
    for (_, binding) in &bindings {
        seed.make_binding_due(*binding).await?;
    }
    scribe.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let runtime = VerificationRuntime::builder(oracle.state())
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let executions = seed.executions().await?;
        let mut settled = 0;
        for execution in &executions {
            let row = seed.execution(*execution).await?;
            if !matches!(row.status.as_str(), "pending" | "running" | "retrying") {
                if row.status != "completed" || row.result_id.is_none() {
                    return Err(format!("a binding execution settled {row:?}").into());
                }
                settled += 1;
            }
        }
        if executions.len() == 2 && settled == 2 {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(
                format!("expected two settled binding executions, saw {executions:?}").into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    // Each run stages its result on the Oracle node's outbox after settling;
    // the results reach the peer Scribe only once that outbox drains.
    let unwritten = oracle
        .state()
        .scribe_outbox
        .settle(std::time::Instant::now() + WAIT)
        .await;
    if unwritten != 0 {
        return Err(format!("{unwritten} results never reached the peer Scribe").into());
    }
    scribe.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let observed = query_texts(
        oracle,
        tenant,
        format!(
            "SELECT {SUB_PRINCIPAL_ID}, {CARD_UID}, series FROM vala.drift.observations \
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
    for column in ["binding_id", "verifier_card_uid", "result_id"] {
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
                "SELECT verifier_card_uid, {CARD_UID}, verdict, {RUN_ID} \
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

/// One Drift observation `record` of the Card `card_uid` at `event_time`: a
/// drifting `score` and a `latency` feature, as the SDK's tall projection
/// writes them.
///
/// `event_time` is the rows' `created_at`, the observation table's time
/// column, which Scribe keeps verbatim.
///
/// # Errors
/// Returns an Arrow error when the batch cannot be assembled.
fn observation_batch(
    record: &str,
    card_uid: &CardUid,
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
        Field::new(CARD_UID, DataType::Utf8, true),
    ]));
    let event_time = event_time.timestamp_micros();
    Ok(RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![record, record])),
            Arc::new(StringArray::from(vec!["score", "latency"])),
            Arc::new(Float64Array::from(vec![3.0, 120.0])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
            Arc::new(StringArray::from(vec![None::<&str>, None])),
            Arc::new(
                TimestampMicrosecondArray::from(vec![event_time, event_time]).with_timezone("UTC"),
            ),
            Arc::new(StringArray::from(vec![
                card_uid.as_str(),
                card_uid.as_str(),
            ])),
        ],
    )?)
}

/// The analytical layout of verification results across UTC days.
///
/// The tenant SYSTEM writer publishes three results through the public
/// Bifrost facade, flushing after each so every result is its own file: a
/// Drift result whose feature rows are acknowledged on one receipt day and
/// whose summary is acknowledged on the next, a Drift result whose event time
/// is the previous UTC day, and an unscored result that writes its summary
/// only. Each result's details are
/// retrieved by `result_id` alone from both daily partitions; the straddling
/// result's summary partitions on its `ended_at` while its details partition
/// on a `wyrd_ingested_at` that falls on a different day. Every published Parquet file of
/// the two tables carries a physical Bloom filter for its declared and
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
    let unscored = journey.publish(now, Report::Unscored, false).await?;
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
    let none = journey
        .texts(format!(
            "SELECT feature FROM vala.drift.result_features WHERE result_id = '{unscored}'"
        ))
        .await?;
    if !none.is_empty() {
        return Err(format!("the unscored result wrote feature rows: {none:?}").into());
    }

    // The summary's time column is `ended_at`; the details declare none, so
    // they partition on their own receipt instant. Each projects its time
    // column first and its receipt instant second.
    let times = |time_column: &str, table: &str| {
        format!(
            "SELECT CAST({time_column} AS BIGINT) AS time, \
             CAST({WYRD_INGESTED_AT} AS BIGINT) AS ingested \
             FROM {table} WHERE result_id = '{straddling}'"
        )
    };
    let summary = journey
        .texts(times("ended_at", "vala.verification.results"))
        .await?;
    let details = journey
        .texts(times(WYRD_INGESTED_AT, "vala.drift.result_features"))
        .await?;
    let micros = |value: &Option<String>| -> Result<i64, ServerJourneyError> {
        Ok(value.as_deref().ok_or("null timestamp")?.parse()?)
    };
    let day = |micros: i64| micros.div_euclid(86_400_000_000);
    let [summary] = summary.as_slice() else {
        return Err(format!("one straddling summary expected, saw {summary:?}").into());
    };
    if micros(&summary[0])? != now.timestamp_micros() {
        return Err(format!(
            "the summary must partition on its own ended_at: {summary:?} ended {now}"
        )
        .into());
    }
    for detail in &details {
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
                 WHERE ended_at >= TIMESTAMP '{today}'"
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

/// Which outcome a published layout result maps.
#[derive(Debug, Clone, Copy)]
enum Report {
    /// A two-feature Drift outcome over a Drift window.
    Drift,
    /// An unscored Drift outcome, which publishes its summary only.
    Unscored,
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
/// Owns the identities every published result repeats — the subject, the
/// Verifier, and the tenant SYSTEM principal — and the server whose Scribe
/// the internal result writer reaches, so each publication, query, and
/// storage inspection reads them from one place.
struct ResultLayoutJourney<'a> {
    /// The cluster whose telemetry and storage root the journey inspects.
    cluster: &'a WyrdTestCluster,
    /// The mixed node hosting both Scribe and Oracle.
    server: &'a WyrdTestServer,
    /// Tenant every result belongs to.
    tenant: DataTenantId,
    /// The verified subject.
    subject: CardUid,
    /// The UID-bearing Verifier every result is attributed to.
    verifier: CardRef,
    /// The tenant SYSTEM principal every result is attributed to.
    system: PrincipalId,
}

impl<'a> ResultLayoutJourney<'a> {
    /// Provision the tenant's SYSTEM principal, a subject and a Verifier, and
    /// the two result tables.
    ///
    /// # Errors
    /// Returns a seeding or provisioning error.
    async fn start(cluster: &'a WyrdTestCluster) -> Result<Self, ServerJourneyError> {
        let tenant = cluster.data_tenant_id();
        let server = cluster.server(0).ok_or("missing mixed node")?;
        let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
        let (subject, _) = seed.service("layout-subject").await?;
        let uid = seed.drift_verifier("layout-drift").await?;
        let verifier: CardRef = format!("default/Verifier/layout-drift@1.0.0#{uid}").parse()?;
        for (namespace, name) in [("verification", "results"), ("drift", "result_features")] {
            server
                .ensure_builtin_table_for_test(tenant, namespace, name)
                .await?;
        }
        Ok(Self {
            cluster,
            server,
            tenant,
            subject,
            verifier,
            system: PrincipalId::new(seed.system_principal()),
        })
    }

    /// Publish one result that starts and ends at `event_time` through the
    /// production payload builder and a Scribe outbox, and flush it into its
    /// own file; returns its `result_id`. The summary's `ended_at` therefore
    /// names its partition.
    ///
    /// With `straddle`, Scribe's receipt clock moves one day ahead after the
    /// detail batch is acknowledged and before the summary is sent, so the
    /// two ACKs fall on different UTC receipt days; the shift is cleared
    /// before returning.
    ///
    /// # Errors
    /// Returns a payload or flush error, or an error when the outbox loses
    /// the staged result.
    async fn publish(
        &self,
        event_time: DateTime<Utc>,
        report: Report,
        straddle: bool,
    ) -> Result<VerificationResultId, ServerJourneyError> {
        let window = DriftWindow {
            start: event_time - chrono::Duration::hours(1),
            end: event_time,
        };
        let outcome = drift_outcome(match report {
            Report::Drift => Some(two_feature_report()?),
            Report::Unscored => None,
        })?;
        let verifier_uid = self
            .verifier
            .uid
            .as_ref()
            .ok_or("the Verifier has no UID")?;
        let payload = ResultPayloadBuilder::new(
            ResultRun {
                execution_id: ExecutionId::new_v7(),
                run_id: None,
                verifier_version: "1.0.0",
                card_uid: &self.subject,
                owner_card_uid: Some(&self.subject),
                group_id: None,
                binding_id: Some(BindingId::new_v7()),
                trigger: None,
                window: Some(&window),
            },
            verifier_uid,
            event_time,
            event_time,
        )
        .build(&outcome)?;
        let result = payload.result_id();
        let scribe = self
            .server
            .bifrost_scribe()
            .ok_or("the mixed node owns no Scribe")?;
        let outbox = ScribeSink::local_outbox(Arc::new(StraddleScribe {
            inner: Arc::clone(&scribe),
            straddle,
        }));
        outbox.stage(
            self.tenant,
            ScribeWrite::Result {
                payload,
                attribution: Attribution::direct(self.system),
            },
        );
        let lost = outbox
            .shutdown(std::time::Instant::now() + Duration::from_secs(30))
            .await;
        if lost != 0 {
            return Err("the staged result was never written".into());
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
    /// Bloom column in every published file of the two result tables.
    ///
    /// Files are found under the cluster's storage root and classified by a
    /// column only their table carries; each table must have at least one.
    ///
    /// # Errors
    /// Returns a filesystem or Parquet error, or names the first table with
    /// no file or the first column chunk without a Bloom filter.
    fn assert_blooms(&self) -> Result<(), ServerJourneyError> {
        let floor = [RUN_ID, CARD_UID, SUB_PRINCIPAL_ID, ACT_PRINCIPAL_ID];
        let tables: [(&str, &str, &[&str]); 2] = [
            (
                "vala.verification.results",
                "execution_status",
                &["result_id", "verifier_card_uid", "binding_id"],
            ),
            ("vala.drift.result_features", "feature", &["result_id"]),
        ];
        let mut found = [0_usize; 2];
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

/// Scribe decorator that moves the receipt clock one day ahead just before
/// the summary frame, so a straddling result's detail and summary ACKs fall
/// on different UTC receipt days.
struct StraddleScribe {
    /// The mixed node's real Scribe.
    inner: Arc<ScribeImpl>,
    /// Whether to shift the receipt clock before the summary.
    straddle: bool,
}

#[async_trait::async_trait]
impl Scribe for StraddleScribe {
    /// Mirrors the real Scribe's readiness.
    fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Shifts the receipt clock before a straddled summary, then forwards
    /// `frame`.
    ///
    /// # Errors
    /// Returns the real Scribe's outcome.
    async fn ingest_frame(&self, frame: ScribeIngressFrame) -> Result<FrameAdmission, ScribeError> {
        if self.straddle && frame.table.fqn() == "vala.verification.results" {
            self.inner.shift_receipt_clock_for_test(DAY);
        }
        self.inner.ingest_frame(frame).await
    }

    /// Forwards table resolution to the real Scribe.
    ///
    /// # Errors
    /// Returns the real Scribe's resolution error.
    async fn resolve_write_table(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> Result<TableUid, ScribeError> {
        self.inner.resolve_write_table(tenant, table).await
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
            params: Vec::new(),
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
        params: Vec::new(),
        sql,
        deadline_ms: Some(30_000),
    })
    .await?;
    Ok(outcome.rows)
}

/// Built-in payloads keep their JSON types end to end through the real server.
///
/// One bound server takes every built-in producer whose payload is a typed
/// column: the production result builder publishes a scored and an unscored
/// Drift outcome as the tenant SYSTEM writer; a real gateway call is captured
/// with its request and response payloads; and an audit decision with a
/// structured detail is staged on the Scribe outbox and retained. Each
/// summary's `outcome` Variant decodes to the producer's native outcome JSON
/// both before and after the flush, and its `verdict` column names the
/// outcome's verdict. A second gateway call the upstream refuses is captured
/// with no resolved model: hot and published, both its `resolved_model`
/// children read SQL null while the resolved call's read its provider and
/// model. The retained audit row's detail decodes to the decision's canonical
/// JSON.
///
/// # Errors
/// Returns server, seeding, gateway, publication, or query errors, or a
/// description of the first value that does not match its native source.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn typed_builtin_payloads_are_queryable() -> Result<(), ServerJourneyError> {
    let journey = TypedPayloadJourney::start().await?;

    let results = journey.publish_results().await?;
    let call = journey.gateway.capture_call().await?;
    let decision = journey.stage_audit_decision().await?;
    let outcomes_sql = format!(
        "SELECT result_id, verdict, outcome FROM vala.verification.results \
         WHERE result_id IN ('{}', '{}') ORDER BY result_id",
        results.scored_drift.0, results.unscored_drift.0
    );
    let mut expected_outcomes = [&results.scored_drift, &results.unscored_drift]
        .map(|(result, outcome)| {
            Ok::<_, ServerJourneyError>(vec![
                json!(result.to_string()),
                json!(<&'static str>::from(outcome.verdict)),
                serde_json::to_value(outcome)?,
            ])
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    expected_outcomes.sort_by_key(|row| row[0].to_string());
    expect_eq(
        "hot verification outcomes",
        &journey.rows(outcomes_sql.clone()).await?,
        &expected_outcomes,
    )?;
    // Gateway capture: the resolved call carries both model Structs whole,
    // and the refused call's absent resolved model reads both children null.
    let models_sql = format!(
        "SELECT wyrd_request_id, requested_model['deployment'], requested_model['model'], \
         resolved_model['deployment'], resolved_model['model'] FROM vala.gateway.calls \
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
    journey.gateway.server.flush_bifrost().await?;
    expect_eq(
        "published verification outcomes",
        &journey.rows(outcomes_sql).await?,
        &expected_outcomes,
    )?;
    expect_eq(
        "published gateway model children",
        &journey.rows(models_sql).await?,
        &expected_models,
    )?;

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

    // Audit: the retained detail decodes to the decision's canonical JSON.
    let audit = journey
        .rows(format!(
            "SELECT detail FROM vala.system.audit_log WHERE operation = '{}'",
            decision.operation
        ))
        .await?;
    expect_eq("audit detail", &audit, &vec![vec![decision.detail]])?;

    journey.gateway.server.shutdown().await?;
    Ok(())
}

/// Raw Arrow IPC that bypasses every client-side Variant check reaches Scribe
/// admission for a top-level and a nested signal Variant, and each malformed
/// frame is refused with its exact catalogued error before any ACK.
///
/// `vala.logs.records` covers a missing and a foreign extension marker,
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
/// sits inside the over-deep branch, before any ACK; that JSON row encodes
/// against `vala.verification.execution_inputs` on the client.
/// `vala.traces.spans` covers the Variant nested in `events`, with a missing
/// marker and with foreign extension metadata, named by its top-level column.
/// A non-Variant type change on a signal column is refused as an unsupported
/// type. Afterward the same server accepts the valid sentinel frames and only
/// they are readable, so no refused frame persisted a row; the accepted log
/// record's scale-zero `u64::MAX` decimal reads back with every digit.
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
    let inputs = || variant_field("attributes", false);
    let activation = || variant_field("body", true);
    let limit = usize::try_from(VARIANT_MAX_ENCODED_BYTES)?;
    let unsupported = |field: &str| BifrostError::UnsupportedType {
        field: field.to_owned(),
        data_type: variant_storage_type().to_string(),
    };
    let invalid_inputs = BifrostError::VariantInvalidJson {
        field: "attributes".to_owned(),
        row: 0,
        path: String::new(),
    };
    let out_of_range = |path: &str, numeric_kind: &str| BifrostError::VariantNumericOutOfRange {
        field: "attributes".to_owned(),
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
        field: "attributes".to_owned(),
        row: 0,
        path: "/0".repeat(usize::try_from(VARIANT_MAX_DEPTH)?),
        depth: VARIANT_MAX_DEPTH + 1,
        limit: VARIANT_MAX_DEPTH,
    };

    let frames = [
        (
            "missing extension",
            (storage_only("attributes", false), valid()?),
            unsupported("attributes"),
        ),
        (
            "foreign extension",
            (foreign("attributes"), valid()?),
            unsupported("attributes"),
        ),
        (
            "foreign extension metadata",
            (foreign_metadata("attributes"), valid()?),
            unsupported("attributes"),
        ),
        (
            "invalid bytes",
            (inputs(), invalid()?),
            invalid_inputs.clone(),
        ),
        (
            "one container past the depth limit",
            (
                inputs(),
                raw_variant(&EMPTY_METADATA, &nested_lists(VARIANT_MAX_DEPTH + 1)?)?,
            ),
            too_deep.clone(),
        ),
        (
            "a compact hostile depth",
            (
                inputs(),
                raw_variant(&EMPTY_METADATA, &nested_lists(20_000)?)?,
            ),
            too_deep.clone(),
        ),
        (
            "malformed bytes below a container past the depth limit",
            (
                inputs(),
                raw_variant(
                    &EMPTY_METADATA,
                    &(0..VARIANT_MAX_DEPTH + 5)
                        .try_fold(vec![VARIANT_MALFORMED], |value, _| variant_list(&[value]))?,
                )?,
            ),
            invalid_inputs.clone(),
        ),
        (
            "objects whose fields share one child at every level",
            (
                inputs(),
                raw_variant(&AB_METADATA, &shared_objects(VARIANT_MAX_DEPTH)?)?,
            ),
            invalid_inputs.clone(),
        ),
        (
            "malformed bytes after an over-deep sibling",
            (
                inputs(),
                raw_variant(
                    &EMPTY_METADATA,
                    &variant_list(&[over_deep.clone(), vec![VARIANT_MALFORMED]])?,
                )?,
            ),
            invalid_inputs.clone(),
        ),
        (
            "malformed bytes before an over-deep sibling",
            (
                inputs(),
                raw_variant(
                    &EMPTY_METADATA,
                    &variant_list(&[vec![VARIANT_MALFORMED], over_deep.clone()])?,
                )?,
            ),
            invalid_inputs.clone(),
        ),
        (
            "many fields sharing one large child",
            (inputs(), raw_variant(&many_names, &one_shared_child)?),
            invalid_inputs.clone(),
        ),
        (
            "one field starting inside another",
            (
                inputs(),
                raw_variant(
                    &AB_METADATA,
                    &variant_object(&[(0, 0), (1, 1)], &[0x0c, 0x00])?,
                )?,
            ),
            invalid_inputs.clone(),
        ),
        (
            "two fields resolving to one name",
            (
                inputs(),
                raw_variant(
                    &variant_metadata(&["a", "a"], false)?,
                    &variant_object(&[(0, 0), (1, 1)], &[VARIANT_NULL, VARIANT_NULL])?,
                )?,
            ),
            invalid_inputs.clone(),
        ),
        (
            "a Decimal4",
            (
                inputs(),
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
                inputs(),
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
                inputs(),
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
                inputs(),
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
                inputs(),
                raw_variant(&EMPTY_METADATA, &decimal16(12_345, 2))?,
            ),
            out_of_range("", "decimal"),
        ),
        (
            "a scale-zero decimal past u64::MAX",
            (
                inputs(),
                raw_variant(&EMPTY_METADATA, &decimal16(i128::from(u64::MAX) + 1, 0))?,
            ),
            out_of_range("", "decimal"),
        ),
        (
            "a scale-zero decimal within i64",
            (inputs(), raw_variant(&EMPTY_METADATA, &decimal16(5, 0))?),
            out_of_range("", "decimal"),
        ),
        (
            "a refused decimal after an over-deep sibling",
            (
                inputs(),
                raw_variant(
                    &EMPTY_METADATA,
                    &variant_list(&[over_deep.clone(), decimal16(5, 0)])?,
                )?,
            ),
            out_of_range("/1", "decimal"),
        ),
        (
            "an oversized malformed value reports its size",
            (inputs(), raw_variant(&[0xff], &vec![0xff; limit])?),
            BifrostError::VariantTooLarge {
                field: "attributes".to_owned(),
                row: 0,
                bytes: VARIANT_MAX_ENCODED_BYTES + 1,
                limit: VARIANT_MAX_ENCODED_BYTES,
            },
        ),
    ];
    for (what, inputs, expected) in frames {
        let frame = admission.records_frame(&refused, inputs, (activation(), valid()?))?;
        admission.refuse(what, RECORDS, &frame, &expected).await?;
    }
    let frame = admission.records_frame(
        &refused,
        (inputs(), invalid()?),
        (storage_only("body", true), valid()?),
    )?;
    admission
        .refuse(
            "a wrong marker outranks invalid bytes",
            RECORDS,
            &frame,
            &unsupported("body"),
        )
        .await?;
    let wide_severity = |frame: RecordBatch| -> Result<RecordBatch, ServerJourneyError> {
        let fields = frame
            .schema()
            .fields()
            .iter()
            .map(|field| match field.name().as_str() {
                "severity_number" => {
                    Arc::new(field.as_ref().clone().with_data_type(DataType::Int64))
                }
                _ => Arc::clone(field),
            })
            .collect::<Vec<_>>();
        let mut columns = frame.columns().to_vec();
        columns[frame.schema().index_of("severity_number")?] = cast(
            frame
                .column_by_name("severity_number")
                .ok_or("no severity_number")?,
            &DataType::Int64,
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
    let frame =
        admission.records_frame(&refused, (inputs(), valid()?), (activation(), valid()?))?;
    let retyped = BifrostError::UnsupportedType {
        field: "severity_number".to_owned(),
        data_type: DataType::Int64.to_string(),
    };
    admission
        .refuse(
            "a non-Variant type change",
            RECORDS,
            &wide_severity(frame)?,
            &retyped,
        )
        .await?;
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
            admission.records_frame(
                &refused,
                (inputs(), Arc::clone(&bytes)),
                (activation(), valid()?),
            )
        };
        admission
            .refuse(
                &format!("an undeclared column outranks {defect} Variant bytes"),
                RECORDS,
                &undeclared(frame()?)?,
                &BifrostError::UndeclaredField {
                    field: "undeclared".to_owned(),
                    row: 0,
                },
            )
            .await?;
        admission
            .refuse(
                &format!("a non-Variant type change outranks {defect} Variant bytes"),
                RECORDS,
                &wide_severity(frame()?)?,
                &retyped,
            )
            .await?;
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

    // The accepted log record carries the one decimal form the exact numeric
    // domain admits, which must read back with every digit, and a finite float.
    let accepted = admission.session("accepted");
    let sentinel = admission.records_frame(
        &accepted,
        (
            inputs(),
            raw_variant(&EMPTY_METADATA, &decimal16(i128::from(u64::MAX), 0))?,
        ),
        (
            activation(),
            raw_variant(
                &EMPTY_METADATA,
                &primitive(VARIANT_FLOAT, &1.5_f32.to_le_bytes()),
            )?,
        ),
    )?;
    admission.accept(RECORDS, &sentinel).await?;
    admission
        .accept(SPANS, &admission.spans_frame(&accepted))
        .await?;
    admission
        .accept(POINTS, &admission.points_frame(&accepted, false)?)
        .await?;
    admission.journey.gateway.server.flush_bifrost().await?;

    let suffix = &admission.run;
    expect_eq(
        "stored log records",
        &admission
            .journey
            .rows(format!(
                "SELECT scope_name, count(*) FROM {RECORDS} \
                 WHERE scope_name LIKE '%{suffix}' GROUP BY scope_name"
            ))
            .await?,
        &vec![vec![json!(accepted), json!(1)]],
    )?;
    expect_eq(
        "the accepted u64::MAX decimal and finite float",
        &admission
            .journey
            .rows(format!(
                "SELECT attributes, body FROM {RECORDS} \
                 WHERE scope_name = '{accepted}'"
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

    admission.journey.gateway.server.shutdown().await?;
    Ok(())
}

/// The client-writable built-in whose top-level Variants the journey writes:
/// a required `attributes` and a nullable `body`.
const RECORDS: &str = "vala.logs.records";

/// The server-written built-in the public JSON-row facade encodes against;
/// its Variant refusals surface on the client before anything is queued.
const EXECUTION_INPUTS: &str = "vala.verification.execution_inputs";

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
    /// The described `vala.logs.records` user schema.
    records: SchemaRef,
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
            ("logs", "records"),
            ("traces", "spans"),
            ("metrics", "points"),
        ] {
            journey
                .gateway
                .server
                .ensure_builtin_table_for_test(journey.gateway.tenant, namespace, name)
                .await?;
        }
        let Bootstrap::Machine { api_key, .. } = journey
            .gateway
            .server
            .bootstrap_service("variant_admission_writer", &["admin"])
            .await?
        else {
            return Err("a service bootstrap returned a user".into());
        };
        let client = wyrd_client::bifrost::client_from_options(
            journey.gateway.server.base_url(),
            Some(api_key.expose_secret()),
            journey.gateway.server.grpc_url().as_deref(),
        )?;
        let records = Arc::clone(
            TableConfig::describe_with(&client, RECORDS)
                .await?
                .user_schema(),
        );
        let spans = Arc::clone(
            TableConfig::describe_with(&client, SPANS)
                .await?
                .user_schema(),
        );
        let points = Arc::clone(
            TableConfig::describe_with(&client, POINTS)
                .await?
                .user_schema(),
        );
        Ok(Self {
            ingest: RawIngest::connect(&client).await?,
            client,
            journey,
            records,
            spans,
            points,
            run: uuid::Uuid::now_v7().simple().to_string(),
        })
    }

    /// Require that each JSON execution inputs row is refused before any ACK with
    /// the exact Variant error the revision-13 order selects.
    ///
    /// Numeric range outranks depth in both key orders and inside the
    /// over-depth container; valid JSON nested 129 and 10,000 levels deep
    /// is too deep rather than invalid; and an `inputs` value whose built
    /// bytes already exceed the size limit reports size beside a refused
    /// number or an over-depth sibling. Each row goes through the public
    /// facade's JSON-row path with its integer token intact, and the refusal
    /// surfaces from `insert_into` before the row is queued.
    ///
    /// # Errors
    ///
    /// Returns a connect or describe error, or a description when a row is
    /// accepted or refused with any other error.
    async fn refuse_json_rows(&self) -> Result<(), ServerJourneyError> {
        let nested = |inner: &str, levels: usize| {
            format!("{}{inner}{}", "[".repeat(levels), "]".repeat(levels))
        };
        let deep = nested("1", 65);
        let numeric = |path: String| BifrostError::VariantNumericOutOfRange {
            field: "inputs".to_owned(),
            row: 0,
            path,
            numeric_kind: "integer".to_owned(),
        };
        let too_deep = BifrostError::VariantTooDeep {
            field: "inputs".to_owned(),
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
        let too_large = |inputs: String| -> Result<(String, BifrostError), ServerJourneyError> {
            let error = EncodedVariant::from_json_text(&inputs)
                .err()
                .ok_or("the oversized row encodes")?
                .into_error("inputs", 0);
            expect_eq(
                "the oversized row's error",
                &error.code(),
                &"WYRD_VALA_413_VARIANT_TOO_LARGE",
            )?;
            Ok((inputs, error))
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
        for (inputs, expected) in cases {
            let bifrost = Bifrost::connect_with(&self.client, BifrostOptions::default()).await?;
            let table = bifrost.writer_table(EXECUTION_INPUTS).await?;
            let fields = table
                .user_schema()
                .fields()
                .iter()
                .map(|field| {
                    let value = match field.data_type() {
                        _ if field.name() == "inputs" => inputs.clone(),
                        _ if field.is_nullable() => "null".to_owned(),
                        DataType::Boolean => "false".to_owned(),
                        DataType::Timestamp(..) => format!("\"{}\"", Utc::now().to_rfc3339()),
                        _ => format!("\"{}\"", self.session("refused")),
                    };
                    format!("{}: {value}", Value::from(field.name().as_str()))
                })
                .collect::<Vec<_>>();
            let refused = bifrost
                .insert_into(
                    &table,
                    format!("{{{}}}", fields.join(", ")).into_bytes(),
                    wyrd_client::bifrost::Correlation::default(),
                )
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

    /// The canonical log record under scope `scope` with the given
    /// `attributes` and `body` Variant columns.
    ///
    /// Every other column keeps the canonical fixture's valid value, so a
    /// refusal can come only from the two supplied fields or a deliberate
    /// schema change.
    ///
    /// # Errors
    ///
    /// Returns the Arrow error when the columns do not form a batch, or an
    /// error when a supplied field is undeclared.
    fn records_frame(
        &self,
        scope: &str,
        attributes: (Field, ArrayRef),
        body: (Field, ArrayRef),
    ) -> Result<RecordBatch, ServerJourneyError> {
        let record = canonical_signals::logs(
            &self.records,
            scope,
            Utc::now().timestamp_nanos_opt().unwrap_or(0),
        );
        let mut fields = record.schema().fields().to_vec();
        let mut columns = record.columns().to_vec();
        for (field, column) in [attributes, body] {
            let index = record.schema().index_of(field.name())?;
            fields[index] = Arc::new(field);
            columns[index] = column;
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

/// Results the journey published and the native outcomes they must read
/// back as.
struct PublishedResults {
    /// A Drift result whose requirement scored two features.
    scored_drift: (VerificationResultId, VerificationOutcome),
    /// A Drift result whose window could not be scored.
    unscored_drift: (VerificationResultId, VerificationOutcome),
}

/// The staged audit decision and its native detail.
struct StagedDecision {
    /// The decision's unique operation, which selects its retained row.
    operation: String,
    /// The detail's canonical JSON, the value the retained row must decode to.
    detail: Value,
}

/// The typed-payload journey's server, mock upstream, and tenant.
struct TypedPayloadJourney {
    /// Bound server with a capturing gateway.
    gateway: GatewayCapture,
}

impl TypedPayloadJourney {
    /// Start a bound server whose gateway captures calls to a mock upstream.
    ///
    /// # Errors
    /// Returns a mock URL or server start error.
    async fn start() -> Result<Self, ServerJourneyError> {
        Ok(Self {
            gateway: GatewayCapture::start().await?,
        })
    }

    /// Publish a scored and an unscored Drift outcome through the production
    /// payload builder, staged on the process Scribe outbox under the tenant
    /// SYSTEM principal as the server's internal result writer does, then
    /// flushed.
    ///
    /// # Errors
    /// Returns a seeding, payload, or flush error.
    async fn publish_results(&self) -> Result<PublishedResults, ServerJourneyError> {
        let seed = VerificationFixture::provision(
            self.gateway.server.state().postgres.wyrd(),
            self.gateway.tenant,
        )
        .await?;
        let (subject, _) = seed.service("typed-subject").await?;
        let uid = seed.drift_verifier("typed-drift").await?;
        for (namespace, name) in [("verification", "results"), ("drift", "result_features")] {
            self.gateway
                .server
                .ensure_builtin_table_for_test(self.gateway.tenant, namespace, name)
                .await?;
        }
        let now = Utc::now();
        let window = DriftWindow {
            start: now - chrono::Duration::hours(1),
            end: now,
        };
        let mut published = Vec::new();
        for outcome in [
            drift_outcome(Some(two_feature_report()?))?,
            drift_outcome(None)?,
        ] {
            let payload = ResultPayloadBuilder::new(
                ResultRun {
                    execution_id: ExecutionId::new_v7(),
                    run_id: None,
                    verifier_version: "1.0.0",
                    card_uid: &subject,
                    owner_card_uid: Some(&subject),
                    group_id: None,
                    binding_id: Some(BindingId::new_v7()),
                    trigger: None,
                    window: Some(&window),
                },
                &uid,
                now,
                now,
            )
            .build(&outcome)?;
            let result = payload.result_id();
            self.gateway.server.state().scribe_outbox.stage(
                self.gateway.tenant,
                ScribeWrite::Result {
                    payload,
                    attribution: Attribution::direct(PrincipalId::new(seed.system_principal())),
                },
            );
            published.push((result, outcome));
        }
        self.gateway.server.flush_bifrost().await?;
        let [scored_drift, unscored_drift] =
            <[_; 2]>::try_from(published).map_err(|_| "two results were not published")?;
        Ok(PublishedResults {
            scored_drift,
            unscored_drift,
        })
    }

    /// Stage one allowed decision with a structured detail on the process
    /// Scribe outbox, the one audit write path, and wait until it is retained.
    ///
    /// # Errors
    /// Returns the retention timeout or a detail encoding error.
    async fn stage_audit_decision(&self) -> Result<StagedDecision, ServerJourneyError> {
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
            Attribution::direct(PrincipalId::new(uuid::Uuid::now_v7())),
            PrincipalKindTag::User,
            "bifrost:query:read".to_owned(),
            AuditOutcome::Allowed,
        )
        .with_detail(detail.clone());
        self.gateway
            .server
            .state()
            .scribe_outbox
            .stage(self.gateway.tenant, event);
        self.gateway.server.await_audit_retained().await?;
        Ok(StagedDecision {
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
                self.gateway.tenant,
                Vec::new(),
                PermissionSet::from_iter([permission.clone(), Permission::gateway_payload_read()]),
            ),
            self.gateway.tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            permission,
        )?;
        let mut batches = Vec::new();
        ScheduledQueryCaller::new(
            self.gateway.server.state().clone(),
            context,
            CancellationToken::new(),
        )
        .run_with(
            BifrostQueryRequest {
                sql,
                params: Vec::new(),
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
            .with_encoder_factory(Arc::new(WyrdJsonEncoderFactory))
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

/// Register a Verifier whose one requirement asserts its `x` JSON input is
/// `true`, so an enqueued execution reads no external evidence.
///
/// # Errors
/// Returns a fixture error when the Card write fails.
async fn assert_verifier(
    seed: &VerificationFixture,
    name: &str,
) -> Result<CardUid, ServerJourneyError> {
    Ok(seed
        .verifier(
            name,
            &json!({
                "inputs": { "x": { "type": "json" } },
                "requirements": [{ "id": "valid", "statement": "x holds",
                    "check": { "kind": "assert", "path": "inputs.x",
                        "operator": "equals", "expected": true } }]
            }),
        )
        .await?)
}

/// Body of `POST /v1/verification/enqueue` asking `verifier` to judge
/// `subject` over the caller-supplied `inputs` declarations.
fn enqueue_body(verifier: &CardUid, subject: &CardUid, inputs: Value) -> Value {
    json!({ "verifier_card_uid": verifier, "card_uid": subject, "inputs": inputs })
}

/// Parse the `execution_id` of an accepted enqueue and wait until the
/// staged execution reaches the queue, which happens only after Scribe
/// acknowledges it.
///
/// # Errors
/// Returns a fixture error, or a description when the response carries no
/// ID or the execution is not queued within [`WAIT`].
async fn await_queued(
    seed: &VerificationFixture,
    accepted: &Value,
) -> Result<ExecutionId, ServerJourneyError> {
    let id = accepted["execution_id"]
        .as_str()
        .ok_or("the execution has an ID")?;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        if let Some(execution) = seed
            .executions()
            .await?
            .into_iter()
            .find(|execution| execution.to_string() == id)
        {
            return Ok(execution);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the accepted execution {id} was never queued").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Role a delegated enqueue needs from both its subject and actor.
const QUEUED_RUNNER: &str = "queued_runner";

/// Role only the subject holds, so a delegated execution's accepted
/// authority excludes it.
const QUEUED_SUBJECT_ONLY: &str = "queued_subject_only";

/// A delegated enqueued execution keeps its accepted subject, actor
/// history, and narrowed authority through a retry and a restarted runtime
/// (AC-013/017).
///
/// User A holds the runner role and gateway invocation; Service B holds
/// only the runner role. B, acting for A, enqueues an assertion Verifier
/// over the public route. The execution is stored as A/B with an authority
/// snapshot whose principal is A, whose delegation chain names B, and which
/// carries no gateway grant; the execution view exposes no snapshot. A's runner role is then
/// revoked and B is granted gateway invocation. A first runtime fails the
/// attempt retryably and stops; a newly built runtime, holding no caller
/// state, completes it. The snapshot is byte-for-byte unchanged and the
/// result is attributed to A with SYSTEM as actor.
///
/// # Errors
/// Returns server, exchange, route, runtime, or query errors, or a
/// description of the first identity or snapshot that differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn queued_subject_authority_survives_restart() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::builder().start_bound().await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (subject, _) = seed.service("queued-subject").await?;
    let verifier = assert_verifier(&seed, "queued-assert").await?;
    server
        .seed_role(
            QUEUED_RUNNER,
            &[Permission::card_read(), Permission::verifier_run_any()],
        )
        .await?;
    server
        .seed_role(QUEUED_SUBJECT_ONLY, &[Permission::gateway_invoke_any()])
        .await?;
    let a = server
        .bootstrap_user("queued-a", &[QUEUED_RUNNER, QUEUED_SUBJECT_ONLY])
        .await?;
    let b = server
        .bootstrap_service("queued-b", &[QUEUED_RUNNER])
        .await?;
    let Bootstrap::User { jwt: a_token, .. } = &a else {
        return Err("A bootstraps a user".into());
    };
    let a_token = a_token.clone();
    let delegated = super::query::client_for(&server, b.api_key().ok_or("B carries an API key")?)?
        .on_behalf_of(
            secrecy::SecretString::from(a_token),
            wyrd_spec::auth::TokenAudience::Wyrd,
        )
        .await?;
    let started: Value = delegated
        .request_json(
            reqwest::Method::POST,
            "/v1/verification/enqueue",
            Some(&enqueue_body(
                &verifier,
                &subject,
                json!({ "x": { "kind": "json", "value": true } }),
            )),
        )
        .await?;
    let execution = await_queued(&seed, &started).await?;

    let superuser = server.pg_fixture().superuser_pool()?;
    let stored = async || -> Result<(uuid::Uuid, uuid::Uuid, Value), ServerJourneyError> {
        Ok(sqlx::query_as(
            "SELECT sub_principal_id, act_principal_id, authority \
             FROM wyrd.verifier_executions WHERE execution_id = $1",
        )
        .bind(execution.as_uuid())
        .fetch_one(&superuser)
        .await?)
    };
    let (sub, act, authority) = stored().await?;
    let (a_id, b_id) = (a.id().as_uuid(), b.id().as_uuid());
    let snapshot = authority.to_string();
    if (sub, act) != (a_id, b_id)
        || authority["principal"]["id"] != json!(a_id)
        || !authority["delegation_chain"]
            .to_string()
            .contains(&b_id.to_string())
        || snapshot.contains("gateway")
    {
        return Err(format!(
            "expected A/B with A's narrowed authority, read {sub}/{act} {authority}"
        )
        .into());
    }
    let view: Value = delegated
        .request_json::<(), _>(
            reqwest::Method::GET,
            &format!("/v1/verification/executions/{execution}"),
            None,
        )
        .await?;
    if view.get("authority").is_some() || view["execution_id"] != json!(execution.to_string()) {
        return Err(format!("the execution view differs or exposes authority: {view}").into());
    }

    server.revoke_role(&a, QUEUED_RUNNER).await?;
    server.grant_role(&b, QUEUED_SUBJECT_ONLY).await?;
    let retryable = wyrd_spec::verification::VerificationError {
        code: "test_transient".to_owned(),
        message: "the first worker fails transiently".to_owned(),
    };
    let settled =
        async |outcome: EngineOutcome, done: &[&str]| -> Result<ExecutionRow, ServerJourneyError> {
            let script = EngineScript::default();
            script.push_unheld(outcome);
            let runtime = VerificationRuntime::builder(server.state())
                .engine_script(script)
                .build()
                .ok_or("the runtime did not compose")?;
            let stop = CancellationToken::new();
            let task = tokio::spawn(runtime.run(stop.clone()));
            let deadline = tokio::time::Instant::now() + WAIT;
            let row = loop {
                let row = seed.execution(execution).await?;
                if done.contains(&row.status.as_str()) {
                    break row;
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!("the execution never reached {done:?}: {row:?}").into());
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            };
            stop.cancel();
            tokio::time::timeout(WAIT, task).await??;
            Ok(row)
        };
    let first = settled(EngineOutcome::Retry(retryable), &["retrying"]).await?;
    seed.make_retry_due(execution).await?;
    let second = settled(
        EngineOutcome::Completed(drift_outcome(None)?),
        &["completed", "errored"],
    )
    .await?;
    let (Some(result), 2, "completed") =
        (second.result_id, second.attempts, second.status.as_str())
    else {
        return Err(format!(
            "expected completion on the second attempt: {first:?} then {second:?}"
        )
        .into());
    };
    let (sub, act, after) = stored().await?;
    if (sub, act) != (a_id, b_id) || after != authority {
        return Err(format!("the accepted execution changed: {sub}/{act} {after}").into());
    }

    let unwritten = server
        .state()
        .scribe_outbox
        .settle(std::time::Instant::now() + WAIT)
        .await;
    if unwritten != 0 {
        return Err(format!("{unwritten} results never reached Scribe").into());
    }
    server.flush_bifrost().await?;
    let system = seed.system_principal();
    let rows = query_rows(
        &server,
        tenant,
        format!(
            "SELECT result_id FROM vala.verification.results WHERE result_id = '{result}' \
             AND {SUB_PRINCIPAL_ID} = '{a_id}' AND {ACT_PRINCIPAL_ID} = '{system}'"
        ),
    )
    .await?;
    if rows != 1 {
        return Err(format!("expected one A/SYSTEM result, read {rows}").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// A delegated execution's trace input reads keep the accepted delegation
/// history (REQ-008/010, AC-017, FIND-TASK-001-6).
///
/// Service B, acting for User A, enqueues a trace Verifier over the public
/// route naming a trace that was never recorded. The real runtime reads the
/// registered span table as the accepted caller. Every retained read
/// decision of that execution is recorded for A with SYSTEM as actor and
/// carries exactly the accepted chain `[B]`; the absent trace makes the
/// requirement inconclusive, so the execution still completes.
///
/// # Errors
/// Returns server, exchange, route, runtime, or query errors, or a
/// description of the first expectation that does not hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn delegated_input_reads_keep_history() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::builder().start_bound().await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (subject, _) = seed.service("history-subject").await?;
    let verifier = seed
        .verifier(
            "history-trace",
            &json!({
                "inputs": { "trace": { "type": "trace" } },
                "requirements": [{ "id": "traced", "statement": "the trace retrieves first",
                    "check": { "kind": "trace", "on": "inputs.trace",
                        "span_selector": "$.spans[0].name",
                        "operator": "equals", "expected": "retrieve" } }]
            }),
        )
        .await?;
    server
        .ensure_builtin_table_for_test(tenant, "traces", "spans")
        .await?;
    server
        .seed_role(
            QUEUED_RUNNER,
            &[
                Permission::card_read(),
                Permission::verifier_run_any(),
                Permission::bifrost_table_read(),
                Permission::bifrost_query_read(),
            ],
        )
        .await?;
    let a = server.bootstrap_user("history-a", &[QUEUED_RUNNER]).await?;
    let b = server
        .bootstrap_service("history-b", &[QUEUED_RUNNER])
        .await?;
    let a_token = a.jwt().ok_or("A bootstraps a user")?.to_owned();
    let delegated = super::query::client_for(&server, b.api_key().ok_or("B carries an API key")?)?
        .on_behalf_of(
            secrecy::SecretString::from(a_token),
            wyrd_spec::auth::TokenAudience::Wyrd,
        )
        .await?;
    let started: Value = delegated
        .request_json(
            reqwest::Method::POST,
            "/v1/verification/enqueue",
            Some(&enqueue_body(
                &verifier,
                &subject,
                json!({ "trace": { "kind": "trace",
                    "trace_id": uuid::Uuid::now_v7().simple().to_string() } }),
            )),
        )
        .await?;
    let execution = await_queued(&seed, &started).await?;

    let runtime = VerificationRuntime::builder(server.state())
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + WAIT;
    let row = loop {
        let row = seed.execution(execution).await?;
        if !["pending", "running", "retrying"].contains(&row.status.as_str()) {
            break row;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the trace execution never settled: {row:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    if row.status != "completed" {
        return Err(format!("the delegated trace execution settled {row:?}").into());
    }

    server.await_audit_retained().await?;
    let (a_id, b_id) = (a.id().to_string(), b.id().to_string());
    let reads = server
        .retained_audit_records(
            tenant,
            "act_principal_id, detail",
            &format!("operation = 'bifrost.query.read_decision' AND sub_principal_id = '{a_id}'"),
        )
        .await?;
    if reads.is_empty() {
        return Err("the execution's trace read retained no decision for A".into());
    }
    let system = seed.system_principal().to_string();
    for read in &reads {
        let detail: Value =
            serde_json::from_str(read[1].as_deref().ok_or("the decision has a detail")?)?;
        let chain: Vec<&str> = super::audit_retention::find_field(&detail, "delegation_chain")
            .and_then(Value::as_array)
            .map(|steps| {
                steps
                    .iter()
                    .filter_map(|step| step["principal_id"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        if read[0].as_deref() != Some(system.as_str()) || chain != [b_id.as_str()] {
            return Err(
                format!("expected an A/SYSTEM read with chain [{b_id}], read {read:?}").into(),
            );
        }
    }
    server.shutdown().await?;
    Ok(())
}

/// A scheduled Drift occurrence runs for its binding owner with SYSTEM as
/// actor from acceptance to the published result (AC-009/018,
/// FIND-TASK-001-7).
///
/// A monthly Custom Drift binding of a subject Card is owned by a separate
/// Service. Once the owner is active and the occurrence is due, the production
/// runtime accepts it and runs the real Drift engine. The stored execution
/// pairs the owner with SYSTEM and captures the owner's recovered authority with no
/// delegation history; its input read decision and its published result carry
/// the same owner/SYSTEM pair.
///
/// # Errors
/// Returns server, seeding, runtime, or query errors, or a description of
/// the first expectation that does not hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn scheduled_owner_authority_is_retained() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::builder().start_bound().await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (subject, _) = seed.service("scheduled-subject").await?;
    let (owner, owner_principal) = seed.service("scheduled-owner").await?;
    let verifier = seed
        .custom_drift_verifier("scheduled-drift", "score", 1.0, 0.5)
        .await?;
    server
        .ensure_builtin_table_for_test(tenant, "drift", "observations")
        .await?;
    let binding = seed
        .bind_schedule(&owner, &subject, &verifier, MONTHLY, Vec::new())
        .await?;
    seed.activate(owner_principal).await?;
    seed.make_binding_due(binding).await?;

    let runtime = VerificationRuntime::builder(server.state())
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let (execution, row) = first_settled(&seed, &["pending", "running", "retrying"]).await?;
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    let ("completed", Some(result)) = (row.status.as_str(), row.result_id) else {
        return Err(format!("the scheduled Drift run settled {row:?}").into());
    };

    let (owner_id, system) = (owner_principal.as_uuid(), seed.system_principal());
    let (sub, act, authority): (uuid::Uuid, uuid::Uuid, Value) = sqlx::query_as(
        "SELECT sub_principal_id, act_principal_id, authority FROM wyrd.verifier_executions \
         WHERE execution_id = $1",
    )
    .bind(execution.as_uuid())
    .fetch_one(&server.pg_fixture().superuser_pool()?)
    .await?;
    if (sub, act) != (owner_id, system)
        || authority["principal"]["id"] != json!(owner_id)
        || authority
            .get("delegation_chain")
            .and_then(Value::as_array)
            .is_some_and(|chain| !chain.is_empty())
    {
        return Err(format!(
            "expected the owner/SYSTEM execution under the owner's own authority, read \
             {sub}/{act} {authority}"
        )
        .into());
    }
    server.await_audit_retained().await?;
    let reads = server
        .retained_audit_records(
            tenant,
            "act_principal_id",
            &format!(
                "operation = 'bifrost.query.read_decision' AND sub_principal_id = '{owner_id}'"
            ),
        )
        .await?;
    if reads.is_empty()
        || reads
            .iter()
            .any(|read| read[0].as_deref() != Some(system.to_string().as_str()))
    {
        return Err(format!("expected owner/SYSTEM input reads, read {reads:?}").into());
    }

    let unwritten = server
        .state()
        .scribe_outbox
        .settle(std::time::Instant::now() + WAIT)
        .await;
    if unwritten != 0 {
        return Err(format!("{unwritten} results never reached Scribe").into());
    }
    server.flush_bifrost().await?;
    let rows = query_rows(
        &server,
        tenant,
        format!(
            "SELECT result_id FROM vala.verification.results WHERE result_id = '{result}' \
             AND binding_id = '{binding}' AND {SUB_PRINCIPAL_ID} = '{owner_id}' \
             AND {ACT_PRINCIPAL_ID} = '{system}'"
        ),
    )
    .await?;
    if rows != 1 {
        return Err(format!("expected one owner/SYSTEM result, read {rows}").into());
    }
    server.shutdown().await?;
    Ok(())
}

/// Access-token lifetime of [`reloaded_authority_cannot_widen_gateway_access`],
/// short enough to observe the initiating credential expire.
const WIDEN_ACCESS_TTL: chrono::Duration = chrono::Duration::seconds(3);

/// A queued execution's reloaded authority cannot reach a model its
/// captured delegated intersection excluded (AC-013/017, FIND-TASK-001-7).
///
/// User A holds the runner and model roles; Service B holds only the
/// runner role. B, acting for A, enqueues an assertion Verifier, so the stored
/// authority is A's narrowed by B and carries no model grant. B is then
/// granted the model role and the initiating delegated token is let expire
/// until the server refuses it. A fresh queue claim reloads the stored
/// authority into the protected gateway consumer a judge uses: the model call
/// is refused before any provider request and audited as A/SYSTEM with the
/// accepted chain `[B]`. B's own fresh token then reaches the same model,
/// proving the new grant permits it and only the captured intersection
/// excluded it.
///
/// # Errors
/// Returns server, exchange, route, claim, or query errors, or a description
/// of the first expectation that does not hold.
///
/// # Panics
/// Panics if `#[tokio::test]` cannot build its runtime, if the static
/// `openai/gpt-test` projection stops parsing as a model reference, or if a
/// retained audit row lacks either of the two projected columns; every
/// behavioral expectation failure is returned as an error instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn reloaded_authority_cannot_widen_gateway_access() -> Result<(), ServerJourneyError> {
    /// The role carrying the gateway model's invoke permission: A holds it
    /// before acceptance, B receives it only after the execution is accepted, so
    /// the captured A∩B authority lacks it while B's fresh token, the
    /// positive control, has it.
    const MODEL_ROLE: &str = "widen_model";
    let provider = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_widen", "object": "chat.completion", "created": 1_700_000_000,
            "model": "gpt-test",
            "choices": [{ "index": 0, "finish_reason": "stop",
                "message": { "role": "assistant", "content": "ok" } }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        })))
        .mount(&provider)
        .await;
    let server = Box::pin(
        WyrdTestServer::builder()
            .with_gateway_provider_root_for_test(url::Url::parse(&provider.uri())?)
            .with_access_ttl(WIDEN_ACCESS_TTL)
            .with_auth_verify_settings(wyrd_auth_verify::WyrdAuthVerifySettings {
                allowed_clock_skew: Duration::ZERO,
            })
            .start_bound(),
    )
    .await?;
    let tenant = server.data_tenant_id();
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (subject, _) = seed.service("widen-subject").await?;
    let verifier = assert_verifier(&seed, "widen-assert").await?;
    server
        .deploy_gateway_model_for_test("gpt-test", MODEL_ROLE)
        .await?;
    server
        .seed_role(
            QUEUED_RUNNER,
            &[Permission::card_read(), Permission::verifier_run_any()],
        )
        .await?;
    let a = server
        .bootstrap_user("widen-a", &[QUEUED_RUNNER, MODEL_ROLE])
        .await?;
    let b = server
        .bootstrap_service("widen-b", &[QUEUED_RUNNER])
        .await?;
    let b_token = async || -> Result<String, ServerJourneyError> {
        Ok(server
            .exchange_api_key(b.api_key().ok_or("B carries an API key")?)
            .await?)
    };
    let initiating = server
        .delegate(
            a.jwt().ok_or("A bootstraps a user")?,
            &b_token().await?,
            wyrd_spec::auth::TokenAudience::Wyrd,
        )
        .await?;
    let started = server
        .oneshot_authenticated(
            &initiating,
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/verification/enqueue")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(serde_json::to_vec(&enqueue_body(
                    &verifier,
                    &subject,
                    json!({ "x": { "kind": "json", "value": true } }),
                ))?))?,
        )
        .await?;
    if !started.status().is_success() {
        return Err(format!(
            "the delegated execution was not accepted: {}",
            started.status()
        )
        .into());
    }
    let accepted: Value =
        serde_json::from_slice(&axum::body::to_bytes(started.into_body(), usize::MAX).await?)?;
    let execution = await_queued(&seed, &accepted).await?;

    server.grant_role(&b, MODEL_ROLE).await?;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let probe = server
            .oneshot_authenticated(
                &initiating,
                axum::http::Request::builder()
                    .uri(format!("/v1/verification/executions/{execution}"))
                    .body(axum::body::Body::empty())?,
            )
            .await?;
        if probe.status() == axum::http::StatusCode::UNAUTHORIZED {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("the initiating delegated token never expired".into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let mut conn = server.tenant_conn_for(tenant).await?;
    let claimed = wyrd_sql::queries::verifier_executions::VerifierExecutionQueue::default()
        .claim(&mut conn, chrono::Duration::minutes(1))
        .await?
        .ok_or("the queued execution is claimable")?;
    conn.commit().await?;
    let system = claimed.system_principal.ok_or("the tenant has SYSTEM")?;
    let call = |subject: Option<CardUid>| wyrd_server::components::gateway::GatewayCallRequest {
        operation: wyrd_spec::gateway::GatewayOperation::ChatCompletions,
        ingress: wyrd_gateway::IngressDialect::OpenAi,
        model: wyrd_spec::gateway::ModelRef::from_projection("openai/gpt-test")
            .expect("the model reference is valid"),
        fallback: None,
        body: json!({ "model": "openai/gpt-test",
            "messages": [{ "role": "user", "content": "hi" }] }),
        media: None,
        batch: None,
        deployment: None,
        stream: false,
        usage_bound: None,
        timeout: Duration::from_secs(10),
        subject: subject.map(|card_uid| {
            wyrd_server::components::gateway::GatewayCallAttribution::Resolved(
                wyrd_server::components::gateway::GatewayCallSubject {
                    run_id: None,
                    card_uid: Some(card_uid),
                },
            )
        }),
        prompt_provider: None,
    };
    let reloaded = wyrd_server::components::auth::Caller::accepted(&claimed.authority, system);
    if let Ok(response) = wyrd_server::components::gateway::GatewayInvocation::new(server.state())
        .invoke(&reloaded, call(Some(claimed.card_uid.clone())))
        .await
    {
        return Err(format!(
            "the reloaded authority reached the model: {}",
            response.status
        )
        .into());
    }
    let reached = provider.received_requests().await.unwrap_or_default().len();
    if reached != 0 {
        return Err(format!("the refused call reached the provider {reached} times").into());
    }

    let fresh = reqwest::Client::new()
        .post(format!(
            "{}/v1/chat/completions",
            server.base_url().ok_or("missing HTTP URL")?
        ))
        .bearer_auth(b_token().await?)
        .json(&call(None).body)
        .send()
        .await?;
    if !fresh.status().is_success() {
        return Err(format!("B's fresh token was refused: {}", fresh.status()).into());
    }

    server.await_audit_retained().await?;
    let (a_id, b_id) = (a.id().to_string(), b.id().to_string());
    let denied = server
        .retained_audit_records(
            tenant,
            "act_principal_id, detail",
            &format!(
                "operation = 'gateway.invoke' AND outcome = 'denied' \
                 AND sub_principal_id = '{a_id}'"
            ),
        )
        .await?;
    let [row] = denied.as_slice() else {
        return Err(format!("expected one denied A/SYSTEM model decision: {denied:?}").into());
    };
    let detail: Value = serde_json::from_str(row[1].as_deref().ok_or("the decision has detail")?)?;
    let chain: Vec<&str> = super::audit_retention::find_field(&detail, "delegation_chain")
        .and_then(Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| step["principal_id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    if row[0].as_deref() != Some(system.to_string().as_str()) || chain != [b_id.as_str()] {
        return Err(format!("expected A/SYSTEM with chain [{b_id}], read {row:?}").into());
    }
    server.shutdown().await?;
    Ok(())
}
