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

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use wyrd_client::bifrost::BifrostOptions;

use arrow::array::{
    Array, ArrayRef, AsArray, BinaryArray, Float64Array, Int32Array, ListArray, StringArray,
    StructArray, TimestampMicrosecondArray,
};
use arrow::compute::cast;
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
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
use wyrd_server::state::LimitsConfig;
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::engines::EngineOutcome;
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::hosted::HostedLimits;
use wyrd_server::verification::results::{ResultPayloadBuilder, ResultRun};
use wyrd_server::verification::runner::EngineScript;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId, PrincipalKindTag};
use wyrd_spec::card::verifier::Check;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, SlotName, VerificationResultId};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, AuthMethod, BifrostQueryRequest, VARIANT_MAX_DEPTH,
    VARIANT_MAX_ENCODED_BYTES,
};
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::managed_columns::{
    ACT_PRINCIPAL_ID, RUN_ID, SUB_PRINCIPAL_ID, SUBJECT_CARD_UID, WORKLOAD_CARD_UID,
    WYRD_INGESTED_AT,
};
use wyrd_spec::vala::{AuditDetail, BifrostError, audit_detail_canonical_json};
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{
    InputDeclaration, RequirementDetail, RequirementOutcome, RequirementResult, VerificationOutcome,
};
use wyrd_spec::verification::{DriftReport, DriftVerdict, DriftWindow, VerificationVerdict};
use wyrd_testing::bifrost::{BifrostClusterSpec, RawIngest, WyrdTestCluster, canonical_signals};
use wyrd_testing::gateway_capture::GatewayCapture;
use wyrd_testing::listener_stall::ListenerStallProxy;
use wyrd_testing::verification::{ExecutionRow, VerificationFixture};
use wyrd_testing::{Bootstrap, WyrdTestServer, WyrdTestServerBuilder};
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
            data: None,
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
/// owner, binding, run, result, and one shared event time, with the owner
/// Service and subject described by name and version by the peer that
/// admitted them.
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
    seed.activate(owner_principal, &owner).await?;
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
        .bifrost
        .scribe_outbox()
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
             AND {alias}{SUBJECT_CARD_UID} = '{subject}' AND {alias}{WORKLOAD_CARD_UID} = '{owner}' \
             AND {alias}binding_id = '{binding}' AND {alias}service_name = 'remote-owner' \
             AND {alias}service_version = '1.0.0' AND {alias}subject_kind = 'Service' \
             AND {alias}subject_name = 'remote-subject' AND {alias}subject_version = '1.0.0'"
        )
    };
    let checks = [
        (
            format!(
                "SELECT result_id FROM bifrost.verification.results WHERE result_id = '{result}'"
            ),
            1,
            "one published summary for the run",
        ),
        (
            format!(
                "SELECT result_id FROM bifrost.verification.results WHERE {}",
                identity("")
            ),
            1,
            "the summary carries every exact identity",
        ),
        (
            format!(
                "SELECT result_id FROM bifrost.drift.result_features WHERE result_id = '{result}'"
            ),
            2,
            "two published feature rows for the run",
        ),
        (
            format!(
                "SELECT f.result_id FROM bifrost.drift.result_features f \
                 JOIN bifrost.verification.results r \
                   ON f.result_id = r.result_id \
                  AND f.{ACT_PRINCIPAL_ID} = r.{ACT_PRINCIPAL_ID} AND f.{SUBJECT_CARD_UID} = r.{SUBJECT_CARD_UID} \
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
    seed.activate(owner_principal, &owner).await?;
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
    let subject_ref = scribe
        .bootstrap_service_in_tenant(tenant, "shared-subject", &[])
        .await?
        .card_ref()
        .ok_or("a service bootstrap returned a user")?
        .clone();
    // The family's runtime account owns the scheduled bindings and writes the
    // observations at the subject root.
    let Bootstrap::Machine {
        id: client,
        api_key,
        ..
    } = scribe
        .credential_registered_service(&subject_ref, &["admin"])
        .await?
    else {
        return Err("a service credential returned a user".into());
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
    )?
    .for_workload(subject.clone());
    Bifrost::connect_with(&writer, BifrostOptions::default())
        .await?
        .write_batch(
            "bifrost.drift.observations",
            &observation_batch(
                &record,
                &subject,
                observed_in_current_month(Utc::now())?,
                3.0,
            )?,
        )
        .await?;
    seed.activate(client, &subject).await?;
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
        .bifrost
        .scribe_outbox()
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
            "SELECT {SUB_PRINCIPAL_ID}, {SUBJECT_CARD_UID}, series FROM bifrost.drift.observations \
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
            format!("SELECT {column} FROM bifrost.drift.observations"),
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
                "SELECT verifier_card_uid, {SUBJECT_CARD_UID}, verdict, {RUN_ID} \
                 FROM bifrost.verification.results WHERE binding_id = '{binding}'"
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

/// One Drift observation `record` of the Card `subject` at `event_time`: a
/// `score` feature valued `score` and a `latency` feature, as the SDK's tall
/// projection writes them.
///
/// The batch carries the table's own authored columns, its shared dimensions
/// left null for the server to derive, and the `subject_card_uid` correlation.
/// `event_time` is the rows' `created_at`, the observation table's time
/// column, which Scribe keeps verbatim.
///
/// # Errors
/// Returns an Arrow error when the batch cannot be assembled.
fn observation_batch(
    record: &str,
    subject: &CardUid,
    event_time: DateTime<Utc>,
    score: f64,
) -> Result<RecordBatch, ServerJourneyError> {
    use vala_bifrost_redux::tables::DomainTable;
    use vala_bifrost_redux::tables::drift::ObservationsTable;

    let mut fields = ObservationsTable::arrow_fields();
    fields.push(Field::new(SUBJECT_CARD_UID, DataType::Utf8, true));
    let event_time = event_time.timestamp_micros();
    let columns = fields
        .iter()
        .map(|field| -> ArrayRef {
            match field.name().as_str() {
                "record_id" => Arc::new(StringArray::from(vec![record, record])),
                "series" => Arc::new(StringArray::from(vec!["score", "latency"])),
                "num_value" => Arc::new(Float64Array::from(vec![score, 120.0])),
                "created_at" => Arc::new(
                    TimestampMicrosecondArray::from(vec![event_time, event_time])
                        .with_data_type(field.data_type().clone()),
                ),
                SUBJECT_CARD_UID => {
                    Arc::new(StringArray::from(vec![subject.as_str(), subject.as_str()]))
                }
                _ => arrow::array::new_null_array(field.data_type(), 2),
            }
        })
        .collect();
    Ok(RecordBatch::try_new(
        Arc::new(Schema::new(fields)),
        columns,
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
/// the two tables carries a physical Bloom filter for every default Bloom
/// column of its table's layout, and Oracle's scan metrics show a day-range
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
                "SELECT feature FROM bifrost.drift.result_features WHERE result_id = '{result}'"
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
            "SELECT feature FROM bifrost.drift.result_features WHERE result_id = '{unscored}'"
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
        .texts(times("ended_at", "bifrost.verification.results"))
        .await?;
    let details = journey
        .texts(times(WYRD_INGESTED_AT, "bifrost.drift.result_features"))
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
            "SELECT result_id FROM bifrost.verification.results".to_owned(),
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
                "SELECT result_id FROM bifrost.verification.results \
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
                "SELECT result_id FROM bifrost.verification.results WHERE result_id = '{straddling}'"
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

/// Production Oracle query families a measured read prints as its evidence:
/// query count, files, row groups, pruning, bytes, rows, and the per-step
/// profile nanoseconds that separate cut discovery, planning, admission, and
/// operator time.
const WORK_FAMILIES: &[&str] = &[
    "oracle_query_profiles_total",
    "oracle_query_files_scanned_total",
    "oracle_query_row_groups_scanned_total",
    "oracle_query_row_groups_pruned_total",
    "oracle_query_row_groups_pruned_bloom_total",
    "oracle_query_rows_pruned_page_index_total",
    "oracle_query_bytes_scanned_total",
    "oracle_query_rows_total",
    "oracle_query_profile_nanoseconds_total",
    "scribe_write_profiles_total",
    "scribe_write_profile_nanoseconds_total",
    "scribe_write_profile_count_total",
    "bifrost_parquet_upload_bytes",
    "bifrost_parquet_upload_duration_seconds",
    "bifrost_scribe_persistence_encoded_bytes_total",
];

/// Oracle work one measured read did, from the cluster's production query
/// counters.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScanEvidence {
    /// `oracle_query_profiles_total` delta: settled query attempts.
    queries: f64,
    /// `oracle_query_files_scanned_total` delta.
    files: f64,
    /// `oracle_query_row_groups_scanned_total` delta.
    row_groups: f64,
    /// `oracle_query_row_groups_pruned_bloom_total` delta.
    bloom_pruned: f64,
    /// `oracle_query_bytes_scanned_total` delta.
    bytes: f64,
}

impl ScanEvidence {
    /// Whether the integral counter delta `value` is `expected`.
    fn is(value: f64, expected: u32) -> bool {
        (value - f64::from(expected)).abs() < 0.5
    }

    /// Await `read` and return its value with the Oracle work done while it
    /// ran, printing that work under `label` as one evidence line.
    ///
    /// The window is the whole future, so a read that issues several
    /// statements reports their sum and its statement count.
    ///
    /// # Errors
    /// Returns the read's error or a telemetry error.
    async fn of<T>(
        cluster: &WyrdTestCluster,
        label: &str,
        read: impl Future<Output = Result<T, ServerJourneyError>>,
    ) -> Result<(T, Self), ServerJourneyError> {
        let telemetry = cluster.telemetry();
        let checkpoint = telemetry.checkpoint()?;
        let value = read.await?;
        let delta = telemetry.delta_since(&checkpoint)?;
        let sum = |family: &str| -> f64 {
            delta
                .metrics
                .iter()
                .filter(|sample| sample.family == family)
                .map(|sample| sample.value)
                .sum()
        };
        let evidence = Self {
            queries: sum("oracle_query_profiles_total"),
            files: sum("oracle_query_files_scanned_total"),
            row_groups: sum("oracle_query_row_groups_scanned_total"),
            bloom_pruned: sum("oracle_query_row_groups_pruned_bloom_total"),
            bytes: sum("oracle_query_bytes_scanned_total"),
        };
        println!(
            "work {label}: {evidence:?} {}",
            delta.evidence(WORK_FAMILIES)
        );
        Ok((value, evidence))
    }
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
        let verifier: CardRef = format!("Verifier/layout-drift@1.0.0#{uid}").parse()?;
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
                deployment_environment: None,
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
        let outbox = ScribeSink::local_outbox(
            Arc::new(StraddleScribe {
                inner: Arc::clone(&scribe),
                straddle,
            }),
            None,
        );
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
        let (read, evidence) =
            ScanEvidence::of(self.cluster, &sql, self.texts(sql.clone())).await?;
        if read.len() != rows {
            return Err(format!("{sql} expected {rows} rows, read {read:?}").into());
        }
        Ok(evidence)
    }

    /// Require a physical Bloom filter on every default Bloom column of each
    /// result table's own layout, in every published file of the two tables.
    ///
    /// Files are found under the cluster's storage root and classified by a
    /// column only their table carries; each table must have at least one.
    ///
    /// # Errors
    /// Returns a filesystem or Parquet error, or names the first table with
    /// no file or the first column chunk without a Bloom filter.
    fn assert_blooms(&self) -> Result<(), ServerJourneyError> {
        use vala_bifrost_redux::tables::DomainTable;
        use vala_bifrost_redux::tables::drift::ResultFeaturesTable;
        use vala_bifrost_redux::tables::verification::ResultsTable;

        let tables = [
            (
                "bifrost.verification.results",
                "execution_status",
                ResultsTable::physical_layout().bloom_columns,
            ),
            (
                "bifrost.drift.result_features",
                "feature",
                ResultFeaturesTable::physical_layout().bloom_columns,
            ),
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
                let (table, _, declared) = &tables[index];
                for group in metadata.row_groups() {
                    for name in declared {
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
        if self.straddle && frame.table.fqn() == "bifrost.verification.results" {
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
        "SELECT result_id, verdict, outcome FROM bifrost.verification.results \
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
        "SELECT request_id, requested_model['deployment'], requested_model['model'], \
         resolved_model['deployment'], resolved_model['model'] FROM bifrost.gateway.calls \
         WHERE request_id IN ('{}', '{}') ORDER BY request_id",
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
            "SELECT request_payload, response_payload FROM bifrost.gateway.calls \
             WHERE request_id = '{}'",
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
            "SELECT detail FROM bifrost.system.audit_log WHERE operation = '{}'",
            decision.operation
        ))
        .await?;
    expect_eq("audit detail", &audit, &vec![vec![decision.detail]])?;

    journey.gateway.server.shutdown().await?;
    Ok(())
}

/// Every built-in and a dynamic table share one attribution envelope.
///
/// Each of the nine freshly provisioned built-ins describes, through the
/// public HTTP route, exactly `subject_card_uid` then `run_id` as writer
/// correlation inputs, its declared ledger layout as the resolved layout, and
/// the six shared dimensions once each as nullable text, with no managed or
/// retired `card_uid`/`owner_card_uid` name among its own fields. A dynamic
/// table describes its two declared fields and the same correlation inputs,
/// and no shared dimension. All ten tables plan a read of the seven managed
/// columns, and the dynamic table refuses a read of a shared dimension.
/// Rows written by three producers carry their own origin: a verification
/// result names its binding owner as workload and its subject Card, written
/// by the tenant SYSTEM principal; a gateway call stores its own request as
/// `wyrd_request_id`; and an audit decision keeps its decision pair and
/// request, with no workload or subject.
///
/// # Errors
/// Returns a server, describe, registration, publication, or query error, or a
/// description of the first table or row that differs from its contract.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn system_and_dynamic_tables_share_attribution() -> Result<(), ServerJourneyError> {
    use vala_bifrost_redux::tables::managed_columns::{MANAGED_COLUMNS, is_managed_column};
    use vala_bifrost_redux::tables::{SHARED_DIMENSIONS, builtin_tables};

    let journey = TypedPayloadJourney::start().await?;
    let server = &journey.gateway.server;
    for table in builtin_tables() {
        server
            .ensure_builtin_table_for_test(journey.gateway.tenant, table.namespace, table.name)
            .await?;
    }
    let bootstrap = server
        .bootstrap_service("attribution_describer", &["admin"])
        .await?;
    let client = super::query::client_for(server, bootstrap.api_key().ok_or("a machine API key")?)?;
    let dynamic = format!(
        "bifrost.datasets.attributed_{}",
        uuid::Uuid::now_v7().simple()
    );
    let declared = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, true),
    ]));
    Bifrost::connect_with(
        &client,
        BifrostOptions {
            table: Some(TableConfig::from_arrow(&dynamic, declared)?),
            ..BifrostOptions::default()
        },
    )
    .await?
    .register()
    .await?;
    let query = Bifrost::query_only(&client);
    let correlation = |description: &wyrd_spec::vala::api::BifrostTableDescription| {
        description
            .correlation_fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>()
    };

    let mut fqns = Vec::new();
    for table in builtin_tables() {
        let fqn = format!("bifrost.{}.{}", table.namespace, table.name);
        let description = query
            .describe_table(&format!("bifrost.{}", table.namespace), table.name)
            .await?;
        expect_eq(
            &format!("{fqn} correlation inputs"),
            &correlation(&description),
            &vec![SUBJECT_CARD_UID.to_owned(), RUN_ID.to_owned()],
        )?;
        expect_eq(
            &format!("{fqn} resolved layout"),
            &description.physical_layout,
            &(table.physical_layout)(),
        )?;
        for dimension in SHARED_DIMENSIONS {
            let declared: Vec<_> = description
                .user_fields
                .iter()
                .filter(|field| field.name == dimension)
                .map(|field| (field.data_type.clone(), field.nullable))
                .collect();
            expect_eq(
                &format!("{fqn} {dimension}"),
                &declared,
                &vec![(wyrd_spec::vala::api::DataTypeSpec::Utf8, true)],
            )?;
        }
        if let Some(field) = description.user_fields.iter().find(|field| {
            is_managed_column(&field.name) || ["card_uid", "owner_card_uid"].contains(&&*field.name)
        }) {
            return Err(format!("{fqn} declares server-owned `{}`", field.name).into());
        }
        fqns.push(fqn);
    }
    let (namespace, name) = dynamic
        .rsplit_once('.')
        .ok_or("the dynamic FQN names its table")?;
    let description = query.describe_table(namespace, name).await?;
    expect_eq(
        "dynamic declared fields",
        &description
            .user_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        &vec!["id", "value"],
    )?;
    expect_eq(
        "dynamic correlation inputs",
        &correlation(&description),
        &vec![SUBJECT_CARD_UID.to_owned(), RUN_ID.to_owned()],
    )?;
    fqns.push(dynamic.clone());
    let managed = MANAGED_COLUMNS
        .iter()
        .map(|column| column.field.name)
        .collect::<Vec<_>>()
        .join(", ");
    for fqn in &fqns {
        journey
            .rows(format!("SELECT {managed} FROM {fqn} LIMIT 1"))
            .await
            .map_err(|error| format!("{fqn} does not read the managed envelope: {error}"))?;
    }
    if journey
        .rows(format!("SELECT {} FROM {dynamic}", SHARED_DIMENSIONS[0]))
        .await
        .is_ok()
    {
        return Err("a dynamic table reads a shared dimension".into());
    }

    let results = journey.publish_results().await?;
    let call = journey.gateway.capture_call().await?;
    let decision = journey.stage_audit_decision().await?;
    server.flush_bifrost().await?;
    let envelope = format!(
        "{WORKLOAD_CARD_UID}, {SUBJECT_CARD_UID}, {SUB_PRINCIPAL_ID}, {ACT_PRINCIPAL_ID}, \
         {WYRD_INGESTED_AT} IS NOT NULL"
    );
    let writer = json!(results.writer.to_string());
    let subject = json!(results.subject.as_str());
    expect_eq(
        "result envelope",
        &journey
            .rows(format!(
                "SELECT {envelope} FROM bifrost.verification.results WHERE result_id = '{}'",
                results.scored_drift.0
            ))
            .await?,
        &vec![vec![
            subject.clone(),
            subject,
            writer.clone(),
            writer,
            json!(true),
        ]],
    )?;
    let calls = journey
        .rows(format!(
            "SELECT wyrd_request_id = request_id, {SUB_PRINCIPAL_ID} IS NOT NULL \
             FROM bifrost.gateway.calls WHERE request_id = '{}'",
            call.request_id
        ))
        .await?;
    expect_eq(
        "call envelope",
        &calls,
        &vec![vec![json!(true), json!(true)]],
    )?;
    let principal = json!(decision.principal);
    expect_eq(
        "audit envelope",
        &journey
            .rows(format!(
                "SELECT {envelope}, wyrd_request_id FROM bifrost.system.audit_log \
                 WHERE operation = '{}'",
                decision.operation
            ))
            .await?,
        &vec![vec![
            Value::Null,
            Value::Null,
            principal.clone(),
            principal,
            json!(true),
            json!(decision.request_id.as_str()),
        ]],
    )?;

    journey.gateway.server.shutdown().await?;
    Ok(())
}

/// Trusted producers sharing one tenant outbox keep each row's own origin.
///
/// Two root Services of different families and versions, each acting through
/// its own runtime account under its own deployment environment, judge the
/// same shared Agent component with one Drift Verifier. Each stages its
/// pinned inputs row and its result through the production payload builder
/// on the process outbox before one flush, so their rows travel together.
/// Every retained `execution_inputs`, `results`, and `result_features` row
/// names its own owning Service as workload and by name and version, its own
/// account pair and environment, and the shared Agent by UID, kind, name,
/// and version; no row takes another producer's or the delivering worker's
/// identity.
///
/// # Errors
///
/// Returns a seeding, payload, flush, or query error, or the first table
/// whose origins differ.
///
/// # Panics
///
/// Panics only if `#[tokio::test]` cannot build its runtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn trusted_delivery_preserves_each_origin() -> Result<(), ServerJourneyError> {
    let journey = TypedPayloadJourney::start().await?;
    let server = &journey.gateway.server;
    let tenant = journey.gateway.tenant;
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    seed.declare(
        "Prompt",
        "origin-prompt",
        &json!({
            "provider": "openai",
            "model": "gpt-4o-mini",
            "messages": [{ "role": "user", "content": "Answer." }],
        }),
    )
    .await?;
    let agent = seed
        .declare(
            "Agent",
            "origin-agent",
            &json!({
                "prompt": { "kind": "Prompt", "name": "origin-prompt", "version": "1.0.0" },
                "execution_route": { "kind": "native" },
            }),
        )
        .await?;
    let verifier = seed.drift_verifier("origin-drift").await?;
    for (namespace, name) in [
        ("verification", "execution_inputs"),
        ("verification", "results"),
        ("drift", "result_features"),
    ] {
        server
            .ensure_builtin_table_for_test(tenant, namespace, name)
            .await?;
    }
    let now = Utc::now();
    let window = DriftWindow {
        start: now - chrono::Duration::hours(1),
        end: now,
    };
    let outbox = server.state().bifrost.scribe_outbox();
    let mut expected = Vec::new();
    for (name, version, environment) in [
        ("origin-billing", "2.0.0", "staging"),
        ("origin-checkout", "1.0.0", "prod"),
    ] {
        let (root, account) = seed.release(name, version).await?;
        let builder = ResultPayloadBuilder::new(
            ResultRun {
                execution_id: ExecutionId::new_v7(),
                run_id: None,
                verifier_version: "1.0.0",
                card_uid: &agent,
                owner_card_uid: Some(&root),
                group_id: None,
                deployment_environment: Some(environment),
                binding_id: Some(BindingId::new_v7()),
                trigger: None,
                window: Some(&window),
            },
            &verifier,
            now,
            now,
        );
        let attribution = Attribution::direct(account);
        outbox.stage(
            tenant,
            ScribeWrite::ExecutionInputs {
                row: builder.inputs(&BTreeMap::new(), None)?,
                attribution,
                queued: None,
            },
        );
        outbox.stage(
            tenant,
            ScribeWrite::Result {
                payload: builder.build(&drift_outcome(Some(two_feature_report()?))?)?,
                attribution,
            },
        );
        expected.push(
            [
                root.as_str(),
                agent.as_str(),
                &account.to_string(),
                &account.to_string(),
                name,
                version,
                environment,
                "Agent",
                "origin-agent",
                "1.0.0",
            ]
            .map(|value| json!(value))
            .to_vec(),
        );
    }
    server.flush_bifrost().await?;

    for table in [
        "bifrost.verification.execution_inputs",
        "bifrost.verification.results",
        "bifrost.drift.result_features",
    ] {
        expect_eq(
            &format!("{table} origins"),
            &journey
                .rows(format!(
                    "SELECT DISTINCT {WORKLOAD_CARD_UID}, {SUBJECT_CARD_UID}, \
                     {SUB_PRINCIPAL_ID}, {ACT_PRINCIPAL_ID}, service_name, service_version, \
                     deployment_environment, subject_kind, subject_name, subject_version \
                     FROM {table} WHERE verifier_card_uid = '{verifier}' ORDER BY service_name"
                ))
                .await?,
            &expected,
        )?;
    }

    journey.gateway.server.shutdown().await?;
    Ok(())
}

/// Raw Arrow IPC that bypasses every client-side Variant check reaches Scribe
/// admission for a top-level and a nested signal Variant, and each malformed
/// frame is refused with its exact catalogued error before any ACK.
///
/// `bifrost.logs.records` covers a missing and a foreign extension marker,
/// the Variant marker with foreign extension metadata, invalid bytes, one
/// container past the depth limit, a compact value nested 20,000 levels deep
/// that is refused for depth without taking the server down, and one byte
/// past the size limit. Malformed bytes below an over-deep container or
/// beside an over-deep sibling in either order are refused as invalid, and a
/// Decimal16 outside the exact numeric domain — fractional, past `u64::MAX`,
/// or a scale-zero value within `i64` — is refused for numeric range, also
/// beside an over-deep sibling. A `bifrost.metrics.points` frame whose absent
/// bucket set keeps a child value is refused as a partial Struct.
/// Precedence is pinned: an oversized value that is also
/// malformed reports its size, a wrong marker on one field outranks invalid
/// bytes in another, and an undeclared column or a non-Variant type change,
/// each a schema-conformance conflict with the registered schema, outranks
/// each of invalid, over-deep, and oversized Variant bytes. A JSON
/// row whose Variant holds an out-of-range integer beside an over-deep branch
/// is refused for numeric range in both key orders, and also when the integer
/// sits inside the over-deep branch, before any ACK; that JSON row encodes
/// against `bifrost.verification.execution_inputs` on the client.
/// `bifrost.traces.spans` covers the Variant nested in `events`, with a missing
/// marker and with foreign extension metadata, named by its top-level column.
/// A non-Variant type change on a signal column is refused as a
/// schema-conformance conflict. Afterward the same server accepts the valid sentinel frames and only
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
    admission
        .refuse_conformance("a non-Variant type change", RECORDS, &wide_severity(frame)?)
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
            .refuse_conformance(
                &format!("an undeclared column outranks {defect} Variant bytes"),
                RECORDS,
                &undeclared(frame()?)?,
            )
            .await?;
        admission
            .refuse_conformance(
                &format!("a non-Variant type change outranks {defect} Variant bytes"),
                RECORDS,
                &wide_severity(frame()?)?,
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
const RECORDS: &str = "bifrost.logs.records";

/// The server-written built-in the public JSON-row facade encodes against;
/// its Variant refusals surface on the client before anything is queued.
const EXECUTION_INPUTS: &str = "bifrost.verification.execution_inputs";

/// The signal built-in whose nested `events` Variant the journey writes.
const SPANS: &str = "bifrost.traces.spans";

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
const POINTS: &str = "bifrost.metrics.points";

/// One bound server and an admin ingest door that sends raw Arrow IPC frames.
struct VariantAdmissionJourney {
    /// Server, tenant, and published-row reader.
    journey: TypedPayloadJourney,
    /// Ingest transport that sends frames exactly as built.
    ingest: RawIngest,
    /// Admin client the public Bifrost facade writes JSON rows through.
    client: wyrd_client::WyrdClient,
    /// The described `bifrost.logs.records` user schema.
    records: SchemaRef,
    /// The described `bifrost.traces.spans` user schema.
    spans: SchemaRef,
    /// The described `bifrost.metrics.points` user schema.
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
    /// schema change. No supplied body is a string, so the derived
    /// `body_text` is null.
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
        with_text(
            &RecordBatch::try_new(Arc::new(Schema::new(fields)), columns)?,
            "body_text",
            None,
        )
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

    /// Require that `batch` is refused as a schema-conformance conflict
    /// against its table's registered schema.
    ///
    /// # Errors
    ///
    /// Returns a description naming `what` when the frame is acked or the
    /// refusal is any other error.
    async fn refuse_conformance(
        &self,
        what: &str,
        table: &str,
        batch: &RecordBatch,
    ) -> Result<(), ServerJourneyError> {
        match self.refusal(table, batch).await? {
            WyrdError::Vala {
                error: BifrostError::FingerprintMismatch { .. },
            } => Ok(()),
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
    /// The Service both results judge, which also owns their bindings.
    subject: CardUid,
    /// The tenant SYSTEM principal that wrote both results.
    writer: uuid::Uuid,
}

/// The staged audit decision and its native detail.
struct StagedDecision {
    /// The decision's unique operation, which selects its retained row.
    operation: String,
    /// The detail's canonical JSON, the value the retained row must decode to.
    detail: Value,
    /// The principal the decision was for and by.
    principal: PrincipalId,
    /// The request that made the decision.
    request_id: RequestId,
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
                    deployment_environment: None,
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
            self.gateway.server.state().bifrost.scribe_outbox().stage(
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
            subject,
            writer: seed.system_principal(),
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
        let principal = PrincipalId::new(uuid::Uuid::now_v7());
        let request_id = RequestId::now_v7();
        let event = AuditEvent::new(
            request_id.clone(),
            None,
            operation.clone(),
            "vala.oracle.admission".to_owned(),
            None,
            Attribution::direct(principal),
            PrincipalKindTag::User,
            "bifrost:query:read".to_owned(),
            AuditOutcome::Allowed,
        )
        .with_detail(detail.clone());
        self.gateway
            .server
            .state()
            .bifrost
            .scribe_outbox()
            .stage(self.gateway.tenant, event);
        self.gateway.server.await_audit_retained().await?;
        Ok(StagedDecision {
            operation,
            detail: serde_json::from_str(&audit_detail_canonical_json(&detail))?,
            principal,
            request_id,
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
    // B delegates while running as the verified subject, which its Card
    // scope must cover.
    let delegated = super::query::client_for(&server, b.api_key().ok_or("B carries an API key")?)?
        .for_workload(subject.clone())
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
        .bifrost
        .scribe_outbox()
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
            "SELECT result_id FROM bifrost.verification.results WHERE result_id = '{result}' \
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
/// registered span table as the accepted caller on each of its three
/// attempts: the absent trace defers the first two, and the last judges the
/// requirement inconclusive and completes. Every retained read decision of
/// that execution is recorded for A with SYSTEM as actor and carries exactly
/// the accepted chain `[B]`.
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
    // B delegates while running as the verified subject, which its Card
    // scope must cover.
    let delegated = super::query::client_for(&server, b.api_key().ok_or("B carries an API key")?)?
        .for_workload(subject.clone())
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
        if row.status == "retrying" {
            seed.make_retry_due(execution).await?;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    if (row.status.as_str(), row.attempts) != ("completed", 3) {
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
    if reads.len() < 3 {
        return Err(format!("expected a decision for A per attempt, read {reads:?}").into());
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
    seed.activate(owner_principal, &owner).await?;
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
        .bifrost
        .scribe_outbox()
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
            "SELECT result_id FROM bifrost.verification.results WHERE result_id = '{result}' \
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
    // B delegates while running as the verified subject, which its Card
    // scope must cover.
    let b_token = async || -> Result<String, ServerJourneyError> {
        Ok(server
            .exchange_api_key_for_workload(b.api_key().ok_or("B carries an API key")?, &subject)
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
        .claim(&mut conn, chrono::Duration::minutes(1), 1)
        .await?
        .pop()
        .ok_or("the queued execution is claimable")?;
    conn.commit().await?;
    let system = PrincipalId::new(seed.system_principal());
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
                wyrd_server::components::gateway::GatewayCallContext {
                    subject_card_uid: Some(card_uid),
                    ..Default::default()
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

/// Request body ceiling the shared-startup journey injects, well under the
/// production default so an ordinary body proves the injected value governs.
const STARTUP_BODY_BYTES: usize = 16 * 1024;

/// Poll `execution` until the runtime settles it as completed or errored.
///
/// # Errors
/// Returns a fixture error, or a description when the execution is not
/// settled within [`WAIT`].
async fn settled_execution(
    seed: &VerificationFixture,
    execution: ExecutionId,
) -> Result<ExecutionRow, ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let row = seed.execution(execution).await?;
        if matches!(row.status.as_str(), "completed" | "errored") {
            return Ok(row);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the execution was never settled: {row:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Production and harness servers start the same complete graph from the
/// same configuration and injected inputs (AC-010/011, INV-001/003).
///
/// A server whose hosted blocks must prewarm a worker that does not exist
/// fails startup after its Bifrost roles were acquired; the next server
/// opens the same durable data root, so the failed start released it. That server is composed with
/// an injected body ceiling and the queued runtime: its state carries the
/// injected limit and development profile, a body over the ceiling is
/// refused at a buffered public route, and an execution submitted to the queue is
/// completed by the runtime startup returned. A last server over the missing
/// worker with no prewarm starts lazily, and without the queued runtime its
/// runner never comes up.
///
/// # Errors
/// Returns server, seeding, or request errors, or a description of the
/// first startup, limit, or runtime expectation that does not hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn shared_startup_uses_complete_injected_engines() -> Result<(), ServerJourneyError> {
    let scratch = tempfile::tempdir()?;
    let volume = scratch.path().join("volume");
    std::fs::create_dir_all(&volume)?;
    let missing = scratch.path().join("no-such-worker");
    let builder = || WyrdTestServer::builder().with_durable_bifrost_data_root(volume.clone());

    let refused = Box::pin(
        builder()
            .with_verification_worker_for_test(missing.clone())
            .with_verification_limits_for_test(HostedLimits {
                monty_min_workers: 1,
                ..HostedLimits::default()
            })
            .start_bound(),
    )
    .await;
    match refused {
        Ok(server) => {
            server.shutdown().await?;
            return Err("a prewarmed missing worker must fail startup".into());
        }
        Err(error)
            if error
                .to_string()
                .contains("hosted Verifier blocks could not start") => {}
        Err(error) => return Err(format!("startup failed for another reason: {error}").into()),
    }

    let server = Box::pin(
        builder()
            .with_limits_for_test(LimitsConfig {
                body_bytes: STARTUP_BODY_BYTES,
                ..LimitsConfig::default()
            })
            .with_verification_runtime_for_test()
            .start_bound(),
    )
    .await?;
    let state = server.state();
    if state.settings.limits.body_bytes != STARTUP_BODY_BYTES
        || state.settings.deployment_profile.is_production()
    {
        return Err(format!("the injected settings were not kept: {:?}", state.settings).into());
    }
    let caller = server.bootstrap_service("startup-caller", &[]).await?;
    let token = server
        .exchange_api_key(caller.api_key().ok_or("the caller carries an API key")?)
        .await?;
    let oversized = reqwest::Client::new()
        .post(format!(
            "{}/v1/cards/upload/init",
            server.base_url().ok_or("the server is bound")?
        ))
        .header("x-wyrd-access-token", format!("Bearer {token}"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(vec![b' '; 2 * STARTUP_BODY_BYTES])
        .send()
        .await?;
    if oversized.status() != reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return Err(format!("an oversized body answered {}", oversized.status()).into());
    }

    let seed =
        VerificationFixture::provision(state.postgres.wyrd(), server.data_tenant_id()).await?;
    let (subject, _) = seed.service("startup-subject").await?;
    let verifier = assert_verifier(&seed, "startup-assert").await?;
    let execution = seed
        .submit(
            &verifier,
            &subject,
            BTreeMap::from([(
                SlotName::new("x")?,
                InputDeclaration::Json { value: json!(true) },
            )]),
        )
        .await?;
    let row = settled_execution(&seed, execution).await?;
    if row.status != "completed" || !state.verification.health.is_up(RuntimeCapability::Runner) {
        return Err(format!("the started runtime did not complete the run: {row:?}").into());
    }
    server.shutdown().await?;

    // A fresh fixture database cannot replay the previous server's WAL, so
    // the lazy server mounts no shared data root.
    let lazy = Box::pin(
        WyrdTestServer::builder()
            .with_verification_worker_for_test(missing)
            .start_bound(),
    )
    .await?;
    let runner_up = lazy
        .state()
        .verification
        .health
        .is_up(RuntimeCapability::Runner);
    lazy.shutdown().await?;
    if runner_up {
        return Err("a server without the queued runtime reports its runner up".into());
    }
    Ok(())
}

/// One bound server whose gateway reaches a recording mock upstream, and
/// the administration and invocation requests the refresh journey sends.
struct RefreshJourney {
    /// The bound server under test.
    server: WyrdTestServer,
    /// Mock upstream answering every request with one completion.
    upstream: wiremock::MockServer,
    /// Plain HTTP client standing in for the SDK transport.
    http: reqwest::Client,
    /// The server's base URL.
    base: String,
}

impl RefreshJourney {
    /// Start a bound server whose built-in adapters target a fresh mock
    /// upstream answering every request with one completion.
    ///
    /// # Errors
    /// Returns a mock, URL, or server startup error.
    async fn start() -> Result<Self, ServerJourneyError> {
        Self::start_with(WyrdTestServer::builder()).await
    }

    /// Start `builder` bound, with its built-in adapters targeting a fresh
    /// mock upstream answering every request with one completion.
    ///
    /// # Errors
    /// Returns a mock, URL, or server startup error.
    async fn start_with(builder: WyrdTestServerBuilder) -> Result<Self, ServerJourneyError> {
        let upstream = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(GatewayCapture::completion()),
            )
            .mount(&upstream)
            .await;
        let root = url::Url::parse(&upstream.uri())?;
        let server = Box::pin(
            builder
                .with_gateway_provider_root_for_test(root)
                .start_bound(),
        )
        .await?;
        let base = server.base_url().ok_or("the server is bound")?.to_owned();
        Ok(Self {
            server,
            upstream,
            http: reqwest::Client::new(),
            base,
        })
    }

    /// Access token of a new administrator named `name` in `tenant`, or in
    /// the fixture tenant when `tenant` is `None`.
    ///
    /// # Errors
    /// Returns a bootstrap or exchange error.
    async fn admin(
        &self,
        tenant: Option<DataTenantId>,
        name: &str,
    ) -> Result<String, ServerJourneyError> {
        let admin = match tenant {
            Some(tenant) => {
                self.server
                    .bootstrap_service_in_tenant(tenant, name, &["admin"])
                    .await?
            }
            None => self.server.bootstrap_service(name, &["admin"]).await?,
        };
        Ok(self
            .server
            .exchange_api_key(admin.api_key().ok_or("the admin carries an API key")?)
            .await?)
    }

    /// Store `admin`'s tenant deployment `acme`, serving any model from the
    /// mock upstream under `prefix`, through the public administration route.
    ///
    /// # Errors
    /// Returns a request error or a description of a refused write.
    async fn deploy(&self, admin: &str, prefix: &str) -> Result<(), ServerJourneyError> {
        let response = self
            .http
            .put(format!(
                "{}/v1/admin/gateway/provider-deployments/acme",
                self.base
            ))
            .header("x-wyrd-access-token", format!("Bearer {admin}"))
            .json(&json!({
                "name": "acme",
                "adapter": {"openai_compatible": {
                    "base_url": format!("{}/{prefix}/v1", self.upstream.uri())
                }},
                "auth": "none",
                "capabilities": ["chat_completions"],
            }))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(format!(
                "deploying {prefix}: {} {}",
                response.status(),
                response.text().await?
            )
            .into());
        }
        Ok(())
    }

    /// Call `acme/m` as `caller` and return the upstream path the call
    /// reached, or `None` when the gateway refused it before dispatch.
    ///
    /// # Errors
    /// Returns a request or recording error.
    async fn call(&self, caller: &str) -> Result<Option<String>, ServerJourneyError> {
        let before = self.upstream_paths().await?.len();
        let response = self
            .http
            .post(format!("{}/v1/chat/completions", self.base))
            .bearer_auth(caller)
            .json(&json!({
                "model": "acme/m",
                "max_completion_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await?;
        let answered = response.status().is_success();
        response.bytes().await?;
        let mut paths = self.upstream_paths().await?;
        if !answered {
            return if paths.len() == before {
                Ok(None)
            } else {
                Err("a refused call reached the upstream".into())
            };
        }
        Ok(paths.pop().filter(|_| paths.len() == before))
    }

    /// Call as `caller` until the call reaches `expected` or [`WAIT`]
    /// passes, and return the last route reached.
    ///
    /// An administrative write reaches the snapshot cache only through its
    /// committed notification, or through the listener noticing a lost
    /// session and forgetting every snapshot, so a call right after the write
    /// may still route on the previous snapshot for that latency. A snapshot
    /// that is never forgotten keeps the old route past the deadline.
    ///
    /// # Errors
    /// Returns a request or recording error from any call.
    async fn route_after_write(
        &self,
        caller: &str,
        expected: Option<&str>,
    ) -> Result<Option<String>, ServerJourneyError> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let route = self.call(caller).await?;
            if route.as_deref() == expected || tokio::time::Instant::now() >= deadline {
                return Ok(route);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Every path the mock upstream received, oldest first.
    ///
    /// # Errors
    /// Returns a description when request recording is off.
    async fn upstream_paths(&self) -> Result<Vec<String>, ServerJourneyError> {
        Ok(self
            .upstream
            .received_requests()
            .await
            .ok_or("request recording is on")?
            .into_iter()
            .map(|request| request.url.path().to_owned())
            .collect())
    }

    /// Number of this server's Postgres sessions waiting on `LISTEN`; when
    /// `terminate` is set, each is terminated as a lost listener connection.
    ///
    /// The sessions are selected before any is terminated: in one statement
    /// the planner may evaluate the termination before the database filter,
    /// ending concurrent tests' listeners in other databases on the same
    /// instance.
    ///
    /// # Errors
    /// Returns a pool or statement error.
    async fn listeners(&self, terminate: bool) -> Result<i64, ServerJourneyError> {
        let superuser = self.server.pg_fixture().superuser_pool()?;
        let pids: Vec<i32> = sqlx::query_scalar(
            "SELECT pid FROM pg_stat_activity WHERE datname = current_database() \
             AND query ILIKE 'LISTEN%' AND pid <> pg_backend_pid()",
        )
        .fetch_all(&superuser)
        .await?;
        if terminate {
            sqlx::query("SELECT pg_terminate_backend(pid) FROM unnest($1::int[]) AS pid")
                .bind(&pids)
                .execute(&superuser)
                .await?;
        }
        Ok(i64::try_from(pids.len())?)
    }
}

/// Engine-owned refresh keeps every tenant's gateway changes current
/// through listener loss, without one tenant seeing another's snapshot
/// (REQ-005/013, INV-001/003, AC-010/011).
///
/// Tenant A's administrator stores deployment `acme` and calls it, then
/// moves it twice: once while the server's change listeners are connected,
/// and once right after every listener session is terminated. Calls after
/// each move reach the newest upstream once the write's notification or the
/// listener's loss is handled, so a lost listener leaves no stale snapshot.
/// Once the listeners reconnect a further move is still seen. Tenant B
/// cannot call A's deployment, and B's own `acme` reaches only B's upstream
/// while A keeps its own.
///
/// # Errors
/// Returns server, request, or Postgres errors, or a description of the
/// first stale, leaked, or missing route.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn engine_owned_refresh_preserves_tenant_changes() -> Result<(), ServerJourneyError> {
    let journey = RefreshJourney::start().await?;
    let a = journey.admin(None, "refresh-a").await?;
    let other = journey.server.seed_tenant("refresh-other").await?;
    let b = journey.admin(Some(other), "refresh-b").await?;
    let reaches = |prefix: &str| Some(format!("/{prefix}/v1/chat/completions"));

    journey.deploy(&a, "one").await?;
    expect_eq("A's first route", &journey.call(&a).await?, &reaches("one"))?;
    expect_eq(
        "B before its own route",
        &journey.call(&b).await?,
        &None::<String>,
    )?;
    journey.deploy(&a, "two").await?;
    let two = reaches("two");
    expect_eq(
        "A's moved route",
        &journey.route_after_write(&a, two.as_deref()).await?,
        &two,
    )?;

    if journey.listeners(true).await? == 0 {
        return Err("startup composed no engine-owned change listener".into());
    }
    journey.deploy(&a, "three").await?;
    let three = reaches("three");
    expect_eq(
        "A's route after listener loss",
        &journey.route_after_write(&a, three.as_deref()).await?,
        &three,
    )?;
    let deadline = tokio::time::Instant::now() + WAIT;
    while journey.listeners(false).await? == 0 {
        if tokio::time::Instant::now() >= deadline {
            return Err("the change listeners never reconnected".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    journey.deploy(&a, "four").await?;
    let four = reaches("four");
    expect_eq(
        "A's route after reconnect",
        &journey.route_after_write(&a, four.as_deref()).await?,
        &four,
    )?;

    journey.deploy(&b, "bee").await?;
    let bee = reaches("bee");
    expect_eq(
        "B's own route",
        &journey.route_after_write(&b, bee.as_deref()).await?,
        &bee,
    )?;
    expect_eq(
        "A beside B's route",
        &journey.call(&a).await?,
        &reaches("four"),
    )?;
    journey.server.shutdown().await?;
    Ok(())
}

/// The gateway snapshot cache's notification channel.
const SNAPSHOT_CHANNEL: &str = "wyrd_gateway_snapshot";

/// Production bound on noticing a silently stalled listener: one 30 s
/// heartbeat interval plus its 5 s probe timeout.
const STALL_DETECTION: Duration = Duration::from_secs(35);

/// Scheduling allowance this journey grants beyond [`STALL_DETECTION`].
const STALL_TOLERANCE: Duration = Duration::from_secs(10);

/// Wait until `proxy` relays at least one live subscription to `channel`.
///
/// # Errors
/// Returns a description when no subscription arrives within [`WAIT`].
async fn subscribed(proxy: &ListenerStallProxy, channel: &str) -> Result<(), ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    while proxy.listening(channel) == 0 {
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("no listener subscribed to {channel}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// A silently stalled snapshot listener stops serving cached routes within
/// the heartbeat bound and recovers once its path returns (REQ-013,
/// AC-011).
///
/// The server's runtime pools dial Postgres through a proxy. Once tenant A's
/// route is cached, the proxy stops relaying the snapshot listener's
/// connection without closing either socket, so no notification and no loss
/// report reach the server; request connections keep flowing. A's next
/// move commits normally, and calls reach the new route within
/// [`STALL_DETECTION`] plus [`STALL_TOLERANCE`] while every subscription
/// is still stalled, so they read Postgres. Tenant B keeps its own route
/// throughout. After the path is restored and a listener subscribes again, a
/// further move is still seen.
///
/// # Errors
/// Returns server, proxy, request, or Postgres errors, or a description of
/// the first stale, leaked, or late route.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn silent_listener_stall_falls_back_and_recovers() -> Result<(), ServerJourneyError> {
    expect_eq(
        "production stall detection bound",
        &(wyrd_sql::listener::HEARTBEAT + wyrd_sql::listener::PROBE_TIMEOUT),
        &STALL_DETECTION,
    )?;
    let dsns = wyrd_sql::dsn::ResolvedDsns::from_env()?;
    let proxy = Arc::new(ListenerStallProxy::start(&dsns).await?);
    let journey = RefreshJourney::start_with(
        WyrdTestServer::builder().with_postgres_proxy_for_test(Arc::clone(&proxy)),
    )
    .await?;
    let a = journey.admin(None, "stall-a").await?;
    let other = journey.server.seed_tenant("stall-other").await?;
    let b = journey.admin(Some(other), "stall-b").await?;
    let reaches = |prefix: &str| Some(format!("/{prefix}/v1/chat/completions"));

    subscribed(&proxy, SNAPSHOT_CHANNEL).await?;
    journey.deploy(&a, "one").await?;
    journey.deploy(&b, "bee").await?;
    let (one, bee) = (reaches("one"), reaches("bee"));
    expect_eq(
        "A's cached route",
        &journey.route_after_write(&a, one.as_deref()).await?,
        &one,
    )?;
    expect_eq(
        "B's cached route",
        &journey.route_after_write(&b, bee.as_deref()).await?,
        &bee,
    )?;

    if proxy.stall(SNAPSHOT_CHANNEL) == 0 {
        return Err("no subscribed snapshot listener was stalled".into());
    }
    let stalled_at = tokio::time::Instant::now();
    journey.deploy(&a, "two").await?;
    let two = reaches("two");
    expect_eq(
        "A's route during a silent stall",
        &journey.route_after_write(&a, two.as_deref()).await?,
        &two,
    )?;
    let detected = stalled_at.elapsed();
    tracing::info!(?detected, "stalled snapshot listener detected");
    if detected > STALL_DETECTION + STALL_TOLERANCE {
        return Err(format!("the stalled listener served a stale route for {detected:?}").into());
    }
    if proxy.listening(SNAPSHOT_CHANNEL) != 0 {
        return Err("a snapshot listener subscribed through a stalled path".into());
    }
    expect_eq("B during A's stall", &journey.call(&b).await?, &bee)?;

    proxy.restore(SNAPSHOT_CHANNEL);
    subscribed(&proxy, SNAPSHOT_CHANNEL).await?;
    expect_eq("A after recovery", &journey.call(&a).await?, &two)?;
    journey.deploy(&a, "three").await?;
    let three = reaches("three");
    expect_eq(
        "A's route after recovery",
        &journey.route_after_write(&a, three.as_deref()).await?,
        &three,
    )?;
    expect_eq("B after recovery", &journey.call(&b).await?, &bee)?;
    journey.server.shutdown().await?;
    Ok(())
}

/// What a shared cache listener told its cache, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Heard {
    /// `reset(live)`: the cache was cleared and now serves only when `live`.
    Reset(bool),
    /// `notify(payload)`.
    Notified(String),
}

/// One shared cache listener running under test, recording each callback
/// in place of a cache.
struct Watched {
    /// Every callback so far.
    heard: Arc<std::sync::Mutex<Vec<Heard>>>,
    /// Cancels the listener.
    shutdown: CancellationToken,
    /// The listener's worker.
    task: tokio::task::JoinHandle<()>,
}

impl Watched {
    /// Spawn `listener`'s watch, recording its callbacks.
    fn start(listener: wyrd_sql::NotificationListener) -> Self {
        let heard = Arc::new(std::sync::Mutex::new(Vec::new()));
        let shutdown = CancellationToken::new();
        let (resets, notes, token) = (Arc::clone(&heard), Arc::clone(&heard), shutdown.clone());
        let task = tokio::spawn(async move {
            listener
                .watch(
                    token,
                    |live| Self::push(&resets, Heard::Reset(live)),
                    |payload| Self::push(&notes, Heard::Notified(payload.to_owned())),
                )
                .await;
        });
        Self {
            heard,
            shutdown,
            task,
        }
    }

    /// Append one callback to `heard`.
    fn push(heard: &std::sync::Mutex<Vec<Heard>>, event: Heard) {
        heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
    }

    /// Every callback so far.
    fn heard(&self) -> Vec<Heard> {
        self.heard
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Whether the last reset switched the cache on.
    fn live(&self) -> bool {
        self.heard()
            .iter()
            .rev()
            .find_map(|heard| match heard {
                Heard::Reset(live) => Some(*live),
                Heard::Notified(_) => None,
            })
            .unwrap_or(false)
    }

    /// The resets heard since the first `skip` callbacks, in order.
    fn resets_since(&self, skip: usize) -> Vec<bool> {
        self.heard()
            .into_iter()
            .skip(skip)
            .filter_map(|heard| match heard {
                Heard::Reset(live) => Some(live),
                Heard::Notified(_) => None,
            })
            .collect()
    }

    /// Whether `payload` was delivered.
    fn notified(&self, payload: &str) -> bool {
        self.heard().contains(&Heard::Notified(payload.to_owned()))
    }

    /// Cancel the listener and return how long it took to finish, failing
    /// unless it ends within the production shutdown drain with its cache
    /// switched off.
    ///
    /// # Errors
    /// Returns a join error, or a description of a late or live finish.
    async fn stop(self) -> Result<Duration, ServerJourneyError> {
        let started = tokio::time::Instant::now();
        self.shutdown.cancel();
        tokio::time::timeout(SHUTDOWN_DRAIN, self.task)
            .await
            .map_err(|_| "the cancelled listener outlived the shutdown drain")??;
        let elapsed = started.elapsed();
        if self
            .heard
            .lock()
            .map(|heard| heard.last().cloned())
            .ok()
            .flatten()
            != Some(Heard::Reset(false))
        {
            return Err("the cancelled listener left its cache serving".into());
        }
        Ok(elapsed)
    }
}

/// The production shutdown drain every engine worker must finish within.
const SHUTDOWN_DRAIN: Duration = Duration::from_secs(35);

/// Wait until `ready` holds, polling, or fail naming `what` after [`WAIT`].
///
/// # Errors
/// Returns a description naming `what` when it never holds.
async fn until(what: &str, ready: impl Fn() -> bool) -> Result<(), ServerJourneyError> {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !ready() {
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("timed out waiting for {what}").into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

/// Send `payload` on `channel` over a connection that bypasses the proxy.
///
/// # Errors
/// Returns the statement error.
async fn notify_channel(
    pool: &sqlx::PgPool,
    channel: &str,
    payload: &str,
) -> Result<(), ServerJourneyError> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(channel)
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}

/// A fixture database, a stall proxy in front of it, and a Wyrd handle
/// whose pools dial the proxy, with the process telemetry installed so
/// `WYRD_LOG` traces the listener.
///
/// # Errors
/// Returns a telemetry, fixture, proxy, or connection error.
async fn proxied_postgres() -> Result<
    (
        wyrd_dev_fixtures::pg::PgFixture,
        ListenerStallProxy,
        wyrd_sql::WyrdPostgres,
    ),
    ServerJourneyError,
> {
    wyrd_testing::bifrost::shared_process_telemetry_for_test()?;
    let fixture = wyrd_dev_fixtures::pg::PgFixture::start().await?;
    let dsns = fixture.runtime_dsns()?;
    let proxy = ListenerStallProxy::start(&dsns).await?;
    let postgres = wyrd_sql::WyrdPostgres::connect_from_dsns(&proxy.route(&dsns)?).await?;
    Ok((fixture, proxy, postgres))
}

/// A detected listener loss switches the cache off before any replacement
/// subscribes, and a failed start keeps it off (REQ-013, AC-011).
///
/// The shared listener's pool dials Postgres through a stall proxy. Started
/// while its channel is stalled, it never reports a live cache and retries
/// at most once per reconnect delay, releasing each stalled attempt. Once
/// the channel is restored it subscribes and delivers a notification. Its
/// backend is then terminated while every new subscription is held
/// pending: the cache is switched off before the replacement can subscribe
/// and stays off while it waits. After the path is restored the replacement
/// switches the cache on, from empty, and delivers the next notification.
///
/// # Errors
/// Returns fixture, proxy, or statement errors, or a description of the
/// first live cache during recovery.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn listener_loss_disables_cache_before_reconnect() -> Result<(), ServerJourneyError> {
    const CHANNEL: &str = "wyrd_listener_loss";
    let (fixture, proxy, postgres) = proxied_postgres().await?;

    proxy.stall(CHANNEL);
    let started = tokio::time::Instant::now();
    let watched = Watched::start(postgres.listener(CHANNEL));
    until("two stalled subscription attempts", || {
        proxy.subscriptions(CHANNEL) >= 2
    })
    .await?;
    let attempts = proxy.subscriptions(CHANNEL);
    let allowed = started.elapsed().as_secs() / wyrd_sql::listener::RECONNECT_DELAY.as_secs() + 1;
    if u64::try_from(attempts)? > allowed {
        return Err(format!("{attempts} subscription attempts spun past {allowed}").into());
    }
    if watched.resets_since(0).contains(&true) {
        return Err("a stalled start switched the cache on".into());
    }
    if proxy.stalled_open(CHANNEL) > 1 {
        return Err("stalled subscription attempts were retained".into());
    }

    proxy.restore(CHANNEL);
    until("a live subscription", || {
        watched.live() && proxy.listening(CHANNEL) == 1
    })
    .await?;
    notify_channel(fixture.app_pool(), CHANNEL, "one").await?;
    until("the first notification", || watched.notified("one")).await?;

    let before = watched.heard().len();
    let subscribed = proxy.subscriptions(CHANNEL);
    expect_eq("stalled live subscriptions", &proxy.stall(CHANNEL), &1)?;
    let superuser = fixture.superuser_pool()?;
    // Selected first: the planner may evaluate a termination in the same
    // WHERE clause before its filters, ending unrelated sessions.
    let listeners: Vec<i32> = sqlx::query_scalar(
        "SELECT pid FROM pg_stat_activity WHERE datname = current_database() AND query = $1",
    )
    .bind(format!("LISTEN \"{CHANNEL}\""))
    .fetch_all(&superuser)
    .await?;
    expect_eq("listener backends", &listeners.len(), &1)?;
    sqlx::query("SELECT pg_terminate_backend(pid) FROM unnest($1::int[]) AS pid")
        .bind(&listeners)
        .execute(&superuser)
        .await?;
    until("a pending replacement subscription", || {
        proxy.subscriptions(CHANNEL) > subscribed && proxy.stalled_open(CHANNEL) >= 1
    })
    .await?;
    let pending = watched.resets_since(before);
    if pending.first() != Some(&false) || pending.contains(&true) {
        return Err(format!("the cache while its replacement is pending: {pending:?}").into());
    }

    proxy.restore(CHANNEL);
    until("a recovered subscription", || watched.live()).await?;
    let recovered = watched.resets_since(before);
    if recovered.first() != Some(&false) || recovered.iter().filter(|live| **live).count() != 1 {
        return Err(format!("recovery after loss: {recovered:?}").into());
    }
    notify_channel(fixture.app_pool(), CHANNEL, "two").await?;
    until("the recovered notification", || watched.notified("two")).await?;
    let stopped = watched.stop().await?;
    tracing::info!(?stopped, "loss-ordering listener stopped");
    Ok(())
}

/// Stalled listeners release their connections while still stalled, every
/// pending stage ends within the shutdown drain, and one stalled listener
/// never stops another (REQ-013, AC-011).
///
/// Two shared listeners on different channels share one proxied pool, with
/// the heartbeat shortened. Three times the first channel stalls: its
/// listener switches its cache off, closes its stalled connection while the
/// stall persists, and resubscribes once restored, while the second keeps
/// delivering notifications and the pool grows no retained connections.
/// Listeners are then cancelled while receiving on a stalled path, while a
/// liveness probe is unanswered, while a subscription is unanswered, and
/// while waiting to retry; each finishes within the production drain with
/// its cache off and no stalled connection left open.
///
/// # Errors
/// Returns fixture, proxy, or statement errors, or a description of the
/// first retained connection, late cancellation, or coupled failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn listener_shutdown_releases_stalled_connections() -> Result<(), ServerJourneyError> {
    const STALLED: &str = "wyrd_listener_stalled";
    const HEALTHY: &str = "wyrd_listener_healthy";
    let (fixture, proxy, postgres) = proxied_postgres().await?;
    let quick = |channel: &str, timeout: Duration| {
        postgres
            .listener(channel)
            .with_heartbeat_for_test(Duration::from_secs(1), timeout)
    };
    let mut stalled = Watched::start(quick(STALLED, Duration::from_secs(1)));
    let healthy = Watched::start(quick(HEALTHY, Duration::from_secs(1)));
    until("both listeners live", || stalled.live() && healthy.live()).await?;
    let baseline = postgres.app_pool().size();

    for round in 0..3 {
        if proxy.stall(STALLED) == 0 {
            return Err("no live subscription was stalled".into());
        }
        until("stall detection", || !stalled.live()).await?;
        until("the stalled connection's release", || {
            proxy.stalled_open(STALLED) == 0
        })
        .await?;
        let payload = format!("round-{round}");
        notify_channel(fixture.app_pool(), HEALTHY, &payload).await?;
        until("the healthy listener's notification", || {
            healthy.notified(&payload)
        })
        .await?;
        proxy.restore(STALLED);
        until("resubscription", || {
            stalled.live() && proxy.listening(STALLED) == 1
        })
        .await?;
    }
    let size = postgres.app_pool().size();
    if size > baseline {
        return Err(format!("the pool grew from {baseline} to {size} connections").into());
    }

    proxy.stall(STALLED);
    let receive = stalled.stop().await?;
    until("release after receive", || proxy.stalled_open(STALLED) == 0).await?;

    proxy.restore(STALLED);
    stalled = Watched::start(quick(STALLED, wyrd_sql::listener::PROBE_TIMEOUT));
    until("a live listener", || {
        stalled.live() && proxy.listening(STALLED) == 1
    })
    .await?;
    let discarded = proxy.discarded(STALLED);
    proxy.stall(STALLED);
    until("an unanswered probe", || {
        proxy.discarded(STALLED) > discarded
    })
    .await?;
    let probe = stalled.stop().await?;
    until("release after a probe", || proxy.stalled_open(STALLED) == 0).await?;

    let attempts = proxy.subscriptions(STALLED);
    stalled = Watched::start(quick(STALLED, wyrd_sql::listener::PROBE_TIMEOUT));
    until("an unanswered subscription", || {
        proxy.subscriptions(STALLED) > attempts && proxy.stalled_open(STALLED) == 1
    })
    .await?;
    let subscription = stalled.stop().await?;
    until("release after a subscription", || {
        proxy.stalled_open(STALLED) == 0
    })
    .await?;

    let attempts = proxy.subscriptions(STALLED);
    stalled = Watched::start(quick(STALLED, Duration::from_millis(200)));
    until("a released attempt awaiting retry", || {
        proxy.subscriptions(STALLED) > attempts && proxy.stalled_open(STALLED) == 0
    })
    .await?;
    let retry = stalled.stop().await?;
    tracing::info!(
        ?receive,
        ?probe,
        ?subscription,
        ?retry,
        "stalled listeners cancelled"
    );

    notify_channel(fixture.app_pool(), HEALTHY, "after").await?;
    until("the healthy listener after every stall", || {
        healthy.notified("after")
    })
    .await?;
    healthy.stop().await?;
    until("no stalled connection left open", || {
        proxy.stalled_open(STALLED) == 0
    })
    .await?;
    Ok(())
}

/// Shared shutdown settles accepted producer work, reaps its workers, and
/// commits acknowledgements that arrive while it drains (AC-003/011/012).
///
/// A bound server with the queued runtime holds one gateway call at its slow
/// upstream and accepts one public enqueue whose Scribe acknowledgement is
/// still outstanding, then shuts down through the production supervisor.
/// Drain cancels the held call, which answers its caller as unavailable and
/// is still settled: a server restarted over the same Postgres, storage, and
/// Bifrost data root reads exactly one cancelled capture and the accepted
/// execution. A second
/// server whose drain budget is already spent, shut down with a call still
/// held, reports the expired drain instead of a clean one.
///
/// # Errors
/// Returns server, seeding, request, or query errors, or a description of
/// the first lost, duplicated, or falsely settled item.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn shared_shutdown_drains_producers_workers_and_acknowledgements()
-> Result<(), ServerJourneyError> {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(GatewayCapture::completion())
                .set_delay(Duration::from_secs(1)),
        )
        .mount(&upstream)
        .await;
    let root = url::Url::parse(&upstream.uri())?;
    let scratch = tempfile::tempdir()?;
    let builder = || {
        WyrdTestServer::builder()
            .with_gateway_provider_root_for_test(root.clone())
            .with_durable_bifrost_data_root(scratch.path().to_path_buf())
    };
    let server = Box::pin(builder().with_verification_runtime_for_test().start_bound()).await?;
    let journey = RefreshJourney {
        base: server.base_url().ok_or("the server is bound")?.to_owned(),
        server,
        upstream,
        http: reqwest::Client::new(),
    };
    let admin = journey
        .server
        .bootstrap_service("drain-admin", &["admin"])
        .await?;
    let admin_key = admin.api_key().ok_or("the admin carries an API key")?;
    let token = journey.server.exchange_api_key(admin_key).await?;
    journey.deploy(&token, "held").await?;
    let metadata = json!({"mode": "metadata", "payload_fields": []});
    let policy = journey
        .http
        .put(format!("{}/v1/admin/gateway/capture-policy", journey.base))
        .header("x-wyrd-access-token", format!("Bearer {token}"))
        .json(&metadata)
        .send()
        .await?;
    if !policy.status().is_success() {
        return Err(format!("the capture policy was refused: {}", policy.status()).into());
    }
    journey
        .server
        .seed_role(
            "drain_runner",
            &[Permission::card_read(), Permission::verifier_run_any()],
        )
        .await?;
    // A service is bound to its own Card scope; a user is decided by RBAC.
    let Bootstrap::User { jwt: enqueuer, .. } = journey
        .server
        .bootstrap_user("drain-enqueuer", &["drain_runner"])
        .await?
    else {
        return Err("the enqueuer bootstraps a user".into());
    };
    let seed = VerificationFixture::provision(
        journey.server.state().postgres.wyrd(),
        journey.server.data_tenant_id(),
    )
    .await?;
    let (subject, _) = seed.service("drain-subject").await?;
    let verifier = assert_verifier(&seed, "drain-assert").await?;

    let (http, base, caller) = (journey.http.clone(), journey.base.clone(), token.clone());
    let held = tokio::spawn(async move {
        http.post(format!("{base}/v1/chat/completions"))
            .bearer_auth(caller)
            .json(&json!({
                "model": "acme/m",
                "max_completion_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await
    });
    let deadline = tokio::time::Instant::now() + WAIT;
    while journey.upstream_paths().await?.is_empty() {
        if tokio::time::Instant::now() >= deadline {
            return Err("the held call never dispatched".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let accepted: Value = journey
        .http
        .post(format!("{}/v1/verification/enqueue", journey.base))
        .header("x-wyrd-access-token", format!("Bearer {enqueuer}"))
        .json(&enqueue_body(
            &verifier,
            &subject,
            json!({ "x": { "kind": "json", "value": true } }),
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let execution = accepted["execution_id"]
        .as_str()
        .ok_or("the enqueue answers an execution ID")?
        .parse::<uuid::Uuid>()?;

    let RefreshJourney {
        mut server,
        upstream,
        ..
    } = journey;
    server.cancel_and_join_for_test().await?;
    let answered = held.await??;
    if answered.status() != reqwest::StatusCode::SERVICE_UNAVAILABLE {
        return Err(format!("the drained call answered {}", answered.status()).into());
    }

    let server = Box::pin(server.restart_bound(builder())).await?;
    let superuser = server.pg_fixture().superuser_pool()?;
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM wyrd.verifier_executions WHERE execution_id = $1")
            .bind(execution)
            .fetch_one(&superuser)
            .await?;
    server.flush_bifrost().await?;
    let captured = query_rows(
        &server,
        server.data_tenant_id(),
        "SELECT call_id FROM bifrost.gateway.calls WHERE outcome = 'cancelled'".to_owned(),
    )
    .await?;
    server.shutdown().await?;
    if (queued, captured) != (1, 1) {
        return Err(format!(
            "expected the accepted execution and one cancelled capture, read {queued} and {captured}"
        )
        .into());
    }

    let mut expired = Box::pin(
        WyrdTestServer::builder()
            .with_gateway_provider_root_for_test(root)
            .with_shutdown_drain_for_test(Duration::ZERO)
            .start_bound(),
    )
    .await?;
    let journey = RefreshJourney {
        base: expired.base_url().ok_or("the server is bound")?.to_owned(),
        server: expired,
        upstream,
        http: reqwest::Client::new(),
    };
    let token = journey.admin(None, "expired-admin").await?;
    journey.deploy(&token, "held").await?;
    let dispatched = journey.upstream_paths().await?.len();
    let (http, base) = (journey.http.clone(), journey.base.clone());
    let held = tokio::spawn(async move {
        http.post(format!("{base}/v1/chat/completions"))
            .bearer_auth(token)
            .json(&json!({
                "model": "acme/m",
                "max_completion_tokens": 16,
                "messages": [{"role": "user", "content": "hi"}],
            }))
            .send()
            .await
    });
    let deadline = tokio::time::Instant::now() + WAIT;
    while journey.upstream_paths().await?.len() == dispatched {
        if tokio::time::Instant::now() >= deadline {
            return Err("the expired server's held call never dispatched".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    expired = journey.server;
    let drained = expired.cancel_and_join_for_test().await;
    let _ = held.await;
    expired.shutdown().await?;
    if drained.is_ok() {
        return Err("an expired drain with a held call reported a clean shutdown".into());
    }
    Ok(())
}

/// A client of `server` authenticating with `credential`, an API key or an
/// access token.
///
/// # Errors
/// Returns an error when the client cannot be assembled.
fn native_client(
    server: &WyrdTestServer,
    credential: &str,
) -> Result<wyrd_client::WyrdClient, ServerJourneyError> {
    Ok(wyrd_client::bifrost::client_from_options(
        server.base_url(),
        Some(credential),
        server.grpc_url().as_deref(),
    )?)
}

/// Send one row of the `(id, value)` table `fqn` over the native ingest wire
/// as `client`, correlated to `card` when given, and return the server's
/// answer.
///
/// # Errors
/// Returns the server's stable refusal, or a harness error when the ingest
/// channel cannot be dialled.
///
/// # Panics
/// Panics when the row does not prepare against `preflight`.
async fn native_row(
    client: &wyrd_client::WyrdClient,
    fqn: &str,
    preflight: &wyrd_queue::RowPreflight,
    card: Option<&CardUid>,
) -> Result<(), WyrdError> {
    let rows = preflight
        .prepare(&[r#"{"id": 1, "value": "native"}"#], card, None)
        .expect("the row prepares");
    wyrd_testing::bifrost::write::RawIngest::connect(client)
        .await?
        .insert(
            fqn,
            uuid::Uuid::now_v7(),
            rows.to_ipc().expect("the row encodes"),
        )
        .await
        .map(|_request_id| ())
}

/// A native batch id names one accepted origin, not only one user payload.
///
/// One row of a dynamic table is written under an explicit batch id by a
/// Service account rooted at a registered Service, correlated to a Model and
/// a Run. An exact retry is acknowledged without a second row. Reusing that
/// batch id with the same user payload but another workload (the account
/// rootless), another account pair, no subject, or another Run is refused,
/// while each of those writes is accepted under a fresh batch id. After a
/// restart empties the warm duplicate cache, the durable fence alone gives
/// the same answers, and the original row keeps its original origin.
///
/// # Errors
/// Returns [`ServerJourneyError`] when the server, a fixture, a restart, or a
/// query fails.
///
/// # Panics
/// Panics when a retry or fresh write is refused, a reused batch id with a
/// changed origin is acknowledged, or the stored rows differ.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn native_batch_replay_is_bound_to_its_accepted_origin() -> Result<(), ServerJourneyError> {
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let key = async |name: &str, role: &str| -> Result<String, ServerJourneyError> {
        Ok(server
            .bootstrap_service(name, &[role])
            .await?
            .api_key()
            .ok_or("a Service has an API key")?
            .expose_secret()
            .to_owned())
    };
    let [admin_key, writer_key, other_key] = [
        key("native-replay-admin", "admin").await?,
        key("native-replay-writer", "editor").await?,
        key("native-replay-other", "editor").await?,
    ];
    let admin = native_client(&server, &admin_key)?;
    let cards = wyrd_client::cards::Cards::connect_with(&admin).await?;
    let fixtures = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/observe_a_run"
    ));
    let model = Box::pin(cards.declare_from_path(&fixtures.join("observed-model.yaml")))
        .await?
        .root
        .uid
        .ok_or("the Model has a UID")?;
    let service = Box::pin(cards.declare_from_path(&fixtures.join("observed-service.yaml")))
        .await?
        .root
        .uid
        .ok_or("the Service has a UID")?;
    let fqn = format!(
        "bifrost.datasets.native_replay_{}",
        uuid::Uuid::now_v7().simple()
    );
    Bifrost::connect_with(
        &admin,
        BifrostOptions {
            table: Some(TableConfig::from_arrow(
                &fqn,
                Arc::new(Schema::new(vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ])),
            )?),
            ..BifrostOptions::default()
        },
    )
    .await?
    .register()
    .await?;
    let (namespace, name) = fqn.rsplit_once('.').ok_or("the table is qualified")?;
    let preflight = wyrd_queue::RowPreflight::from_description(
        &Bifrost::query_only(&admin)
            .describe_table(namespace, name)
            .await?,
    )?;
    let [run, other_run] = ["replay-run", "other-run"]
        .map(|run| wyrd_spec::vala::ids::RunId::from_string(run.to_owned()));
    let payload = |subject: Option<&CardUid>, run: &wyrd_spec::vala::ids::RunId| {
        preflight
            .prepare(&[r#"{"id": 1, "value": "replayed"}"#], subject, Some(run))
            .expect("the row prepares")
            .to_ipc()
            .expect("the row encodes")
    };
    let insert = async |client: &wyrd_client::WyrdClient, batch: uuid::Uuid, ipc: Vec<u8>| {
        RawIngest::connect(client)
            .await?
            .insert(&fqn, batch, ipc)
            .await
    };
    let batch = uuid::Uuid::now_v7();
    let original = payload(Some(&model), &run);

    let mut server = server;
    for phase in ["warm", "cold"] {
        let writer = native_client(&server, &writer_key)?;
        let rooted = writer.for_workload(service.clone());
        insert(&rooted, batch, original.clone()).await?;
        insert(&rooted, batch, original.clone()).await?;
        let changed = [
            ("workload", writer.clone(), original.clone()),
            (
                "account pair",
                native_client(&server, &other_key)?.for_workload(service.clone()),
                original.clone(),
            ),
            ("subject", rooted.clone(), payload(None, &run)),
            ("run", rooted.clone(), payload(Some(&model), &other_run)),
        ];
        for (origin, client, ipc) in changed {
            insert(&client, batch, ipc.clone())
                .await
                .expect_err(&format!(
                    "{phase}: a batch id reused under a changed {origin} is refused"
                ));
            if phase == "warm" {
                insert(&client, uuid::Uuid::now_v7(), ipc).await?;
            }
        }
        server.flush_bifrost().await?;
        if phase == "warm" {
            server = Box::pin(server.restart_bound(WyrdTestServer::builder())).await?;
        }
    }

    let admin = native_client(&server, &admin_key)?;
    let stored: Vec<Value> = Bifrost::query_only(&admin)
        .sql_as(
            &format!(
                "SELECT CAST(workload_card_uid AS VARCHAR) AS workload_card_uid, \
                 CAST(subject_card_uid AS VARCHAR) AS subject_card_uid, run_id, \
                 COUNT(*) AS n FROM {fqn} \
                 GROUP BY workload_card_uid, subject_card_uid, run_id, sub_principal_id \
                 ORDER BY n DESC, run_id, workload_card_uid, subject_card_uid"
            ),
            &[],
        )
        .await?;
    assert_eq!(
        stored.len(),
        5,
        "the original and each fresh write are distinct row sets: {stored:?}"
    );
    assert!(
        stored.iter().all(|row| row["n"] == json!(1)),
        "every write is stored exactly once: {stored:?}"
    );
    assert!(
        stored.contains(&json!({
            "workload_card_uid": service.to_string(),
            "subject_card_uid": model.to_string(),
            "run_id": "replay-run",
            "n": 1,
        })),
        "the original row keeps its original origin: {stored:?}"
    );
    server.shutdown().await?;
    Ok(())
}

/// A normal User bearer with a loaded `WyrdState` writes native observations:
/// the state selects its Service root on every frame, the server narrows the
/// rootless User bearer to that validated root before Gate admission, and the
/// row lands correlated to the bundle's Model under the User's own identity.
///
/// Selection only ever narrows. An unknown root and a non-Service root are
/// refused as unusable, a bearer signed for one root that selects another is
/// refused as mismatched, a User without a write grant is refused, and a row
/// correlated to a Card outside the selected root's graph is refused, each
/// before any ACK. A rooted machine and a rootless User sending no selector
/// keep their native behavior.
///
/// # Errors
/// Returns server, fixture, bundle, client, state, flush, or query failures.
///
/// # Panics
/// Panics when an accepted write is refused, a refused write is accepted or
/// refused with another code, or the observation lands under another identity
/// or Card.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn user_state_native_io_selects_validated_root() -> Result<(), ServerJourneyError> {
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let admin_key = server
        .bootstrap_service("native-user-admin", &["admin"])
        .await?;
    let admin = native_client(
        &server,
        admin_key
            .api_key()
            .ok_or("the admin has an API key")?
            .expose_secret(),
    )?;
    let cards = wyrd_client::cards::Cards::connect_with(&admin).await?;
    let fixtures = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/observe_a_run"
    ));
    let model = Box::pin(cards.declare_from_path(&fixtures.join("observed-model.yaml")))
        .await?
        .root
        .uid
        .ok_or("the Model has a UID")?;
    let service = Box::pin(cards.declare_from_path(&fixtures.join("observed-service.yaml")))
        .await?
        .root;
    let service_uid = service.uid.clone().ok_or("the Service has a UID")?;
    let other = Box::pin(cards.declare_from_path(&fixtures.join("configured-service.yaml")))
        .await?
        .root
        .uid
        .ok_or("the other Service has a UID")?;
    let bundle = tempfile::tempdir()?;
    Box::pin(
        wyrd_client::cards::CardGraphPuller::new(cards.registry_context()).pull(
            &wyrd_client::cards::CardSelector::exact(service.clone()),
            &bundle.path().join("bundle"),
            wyrd_client::cards::BundleMode::Complete,
        ),
    )
    .await?;
    let user = |name: &'static str, role: &'static str| {
        let server = &server;
        async move {
            match server.bootstrap_user(name, &[role]).await? {
                Bootstrap::User { id, jwt } => Ok::<_, ServerJourneyError>((id, jwt)),
                Bootstrap::Machine { .. } => Err("a user bootstraps as a user".into()),
            }
        }
    };
    let (editor_id, editor_jwt) = user("native-user-editor", "editor").await?;
    let editor = native_client(&server, &editor_jwt)?;

    let state = wyrd_client::state::WyrdState::from_path_with(
        bundle.path().join("bundle"),
        editor.clone(),
    )?;
    state
        .start_bifrost_with_config(None, wyrd_client::QueueConfig::default())
        .await?;
    state
        .run()
        .for_card("model")?
        .observe()
        .submit("observed-drift", json!({ "latency": 12.5 }))
        .await?;
    state.shutdown().await?;
    server.flush_bifrost().await?;
    let landed: Vec<Value> = Bifrost::query_only(&admin)
        .sql_as(
            "SELECT CAST(subject_card_uid AS VARCHAR) AS subject_card_uid, \
             CAST(sub_principal_id AS VARCHAR) AS sub_principal_id \
             FROM bifrost.drift.observations",
            &[],
        )
        .await?;
    assert_eq!(
        landed,
        vec![
            json!({ "subject_card_uid": model.to_string(), "sub_principal_id": editor_id.to_string() })
        ],
        "the User's state observation lands on its Model under the User"
    );

    let fqn = format!(
        "bifrost.datasets.native_user_{}",
        uuid::Uuid::now_v7().simple()
    );
    Bifrost::connect_with(
        &admin,
        BifrostOptions {
            table: Some(TableConfig::from_arrow(
                &fqn,
                Arc::new(Schema::new(vec![
                    Field::new("id", DataType::Int64, false),
                    Field::new("value", DataType::Utf8, false),
                ])),
            )?),
            ..BifrostOptions::default()
        },
    )
    .await?
    .register()
    .await?;
    let (namespace, name) = fqn.rsplit_once('.').ok_or("the table is qualified")?;
    let preflight = wyrd_queue::RowPreflight::from_description(
        &Bifrost::query_only(&admin)
            .describe_table(namespace, name)
            .await?,
    )?;

    let (_, viewer_jwt) = user("native-user-viewer", "viewer").await?;
    let runtime = server
        .credential_registered_service(&service, &["workload"])
        .await?;
    let runtime_key = runtime.api_key().ok_or("the runtime has an API key")?;
    let signed = server
        .exchange_api_key_for_workload(runtime_key, &service_uid)
        .await?;
    let unknown = CardUid::from_uuid(uuid::Uuid::now_v7())?;
    let refusals = [
        (
            editor.for_workload(unknown.clone()),
            &unknown,
            "WYRD_AUTH_401_WORKLOAD_UID_REFUSED",
        ),
        (
            editor.for_workload(model.clone()),
            &model,
            "WYRD_AUTH_401_WORKLOAD_UID_REFUSED",
        ),
        (
            native_client(&server, &signed)?.for_workload(other.clone()),
            &other,
            "WYRD_AUTH_400_SIGNED_WORKLOAD_MISMATCH",
        ),
        (
            native_client(&server, &viewer_jwt)?.for_workload(service_uid.clone()),
            &service_uid,
            "WYRD_PERMISSION_403_DENIED_RBAC",
        ),
        (
            editor.for_workload(service_uid.clone()),
            &other,
            "WYRD_VALA_403_BIFROST_CARD_SCOPE",
        ),
    ];
    for (client, card, code) in refusals {
        let refused = native_row(&client, &fqn, &preflight, Some(card))
            .await
            .expect_err("the write is refused before any ACK");
        assert_eq!(refused.code(), code, "{card}: {refused:?}");
    }

    native_row(
        &editor.for_workload(service_uid.clone()),
        &fqn,
        &preflight,
        Some(&service_uid),
    )
    .await?;
    native_row(
        &native_client(&server, runtime_key.expose_secret())?.for_workload(service_uid.clone()),
        &fqn,
        &preflight,
        Some(&service_uid),
    )
    .await?;
    native_row(&editor, &fqn, &preflight, None).await?;
    server.shutdown().await?;
    Ok(())
}

/// `batch` with every row's `name` set to `value`, appending `name` as a
/// nullable Utf8 column when the batch does not declare it.
///
/// # Errors
/// Returns the Arrow error when the rebuilt batch does not assemble.
fn with_text(
    batch: &RecordBatch,
    name: &str,
    value: Option<&str>,
) -> Result<RecordBatch, ServerJourneyError> {
    let column: ArrayRef = Arc::new(StringArray::from(vec![value; batch.num_rows()]));
    let mut fields = batch.schema().fields().to_vec();
    let mut columns = batch.columns().to_vec();
    if let Ok(index) = batch.schema().index_of(name) {
        columns[index] = column;
    } else {
        fields.push(Arc::new(Field::new(name, DataType::Utf8, true)));
        columns.push(column);
    }
    Ok(RecordBatch::try_new(
        Arc::new(Schema::new(fields)),
        columns,
    )?)
}

/// Send `batch` to `bifrost.traces.spans` as one native frame under
/// `batch_id` as `client`, and return the request id its rows carry.
///
/// # Errors
/// Returns the server's stable refusal, or a harness error when the ingest
/// channel cannot be dialled.
///
/// # Panics
/// Panics when the batch does not encode as Arrow IPC.
async fn native_spans(
    client: &wyrd_client::WyrdClient,
    batch_id: uuid::Uuid,
    batch: &RecordBatch,
) -> Result<RequestId, WyrdError> {
    RawIngest::connect(client)
        .await?
        .insert(SPANS, batch_id, ipc(batch).expect("the frame encodes"))
        .await
}

/// The distinct shared dimensions and workload of the spans one request
/// wrote, as `admin` reads them back; a null column is omitted.
///
/// # Errors
/// Returns the query error.
async fn span_dimensions(
    admin: &wyrd_client::WyrdClient,
    request: &RequestId,
) -> Result<Vec<Value>, ServerJourneyError> {
    table_dimensions(admin, SPANS, request).await
}

/// The distinct shared dimensions and workload of the rows one request wrote
/// to `table`, as `admin` reads them back; a null column is omitted.
///
/// # Errors
/// Returns the query error.
async fn table_dimensions(
    admin: &wyrd_client::WyrdClient,
    table: &str,
    request: &RequestId,
) -> Result<Vec<Value>, ServerJourneyError> {
    Ok(Bifrost::query_only(admin)
        .sql_as(
            &format!(
                "SELECT DISTINCT service_name, service_version, deployment_environment, \
                 subject_kind, subject_name, subject_version, \
                 CAST(workload_card_uid AS VARCHAR) AS workload_card_uid \
                 FROM {table} \
                 WHERE CAST(wyrd_request_id AS VARCHAR) = '{}'",
                request.as_str()
            ),
            &[],
        )
        .await?)
}

/// The table a native drift writer records raw observations in.
const DRIFT_OBSERVATIONS: &str = "bifrost.drift.observations";

/// One valid drift observation row in `schema`, with every shared dimension
/// sent as the null placeholder the SDK Drift producer sends.
///
/// # Errors
/// Returns the Arrow error when the row does not build against `schema`.
fn drift_observation(schema: &SchemaRef) -> Result<RecordBatch, ServerJourneyError> {
    let columns = schema
        .fields()
        .iter()
        .map(|field| -> ArrayRef {
            match field.name().as_str() {
                "record_id" => Arc::new(StringArray::from(vec!["native-drift"])),
                "series" => Arc::new(StringArray::from(vec!["latency"])),
                "num_value" => Arc::new(arrow::array::Float64Array::from(vec![1.5])),
                "created_at" => Arc::new(
                    TimestampMicrosecondArray::from(vec![Utc::now().timestamp_micros()])
                        .with_timezone("+00:00"),
                ),
                _ => arrow::array::new_null_array(field.data_type(), 1),
            }
        })
        .collect();
    Ok(RecordBatch::try_new(Arc::clone(schema), columns)?)
}

/// Public native drift rows take their shared dimensions only from the
/// accepted request context.
///
/// A drift observation has no source field a supplied Service, environment,
/// or subject value could be validated against, so supplying any of the six
/// shared dimensions refuses the whole frame before ACK, rootless and under
/// a selected Service root and environment alike, and stores nothing. The
/// same row with null placeholders lands with the accepted Service, the
/// accepted environment, and its subject's descriptor. A User delegating to
/// that Service's runtime account, which runs under the Service's root, lands
/// owned by that root rather than unowned.
///
/// # Errors
/// Returns [`ServerJourneyError`] when the server, a fixture, or a query
/// fails.
///
/// # Panics
/// Panics when a frame supplying a dimension is acked or refused for another
/// reason, or a stored row's dimensions or count differ.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn native_drift_dimensions_come_only_from_accepted_context() -> Result<(), ServerJourneyError>
{
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    server
        .ensure_builtin_table_for_test(server.data_tenant_id(), "drift", "observations")
        .await?;
    let admin_key = server
        .bootstrap_service("native-drift-admin", &["admin"])
        .await?;
    let admin = native_client(
        &server,
        admin_key
            .api_key()
            .ok_or("the admin has an API key")?
            .expose_secret(),
    )?;
    let cards = wyrd_client::cards::Cards::connect_with(&admin).await?;
    let fixtures = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/observe_a_run"
    ));
    let model = Box::pin(cards.declare_from_path(&fixtures.join("observed-model.yaml")))
        .await?
        .root
        .uid
        .ok_or("the Model has a UID")?;
    let service = Box::pin(cards.declare_from_path(&fixtures.join("observed-service.yaml")))
        .await?
        .root;
    let service_uid = service.uid.clone().ok_or("the Service has a UID")?;
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("native-drift-editor", &["editor"])
        .await?
    else {
        return Err("a user bootstraps as a user".into());
    };
    let editor = native_client(&server, &jwt)?;
    let owned = editor.for_service(service_uid.clone(), Some("staging".to_owned()));
    let schema = Arc::clone(
        TableConfig::describe_with(&admin, DRIFT_OBSERVATIONS)
            .await?
            .user_schema(),
    );
    let row = with_text(
        &drift_observation(&schema)?,
        SUBJECT_CARD_UID,
        Some(model.as_str()),
    )?;
    let insert = async |client: &wyrd_client::WyrdClient, batch: &RecordBatch| {
        RawIngest::connect(client)
            .await?
            .insert(
                DRIFT_OBSERVATIONS,
                uuid::Uuid::now_v7(),
                ipc(batch).expect("the frame encodes"),
            )
            .await
    };

    for column in vala_bifrost_redux::tables::SHARED_DIMENSIONS {
        let forged = with_text(&row, column, Some("forged"))?;
        for (context, client) in [("rootless", &editor), ("rooted", &owned)] {
            let refused = insert(client, &forged)
                .await
                .expect_err("a supplied shared dimension is refused before any ACK");
            assert!(
                matches!(
                    refused,
                    WyrdError::Vala {
                        error: BifrostError::SchemaParse { .. }
                    }
                ),
                "{context} {column}: {refused:?}"
            );
        }
    }

    let actor = server
        .credential_registered_service(&service, &["editor"])
        .await?;
    let actor = server
        .exchange_api_key_for_workload(
            actor
                .api_key()
                .ok_or("the runtime account has an API key")?,
            &service_uid,
        )
        .await?;
    let delegated = native_client(
        &server,
        &server
            .delegate(&jwt, &actor, wyrd_spec::auth::TokenAudience::Wyrd)
            .await?,
    )?;
    let landed = [
        insert(&owned, &row).await?,
        insert(&editor, &row).await?,
        insert(&delegated, &row).await?,
    ];
    server.flush_bifrost().await?;

    let subject = json!({
        "subject_kind": "Model",
        "subject_name": "observed-model",
        "subject_version": "1.0.0",
    });
    let with_service = |environment: Option<&str>| {
        let mut expected = subject.clone();
        expected["service_name"] = json!("observed-service");
        expected["service_version"] = json!("1.0.0");
        expected["workload_card_uid"] = json!(service_uid.to_string());
        if let Some(environment) = environment {
            expected["deployment_environment"] = json!(environment);
        }
        expected
    };
    let expected = [
        with_service(Some("staging")),
        subject.clone(),
        with_service(None),
    ];
    for (request, expected) in landed.iter().zip(expected) {
        assert_eq!(
            table_dimensions(&admin, DRIFT_OBSERVATIONS, request).await?,
            vec![expected],
            "request {request}"
        );
    }
    let stored: Vec<Value> = Bifrost::query_only(&admin)
        .sql_as(
            &format!("SELECT COUNT(*) AS n FROM {DRIFT_OBSERVATIONS}"),
            &[],
        )
        .await?;
    assert_eq!(
        stored,
        vec![json!({ "n": 3 })],
        "no refused frame stores a row"
    );
    server.shutdown().await?;
    Ok(())
}

/// Native canonical span frames carry only what their source fields derive,
/// and their shared dimensions come from the accepted request context.
///
/// A frame whose promoted Service, environment, or subject label is forged is
/// refused whole before any ACK. A frame run as a Service in an accepted
/// environment lands with that Service's canonical name and version and that
/// environment over the rows' labels, and with its Model subject's kind,
/// name, and version. Without a request environment the typed resource
/// environment survives, a rootless frame keeps its observed Service labels,
/// and an empty environment header refuses the request. A batch id replayed
/// under a changed owner is refused and its rows keep the original owner,
/// while an exact replay is answered as a duplicate.
///
/// # Errors
/// Returns server, fixture, client, ingest, flush, or query failures.
///
/// # Panics
/// Panics when a forged or context-refused frame is acked, a valid frame is
/// refused, or a row lands with other dimensions.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn native_signal_values_and_attribution_are_validated_before_ack()
-> Result<(), ServerJourneyError> {
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    server
        .ensure_builtin_table_for_test(server.data_tenant_id(), "traces", "spans")
        .await?;
    let admin_key = server
        .bootstrap_service("native-signal-admin", &["admin"])
        .await?;
    let admin = native_client(
        &server,
        admin_key
            .api_key()
            .ok_or("the admin has an API key")?
            .expose_secret(),
    )?;
    let cards = wyrd_client::cards::Cards::connect_with(&admin).await?;
    let fixtures = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../fixtures/cards/observe_a_run"
    ));
    let model = Box::pin(cards.declare_from_path(&fixtures.join("observed-model.yaml")))
        .await?
        .root
        .uid
        .ok_or("the Model has a UID")?;
    let service_uid = Box::pin(cards.declare_from_path(&fixtures.join("observed-service.yaml")))
        .await?
        .root
        .uid
        .ok_or("the Service has a UID")?;
    let Bootstrap::User { jwt, .. } = server
        .bootstrap_user("native-signal-editor", &["editor"])
        .await?
    else {
        return Err("a user bootstraps as a user".into());
    };
    let editor = native_client(&server, &jwt)?;
    let owned = editor.for_service(service_uid.clone(), Some("staging".to_owned()));
    let schema = Arc::clone(
        TableConfig::describe_with(&admin, SPANS)
            .await?
            .user_schema(),
    );
    let spans = canonical_signals::spans(
        &schema,
        "native-signal",
        Utc::now().timestamp_nanos_opt().unwrap_or(0),
    );
    let attributed = with_text(&spans, SUBJECT_CARD_UID, Some(model.as_str()))?;

    for column in ["service_name", "deployment_environment", "subject_name"] {
        let forged = with_text(&attributed, column, Some("forged"))?;
        let refused = native_spans(&owned, uuid::Uuid::now_v7(), &forged)
            .await
            .expect_err("a forged frame is refused before any ACK");
        assert!(
            matches!(
                refused,
                WyrdError::Vala {
                    error: BifrostError::SchemaParse { .. }
                }
            ),
            "forged {column}: {refused:?}"
        );
    }
    let empty = native_spans(
        &editor.for_service(service_uid.clone(), Some(String::new())),
        uuid::Uuid::now_v7(),
        &spans,
    )
    .await
    .expect_err("an empty environment refuses the request");
    assert!(
        matches!(empty, WyrdError::Validation { .. }),
        "empty environment: {empty:?}"
    );

    let replayed = uuid::Uuid::now_v7();
    let landed = [
        native_spans(&owned, uuid::Uuid::now_v7(), &attributed).await?,
        native_spans(
            &editor.for_workload(service_uid.clone()),
            uuid::Uuid::now_v7(),
            &spans,
        )
        .await?,
        native_spans(&editor, replayed, &spans).await?,
    ];
    native_spans(&editor, replayed, &spans).await?;
    native_spans(&owned, replayed, &spans)
        .await
        .expect_err("a batch id replayed under a changed owner is refused");
    server.flush_bifrost().await?;

    let service = service_uid.to_string();
    let expected = [
        json!({
            "service_name": "observed-service",
            "service_version": "1.0.0",
            "deployment_environment": "staging",
            "subject_kind": "Model",
            "subject_name": "observed-model",
            "subject_version": "1.0.0",
            "workload_card_uid": service,
        }),
        json!({
            "service_name": "observed-service",
            "service_version": "1.0.0",
            "deployment_environment": canonical_signals::SPAN_ENVIRONMENT,
            "workload_card_uid": service,
        }),
        json!({
            "service_name": "wyrd.fixture.service",
            "deployment_environment": canonical_signals::SPAN_ENVIRONMENT,
        }),
    ];
    for (request, expected) in landed.iter().zip(expected) {
        assert_eq!(
            span_dimensions(&admin, request).await?,
            vec![expected],
            "request {request}"
        );
    }
    server.shutdown().await?;
    Ok(())
}

/// One span a trace journey writes: its span ID, name, and start time in
/// Unix nanoseconds.
type TraceSpan = (u64, &'static str, i64);

/// `spans` of `trace` as one batch of the described span `schema`.
///
/// # Panics
/// Panics when the described schema does not assemble the span rows, as
/// [`canonical_signals::trace_spans`] does.
fn trace_batch(schema: &SchemaRef, trace: TraceId, spans: &[TraceSpan]) -> RecordBatch {
    canonical_signals::trace_spans(schema, trace.as_bytes(), spans)
}

/// A fresh random trace ID.
///
/// # Errors
/// Never fails for a v7 UUID's bytes.
fn fresh_trace() -> Result<TraceId, ServerJourneyError> {
    Ok(TraceId::from_bytes(*uuid::Uuid::now_v7().as_bytes())?)
}

/// Now in Unix nanoseconds, shifted by `days`.
fn nanos_from_now(days: i64) -> i64 {
    (Utc::now() + chrono::Duration::days(days))
        .timestamp_nanos_opt()
        .unwrap_or_default()
}

/// A trace Verifier whose `requirements` judge the `trace` slot.
fn trace_spec(requirements: &[(&str, &str, Value)]) -> Value {
    let requirements: Vec<Value> = requirements
        .iter()
        .map(|(id, selector, expected)| {
            json!({ "id": id, "statement": format!("the trace holds {id}"),
                "check": { "kind": "trace", "on": "inputs.trace", "span_selector": selector,
                    "operator": "equals", "expected": expected } })
        })
        .collect();
    json!({ "inputs": { "trace": { "type": "trace" } }, "requirements": requirements })
}

/// The `POST /v1/verification/{route}` body judging `subject` with
/// `verifier` over `trace`.
fn trace_body(verifier: &CardUid, subject: &CardUid, trace: TraceId) -> Value {
    enqueue_body(
        verifier,
        subject,
        json!({ "trace": { "kind": "trace", "trace_id": trace.to_hex() } }),
    )
}

/// Execute `verifier` over `trace` for `subject` now, as `client`.
///
/// # Errors
/// Returns the route's refusal.
async fn execute_trace(
    client: &wyrd_client::WyrdClient,
    verifier: &CardUid,
    subject: &CardUid,
    trace: TraceId,
) -> Result<Value, ServerJourneyError> {
    Ok(client
        .request_json(
            reqwest::Method::POST,
            "/v1/verification/execute",
            Some(&trace_body(verifier, subject, trace)),
        )
        .await?)
}

/// Each requirement's `(id, outcome)` of an execute response.
fn requirement_outcomes(result: &Value) -> Vec<(String, String)> {
    result["outcome"]["requirements"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|requirement| {
            (
                requirement["id"].as_str().unwrap_or_default().to_owned(),
                requirement["outcome"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

/// The `(subject_card_uid, verdict)` of every published result of
/// `execution`.
///
/// # Errors
/// Returns the publication or query error.
async fn published_results(
    server: &WyrdTestServer,
    execution: &str,
) -> Result<Vec<Vec<Option<String>>>, ServerJourneyError> {
    server.flush_bifrost().await?;
    query_texts(
        server,
        server.data_tenant_id(),
        format!(
            "SELECT {SUBJECT_CARD_UID}, verdict FROM bifrost.verification.results \
             WHERE CAST(execution_id AS VARCHAR) = '{execution}'"
        ),
    )
    .await
}

/// An admin client of `server`'s fixture tenant, and the span schema it
/// writes.
///
/// # Errors
/// Returns bootstrap, client, or describe failures.
async fn span_writer(
    server: &WyrdTestServer,
) -> Result<(wyrd_client::WyrdClient, SchemaRef), ServerJourneyError> {
    let admin = server
        .bootstrap_service(
            &format!("trace-admin-{}", uuid::Uuid::now_v7().simple()),
            &["admin"],
        )
        .await?;
    let admin = native_client(
        server,
        admin
            .api_key()
            .ok_or("the admin has an API key")?
            .expose_secret(),
    )?;
    let schema = Arc::clone(
        TableConfig::describe_with(&admin, SPANS)
            .await?
            .user_schema(),
    );
    Ok((admin, schema))
}

/// Verification reads every retained span of the exact trace it names, and
/// the result stays its own subject's (REQ-043/045, AC-017/019).
///
/// One trace's spans are written through native ingest by a checkout-rooted
/// writer 30 days ago, a fraud-rooted writer now, and an untagged writer two
/// days ahead, outside the former seven-days-before/one-day-after window;
/// another trace and another tenant's spans of the same trace ID are written
/// beside them. Checkout's direct verification judges all three spans, in
/// start order with their own span IDs, and none of the others; its
/// published result names checkout. A caller without query access gets an
/// inconclusive requirement; a 10,000-span trace is judged in full and a
/// 10,001-span trace is refused explicitly.
///
/// # Errors
/// Returns server, fixture, ingest, route, or query failures.
///
/// # Panics
/// Panics when a judgment, ownership, or refusal differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn visible_trace_spans_preserve_binding_ownership() -> Result<(), ServerJourneyError> {
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    server
        .ensure_builtin_table_for_test(tenant, "traces", "spans")
        .await?;
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (checkout, _) = seed.service("checkout").await?;
    let (fraud, _) = seed.service("fraud").await?;
    let (admin, schema) = span_writer(&server).await?;
    let trace = fresh_trace()?;
    let spans: [(_, TraceSpan); 3] = [
        (
            admin.for_workload(checkout.clone()),
            (1, "retrieve", nanos_from_now(-30)),
        ),
        (
            admin.for_workload(fraud.clone()),
            (2, "score", nanos_from_now(0)),
        ),
        (admin.clone(), (3, "rank", nanos_from_now(2))),
    ];
    for (writer, span) in &spans {
        native_spans(
            writer,
            uuid::Uuid::now_v7(),
            &trace_batch(&schema, trace, &[*span]),
        )
        .await?;
    }
    native_spans(
        &admin,
        uuid::Uuid::now_v7(),
        &trace_batch(
            &schema,
            fresh_trace()?,
            &[(4, "unrelated", nanos_from_now(0))],
        ),
    )
    .await?;
    let other = server.seed_tenant("trace-other").await?;
    let intruder = server
        .bootstrap_service_in_tenant(other, "trace-intruder", &["admin"])
        .await?;
    native_spans(
        &native_client(
            &server,
            intruder
                .api_key()
                .ok_or("the intruder has an API key")?
                .expose_secret(),
        )?,
        uuid::Uuid::now_v7(),
        &trace_batch(&schema, trace, &[(5, "foreign", nanos_from_now(0))]),
    )
    .await?;
    server.flush_bifrost().await?;
    let workloads: Vec<Value> = Bifrost::query_only(&admin)
        .sql_as(
            &format!(
                "SELECT CAST({WORKLOAD_CARD_UID} AS VARCHAR) AS workload FROM {SPANS} \
                 WHERE trace_id = X'{}' ORDER BY start_time_unix_nano",
                trace.to_hex()
            ),
            &[],
        )
        .await?;
    assert_eq!(
        workloads,
        [
            json!({ "workload": checkout.to_string() }),
            json!({ "workload": fraud.to_string() }),
            json!({})
        ],
        "the trace spans two workloads and an untagged writer"
    );

    let verifier = seed
        .verifier(
            "whole-trace",
            &trace_spec(&[
                (
                    "names",
                    "$.spans[*].name",
                    json!(["retrieve", "score", "rank"]),
                ),
                (
                    "identities",
                    "$.spans[*].span_id",
                    json!(
                        spans
                            .iter()
                            .map(|(_, (id, ..))| format!("{id:016x}"))
                            .collect::<Vec<_>>()
                    ),
                ),
            ]),
        )
        .await?;
    let judged = execute_trace(&admin, &verifier, &checkout, trace).await?;
    assert_eq!(judged["outcome"]["verdict"], "passed", "{judged}");
    assert_eq!(
        judged["subject"]["uid"],
        json!(checkout.to_string()),
        "{judged}"
    );
    let execution = judged["execution_id"].as_str().ok_or("an execution ID")?;
    assert_eq!(
        published_results(&server, execution).await?,
        [vec![Some(checkout.to_string()), Some("passed".to_owned())]],
        "the published result is checkout's own"
    );

    server
        .seed_role(
            "trace_unqueried",
            &[Permission::card_read(), Permission::verifier_run_any()],
        )
        .await?;
    let unqueried = server
        .bootstrap_user("trace-unqueried", &["trace_unqueried"])
        .await?;
    let unqueried = native_client(&server, unqueried.jwt().ok_or("a user has a token")?)?;
    let denied = execute_trace(&unqueried, &verifier, &checkout, trace).await?;
    assert_eq!(
        requirement_outcomes(&denied),
        [
            ("names".to_owned(), "inconclusive".to_owned()),
            ("identities".to_owned(), "inconclusive".to_owned())
        ],
        "{denied}"
    );
    assert!(
        denied["outcome"]["requirements"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|requirement| requirement["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("lacks BifrostQuery/Read"))),
        "the ordinary query refusal decides: {denied}"
    );

    let ceiling = seed
        .verifier(
            "trace-ceiling",
            &trace_spec(&[("last", "$.spans[9999].name", json!("last"))]),
        )
        .await?;
    for (count, verdict) in [(10_000_u64, "passed"), (10_001, "inconclusive")] {
        let large = fresh_trace()?;
        let start = nanos_from_now(0);
        let spans: Vec<TraceSpan> = (1..=count)
            .map(|id| {
                let name = if id == 10_000 { "last" } else { "bulk" };
                (id, name, start + i64::try_from(id).unwrap_or_default())
            })
            .collect();
        native_spans(
            &admin,
            uuid::Uuid::now_v7(),
            &trace_batch(&schema, large, &spans),
        )
        .await?;
        server.flush_bifrost().await?;
        let judged = execute_trace(&admin, &ceiling, &checkout, large).await?;
        assert_eq!(
            judged["outcome"]["verdict"], verdict,
            "{count} spans: {judged}"
        );
        if count > 10_000 {
            assert!(
                judged.to_string().contains("exceeds 10000 spans"),
                "{count} spans: {judged}"
            );
        }
    }
    server.shutdown().await?;
    Ok(())
}

/// A completed verification keeps its result when spans land later; only a
/// new verification judges them (REQ-045, AC-019).
///
/// A queued checkout verification of a trace holding only `retrieve` is
/// judged on its first attempt and fails, since nonempty evidence never
/// waits. `rank` then lands. The execution stays completed after one
/// attempt with its one failed result, and a new direct verification of the
/// same trace passes.
///
/// # Errors
/// Returns server, fixture, ingest, route, runtime, or query failures.
///
/// # Panics
/// Panics when the first judgment waits or changes, or the new one differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn late_trace_spans_require_new_verification() -> Result<(), ServerJourneyError> {
    let server = Box::pin(WyrdTestServer::start_bound()).await?;
    let tenant = server.data_tenant_id();
    server
        .ensure_builtin_table_for_test(tenant, "traces", "spans")
        .await?;
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (checkout, _) = seed.service("checkout").await?;
    let (admin, schema) = span_writer(&server).await?;
    let verifier = seed
        .verifier(
            "late-trace",
            &trace_spec(&[("names", "$.spans[*].name", json!(["retrieve", "rank"]))]),
        )
        .await?;
    let trace = fresh_trace()?;
    let start = nanos_from_now(0);
    native_spans(
        &admin,
        uuid::Uuid::now_v7(),
        &trace_batch(&schema, trace, &[(1, "retrieve", start)]),
    )
    .await?;
    server.flush_bifrost().await?;

    let accepted: Value = admin
        .request_json(
            reqwest::Method::POST,
            "/v1/verification/enqueue",
            Some(&trace_body(&verifier, &checkout, trace)),
        )
        .await?;
    let execution = await_queued(&seed, &accepted).await?;
    let runtime = VerificationRuntime::builder(server.state())
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + WAIT;
    let first = loop {
        let row = seed.execution(execution).await?;
        if !["pending", "running"].contains(&row.status.as_str()) {
            break row;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("the trace execution never settled: {row:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(
        (first.status.as_str(), first.attempts),
        ("completed", 1),
        "{first:?}"
    );
    let id = execution.to_string();
    let failed = vec![vec![Some(checkout.to_string()), Some("failed".to_owned())]];
    assert_eq!(published_results(&server, &id).await?, failed);

    native_spans(
        &admin,
        uuid::Uuid::now_v7(),
        &trace_batch(&schema, trace, &[(2, "rank", start + 1)]),
    )
    .await?;
    server.flush_bifrost().await?;
    let again = execute_trace(&admin, &verifier, &checkout, trace).await?;
    assert_eq!(again["outcome"]["verdict"], "passed", "{again}");
    assert_ne!(again["execution_id"], json!(id), "a new execution judged");
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    assert_eq!(
        seed.execution(execution).await?,
        first,
        "the completed execution is final"
    );
    assert_eq!(published_results(&server, &id).await?, failed);
    server.shutdown().await?;
    Ok(())
}

/// One attributable table the exploration journey writes and filters: its
/// FQN and its time column, rendered as a lower-bound predicate.
struct Explored {
    /// The table's FQN.
    table: &'static str,
    /// The `>=` predicate on the table's time column that every written row
    /// satisfies, so a bounded filter must return the unbounded population.
    since: fn(DateTime<Utc>) -> String,
}

/// `at` as a microsecond UTC `TIMESTAMP` literal.
fn timestamp(at: DateTime<Utc>) -> String {
    format!(
        "TIMESTAMP '{}'",
        at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
    )
}

/// The eight attributable built-ins with the time column each partitions on.
const EXPLORED: [Explored; 8] = [
    Explored {
        table: SPANS,
        since: |at| format!("start_time_unix_nano >= {}", timestamp(at)),
    },
    Explored {
        table: RECORDS,
        since: |at| format!("time_unix_nano >= {}", timestamp(at)),
    },
    Explored {
        table: POINTS,
        since: |at| format!("time_unix_nano >= {}", timestamp(at)),
    },
    Explored {
        table: "bifrost.gateway.calls",
        since: |at| format!("started_at >= {}", timestamp(at)),
    },
    Explored {
        table: DRIFT_OBSERVATIONS,
        since: |at| format!("created_at >= {}", timestamp(at)),
    },
    Explored {
        table: "bifrost.verification.results",
        since: |at| format!("ended_at >= {}", timestamp(at)),
    },
    Explored {
        table: EXECUTION_INPUTS,
        since: |at| format!("recorded_at >= {}", timestamp(at)),
    },
    Explored {
        table: "bifrost.drift.result_features",
        since: |at| format!("{WYRD_INGESTED_AT} >= {}", timestamp(at)),
    },
];

/// The attribution of one stored row, as the exploration ground truth reads
/// it with no predicate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Attributed {
    /// `service_name`.
    service: Option<String>,
    /// `service_version`.
    version: Option<String>,
    /// `deployment_environment`.
    environment: Option<String>,
    /// `subject_kind`.
    kind: Option<String>,
    /// `subject_name`.
    name: Option<String>,
    /// `workload_card_uid` as text.
    workload: Option<String>,
    /// `subject_card_uid` as text.
    subject: Option<String>,
}

/// The projection reading [`Attributed`] in field order.
const ATTRIBUTED: &str = "service_name, service_version, deployment_environment, subject_kind, \
     subject_name, CAST(workload_card_uid AS VARCHAR), CAST(subject_card_uid AS VARCHAR)";

/// One exploration filter: its SQL predicate and the same predicate over the
/// ground truth.
struct Filter {
    /// Name printed beside its work evidence.
    label: &'static str,
    /// SQL predicate.
    sql: String,
    /// The predicate over one ground-truth row.
    keeps: Box<dyn Fn(&Attributed) -> bool>,
}

/// Bloom filters stored in every published Parquet file under a storage root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StoredBlooms {
    /// Files carrying at least one Bloom filter.
    filtered: usize,
    /// Files carrying none.
    bare: usize,
    /// Bytes of every stored Bloom filter.
    filter_bytes: i64,
    /// Bytes of every file.
    file_bytes: u64,
}

impl StoredBlooms {
    /// Walk `root` and total the Bloom filters of every Parquet file.
    ///
    /// # Errors
    /// Returns a filesystem or Parquet footer error.
    fn read(root: &std::path::Path) -> Result<Self, ServerJourneyError> {
        let mut stored = Self {
            filtered: 0,
            bare: 0,
            filter_bytes: 0,
            file_bytes: 0,
        };
        let mut pending = vec![root.to_path_buf()];
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
                let file = std::fs::File::open(&path)?;
                stored.file_bytes += file.metadata()?.len();
                let reader = SerializedFileReader::new(file)?;
                let bytes: i64 = reader
                    .metadata()
                    .row_groups()
                    .iter()
                    .flat_map(parquet::file::metadata::RowGroupMetaData::columns)
                    .filter_map(parquet::file::metadata::ColumnChunkMetaData::bloom_filter_length)
                    .map(i64::from)
                    .sum();
                stored.filter_bytes += bytes;
                if bytes > 0 {
                    stored.filtered += 1;
                } else {
                    stored.bare += 1;
                }
            }
        }
        Ok(stored)
    }
}

/// One Service release writing in one environment: the exploration's writer.
struct Release {
    /// The Service Card UID.
    uid: CardUid,
    /// Its family runtime account, which stages its verification rows.
    account: PrincipalId,
    /// The accepted deployment environment.
    environment: &'static str,
    /// How many separately flushed rounds it writes: skew across Services.
    rounds: usize,
}

/// Exact Service and subject filters keep their populations on every
/// attributable table, and the Oracle reports the work each filter costs
/// (REQ-026, REQ-049–053; AC-008, AC-017, AC-024).
///
/// Three Service releases write through the public native ingest and the
/// production result builder: `lookup-billing` 1.0.0 in prod over four
/// flushed rounds, `lookup-billing` 2.0.0 in staging and `lookup-checkout`
/// 1.0.0 in prod over one each, so one Service family spans two concurrent
/// releases and environments and one Service is low-volume. Each round
/// observes a shared Agent and a Model of the same name, so Card kinds stay
/// apart. Spans, logs, metric points, drift observations, execution inputs,
/// results, and result features carry the accepted attribution; gateway
/// calls are written by the gateway only and stay empty here.
///
/// Every table answers Service-name aggregation across versions, exact
/// workload UID, environment, subject kind and name across versions, exact
/// subject UID, a combined Service-and-subject filter, an empty filter, a
/// time-bounded filter, and no filter with exactly the rows the unfiltered
/// ground truth selects, printing each filter's Oracle work. Clearing every
/// table's Bloom set and writing one more round changes no answer, both
/// while the round is live and once it is published. Audit's own principal
/// lookup keeps its population. The verification results table answers each
/// of its ten default Bloom columns exactly.
///
/// # Errors
/// Returns cluster, seeding, ingest, publication, alteration, or query
/// errors, or the first filter whose answer differs from the ground truth.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn exploration_filters_keep_exact_populations() -> Result<(), ServerJourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let server = cluster.server(0).ok_or("missing mixed node")?;
    let tenant = cluster.data_tenant_id();
    for table in vala_bifrost_redux::tables::builtin_tables() {
        server
            .ensure_builtin_table_for_test(tenant, table.namespace, table.name)
            .await?;
    }
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    seed.declare(
        "Prompt",
        "lookup-prompt",
        &json!({
            "provider": "openai",
            "model": "gpt-4o-mini",
            "messages": [{ "role": "user", "content": "Answer." }],
        }),
    )
    .await?;
    let agent = seed
        .declare(
            "Agent",
            "lookup-component",
            &json!({
                "prompt": { "kind": "Prompt", "name": "lookup-prompt", "version": "1.0.0" },
                "execution_route": { "kind": "native" },
            }),
        )
        .await?;
    let model = seed
        .declare(
            "Model",
            "lookup-component",
            &json!({
                "interface": { "kind": "Custom", "meta": {
                    "framework_version": "0.1.0", "loader_module": "lookup",
                    "loader_class": "Lookup", "extra": {} } },
                "task_type": "Other",
                "signature": {
                    "inputs": [{ "name": "x", "dtype": "float64" }],
                    "outputs": [{ "name": "y", "dtype": "float64" }],
                },
            }),
        )
        .await?;
    let verifier = seed.drift_verifier("lookup-drift").await?;
    let component = |alias: &str, kind: &str, uid: &CardUid| {
        json!({ "alias": alias, "ref": {
            "kind": kind, "name": "lookup-component", "version": "1.0.0", "uid": uid } })
    };
    let components = json!({ "components": [
        component("agent", "Agent", &agent),
        component("model", "Model", &model),
    ] });
    let mut releases = Vec::new();
    for (name, version, environment, rounds) in [
        ("lookup-billing", "1.0.0", "prod", 4),
        ("lookup-billing", "2.0.0", "staging", 1),
        ("lookup-checkout", "1.0.0", "prod", 1),
    ] {
        let (uid, account) = seed.release_with(name, version, &components).await?;
        releases.push(Release {
            uid,
            account,
            environment,
            rounds,
        });
    }
    let admin = server.bootstrap_service("lookup-admin", &["admin"]).await?;
    let admin = native_client(
        server,
        admin
            .api_key()
            .ok_or("the admin has an API key")?
            .expose_secret(),
    )?;
    let Bootstrap::User { jwt, .. } = server.bootstrap_user("lookup-writer", &["editor"]).await?
    else {
        return Err("a user bootstraps as a user".into());
    };
    let writer = native_client(server, &jwt)?;
    let mut schemas = HashMap::new();
    for table in [SPANS, RECORDS, POINTS, DRIFT_OBSERVATIONS] {
        schemas.insert(
            table,
            Arc::clone(
                TableConfig::describe_with(&admin, table)
                    .await?
                    .user_schema(),
            ),
        );
    }
    let started = Utc::now() - chrono::Duration::hours(1);
    let round = async |release: &Release, flush: bool| -> Result<(), ServerJourneyError> {
        let client = writer.for_service(release.uid.clone(), Some(release.environment.to_owned()));
        let now = Utc::now();
        let nanos = now.timestamp_nanos_opt().unwrap_or(0);
        for subject in [&agent, &model] {
            let frames = [
                (
                    SPANS,
                    canonical_signals::spans(&schemas[SPANS], "lookup", nanos),
                ),
                (
                    RECORDS,
                    canonical_signals::logs(&schemas[RECORDS], "lookup", nanos),
                ),
                (
                    POINTS,
                    canonical_signals::points(&schemas[POINTS], "lookup", nanos),
                ),
                (
                    DRIFT_OBSERVATIONS,
                    drift_observation(&schemas[DRIFT_OBSERVATIONS])?,
                ),
            ];
            for (table, batch) in frames {
                RawIngest::connect(&client)
                    .await?
                    .insert(
                        table,
                        uuid::Uuid::now_v7(),
                        ipc(&with_text(
                            &batch,
                            SUBJECT_CARD_UID,
                            Some(subject.as_str()),
                        )?)?,
                    )
                    .await?;
            }
            let window = DriftWindow {
                start: now - chrono::Duration::hours(1),
                end: now,
            };
            let builder = ResultPayloadBuilder::new(
                ResultRun {
                    execution_id: ExecutionId::new_v7(),
                    run_id: None,
                    verifier_version: "1.0.0",
                    card_uid: subject,
                    owner_card_uid: Some(&release.uid),
                    group_id: None,
                    deployment_environment: Some(release.environment),
                    binding_id: Some(BindingId::new_v7()),
                    trigger: None,
                    window: Some(&window),
                },
                &verifier,
                now,
                now,
            );
            let attribution = Attribution::direct(release.account);
            let outbox = server.state().bifrost.scribe_outbox();
            outbox.stage(
                tenant,
                ScribeWrite::ExecutionInputs {
                    row: builder.inputs(&BTreeMap::new(), None)?,
                    attribution,
                    queued: None,
                },
            );
            outbox.stage(
                tenant,
                ScribeWrite::Result {
                    payload: builder.build(&drift_outcome(Some(two_feature_report()?))?)?,
                    attribution,
                },
            );
        }
        if flush {
            server.flush_bifrost().await?;
        }
        Ok(())
    };
    for release in &releases[..2] {
        for _ in 0..release.rounds {
            round(release, true).await?;
        }
    }
    let checkout = &releases[2];
    ScanEvidence::of(
        &cluster,
        "checkout ingest with blooms",
        round(checkout, false),
    )
    .await?;
    ScanEvidence::of(&cluster, "checkout publish with blooms", async {
        Ok(server.flush_bifrost().await?)
    })
    .await?;
    cluster.refresh_oracle_snapshots().await?;

    let query = Bifrost::query_only(&admin);
    let texts = async |sql: String| -> Result<Vec<Vec<Option<String>>>, ServerJourneyError> {
        let rows: Vec<Value> = query.sql_as(&sql, &[]).await?;
        Ok(rows
            .iter()
            .map(|row| {
                row.as_object()
                    .into_iter()
                    .flat_map(|row| row.values())
                    .map(|value| match value {
                        Value::Null => None,
                        Value::String(text) => Some(text.clone()),
                        other => Some(other.to_string()),
                    })
                    .collect()
            })
            .collect())
    };
    let truth = async |table: &str| -> Result<Vec<Attributed>, ServerJourneyError> {
        let mut rows = Vec::new();
        for row in texts(format!("SELECT {ATTRIBUTED} FROM {table}")).await? {
            let [service, version, environment, kind, name, workload, subject] =
                <[Option<String>; 7]>::try_from(row).map_err(|row| format!("{row:?}"))?;
            rows.push(Attributed {
                service,
                version,
                environment,
                kind,
                name,
                workload,
                subject,
            });
        }
        rows.sort();
        Ok(rows)
    };
    let text = |value: &str| Some(value.to_owned());
    let (billing2, agent_uid) = (releases[1].uid.to_string(), agent.to_string());
    let filters = |explored: &Explored| -> Vec<Filter> {
        let (billing2, agent_uid) = (billing2.clone(), agent_uid.clone());
        let agent_too = agent_uid.clone();
        vec![
            Filter {
                label: "service family",
                sql: "service_name = 'lookup-billing'".to_owned(),
                keeps: Box::new(move |row| row.service == text("lookup-billing")),
            },
            Filter {
                label: "exact release",
                sql: format!("workload_card_uid = '{billing2}'"),
                keeps: Box::new(move |row| row.workload.as_deref() == Some(billing2.as_str())),
            },
            Filter {
                label: "environment",
                sql: "deployment_environment = 'prod'".to_owned(),
                keeps: Box::new(move |row| row.environment == text("prod")),
            },
            Filter {
                label: "subject kind and name",
                sql: "subject_kind = 'Agent' AND subject_name = 'lookup-component'".to_owned(),
                keeps: Box::new(move |row| {
                    row.kind == text("Agent") && row.name == text("lookup-component")
                }),
            },
            Filter {
                label: "exact subject",
                sql: format!("subject_card_uid = '{agent_uid}'"),
                keeps: Box::new(move |row| row.subject.as_deref() == Some(agent_uid.as_str())),
            },
            Filter {
                label: "service and subject",
                sql: format!(
                    "service_name = 'lookup-checkout' AND subject_card_uid = '{agent_too}'"
                ),
                keeps: Box::new(move |row| {
                    row.service == text("lookup-checkout")
                        && row.subject.as_deref() == Some(agent_too.as_str())
                }),
            },
            Filter {
                label: "empty",
                sql: "service_name = 'lookup-absent'".to_owned(),
                keeps: Box::new(|_| false),
            },
            Filter {
                label: "time bounded",
                sql: (explored.since)(started),
                keeps: Box::new(|_| true),
            },
            Filter {
                label: "unbounded",
                sql: "true".to_owned(),
                keeps: Box::new(|_| true),
            },
        ]
    };
    let compare = async |stage: &str| -> Result<(), ServerJourneyError> {
        for explored in &EXPLORED {
            let expected = truth(explored.table).await?;
            for filter in filters(explored) {
                let sql = format!(
                    "SELECT {ATTRIBUTED} FROM {} WHERE {}",
                    explored.table, filter.sql
                );
                let label = format!("{stage} {} {}", explored.table, filter.label);
                let (rows, _) = ScanEvidence::of(&cluster, &label, texts(sql)).await?;
                let mut read = rows
                    .into_iter()
                    .map(|row| {
                        let [service, version, environment, kind, name, workload, subject] =
                            <[Option<String>; 7]>::try_from(row)
                                .map_err(|row| format!("{row:?}"))?;
                        Ok(Attributed {
                            service,
                            version,
                            environment,
                            kind,
                            name,
                            workload,
                            subject,
                        })
                    })
                    .collect::<Result<Vec<_>, ServerJourneyError>>()?;
                read.sort();
                let want: Vec<_> = expected
                    .iter()
                    .filter(|row| (filter.keeps)(row))
                    .cloned()
                    .collect();
                if read != want {
                    return Err(format!("{label}: read {read:?}, expected {want:?}").into());
                }
            }
        }
        let versions = texts(
            "SELECT service_version, COUNT(*) AS n FROM bifrost.traces.spans \
             WHERE service_name = 'lookup-billing' GROUP BY service_version \
             ORDER BY service_version"
                .to_owned(),
        )
        .await?;
        if versions.len() != 2 {
            return Err(
                format!("{stage}: billing aggregates two releases, saw {versions:?}").into(),
            );
        }
        Ok(())
    };
    compare("published").await?;

    let results = truth("bifrost.verification.results").await?;
    if results.len() != 12 || results.iter().any(|row| row.service.is_none()) {
        return Err(format!("twelve attributed results expected, saw {results:?}").into());
    }
    let result_row = texts(
        "SELECT CAST(result_id AS VARCHAR), CAST(execution_id AS VARCHAR), \
         CAST(verifier_card_uid AS VARCHAR), CAST(binding_id AS VARCHAR) \
         FROM bifrost.verification.results LIMIT 1"
            .to_owned(),
    )
    .await?;
    let [result_id, execution_id, verifier_uid, binding_id] =
        <[Option<String>; 4]>::try_from(result_row.first().cloned().ok_or("one result row")?)
            .map_err(|row| format!("{row:?}"))?;
    for (column, value, expected) in [
        ("result_id", result_id, 1),
        ("execution_id", execution_id, 1),
        ("verifier_card_uid", verifier_uid, 12),
        ("binding_id", binding_id, 1),
        ("service_name", text("lookup-checkout"), 2),
        ("subject_name", text("lookup-component"), 12),
        ("workload_card_uid", Some(billing2.clone()), 2),
        ("subject_card_uid", Some(agent.to_string()), 6),
        ("run_id", None, 0),
        ("group_id", None, 0),
    ] {
        let predicate = value.map_or_else(
            || format!("{column} = 'absent-{column}'"),
            |value| format!("{column} = '{value}'"),
        );
        let label = format!("results bloom column {column}");
        let (rows, _) = ScanEvidence::of(
            &cluster,
            &label,
            texts(format!(
                "SELECT outcome FROM bifrost.verification.results WHERE {predicate}"
            )),
        )
        .await?;
        if rows.len() != expected {
            return Err(format!("{label}: {} rows, expected {expected}", rows.len()).into());
        }
    }

    cluster.await_audit_retained().await?;
    let cutoff = timestamp(Utc::now());
    let decided = texts(format!(
        "SELECT CAST({SUB_PRINCIPAL_ID} AS VARCHAR) FROM bifrost.system.audit_log \
         WHERE created_at < {cutoff}"
    ))
    .await?;
    let principal = decided
        .iter()
        .flatten()
        .flatten()
        .next()
        .cloned()
        .ok_or("audited decisions exist")?;
    let expected = decided
        .iter()
        .filter(|row| row.first().cloned().flatten().as_deref() == Some(principal.as_str()))
        .count();
    let (rows, _) = ScanEvidence::of(
        &cluster,
        "audit principal",
        texts(format!(
            "SELECT operation FROM bifrost.system.audit_log \
             WHERE {SUB_PRINCIPAL_ID} = '{principal}' AND created_at < {cutoff}"
        )),
    )
    .await?;
    if rows.len() != expected {
        return Err(format!("audit principal: {} rows, expected {expected}", rows.len()).into());
    }

    let bifrost = Bifrost::connect_with(&admin, BifrostOptions::default()).await?;
    for explored in &EXPLORED {
        let (namespace, name) = explored
            .table
            .rsplit_once('.')
            .ok_or("the FQN names its table")?;
        let before = bifrost.describe_table(namespace, name).await?;
        bifrost
            .alter_table(
                namespace,
                name,
                &wyrd_spec::vala::api::AlterTableRequest {
                    expected_table_version: before.table_version,
                    physical_layout: Some(wyrd_spec::vala::api::PhysicalLayoutEdit {
                        bloom_columns: Some(Vec::new()),
                        ..wyrd_spec::vala::api::PhysicalLayoutEdit::default()
                    }),
                    add_columns: None,
                    rename_columns: None,
                },
            )
            .await?;
    }
    let stored = StoredBlooms::read(cluster.storage_root())?;
    ScanEvidence::of(
        &cluster,
        "checkout ingest without blooms",
        round(checkout, false),
    )
    .await?;
    compare("live without blooms").await?;
    ScanEvidence::of(&cluster, "checkout publish without blooms", async {
        Ok(server.flush_bifrost().await?)
    })
    .await?;
    cluster.refresh_oracle_snapshots().await?;
    compare("published without blooms").await?;
    let cleared = StoredBlooms::read(cluster.storage_root())?;
    println!("stored with default blooms: {stored:?}; after one cleared round: {cleared:?}");
    if stored.bare != 0 || cleared.bare == 0 {
        return Err(format!(
            "default layouts write filtered files and a cleared round filterless ones: \
             {stored:?} then {cleared:?}"
        )
        .into());
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Run the production runtime on `server` until the tenant holds `count`
/// executions and every one settles, and return their rows.
///
/// # Errors
/// Returns the runtime, fixture, or timeout error.
async fn settle_all(
    server: &WyrdTestServer,
    seed: &VerificationFixture,
    count: usize,
) -> Result<Vec<ExecutionRow>, ServerJourneyError> {
    let runtime = VerificationRuntime::builder(server.state())
        .build()
        .ok_or("the runtime did not compose")?;
    let stop = CancellationToken::new();
    let task = tokio::spawn(runtime.run(stop.clone()));
    let deadline = tokio::time::Instant::now() + WAIT;
    let rows = loop {
        let mut rows = Vec::with_capacity(count);
        for execution in seed.executions().await? {
            rows.push(seed.execution(execution).await?);
        }
        if rows.len() == count
            && rows
                .iter()
                .all(|row| !["pending", "running", "retrying"].contains(&row.status.as_str()))
        {
            break rows;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!("{count} executions never settled: {rows:?}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    stop.cancel();
    tokio::time::timeout(WAIT, task).await??;
    Ok(rows)
}

/// A completed execution's status returns its own canonical result through
/// one exact result read, whatever other results its tenant retains
/// (Scenario 1 of TASK-007; REQ-026, REQ-040; AC-008, AC-014).
///
/// Results of the same tenant are published today, yesterday, and three
/// days ago. Two queued trace executions of one Verifier and subject judge
/// different traces, one passing and one failing. While they are pending,
/// each status carries no result and costs no Oracle read. The runtime then
/// collects each trace once. After publication each status, read through the
/// Rust SDK, returns its own verdict and execution with one Oracle statement
/// that scans no more files than the day-partitioned result table holds
/// results for that execution; another execution's result never substitutes.
/// Another tenant's caller and an unknown execution are refused as not found.
///
/// # Errors
/// Returns cluster, fixture, ingest, runtime, SDK, or query errors, or the
/// first status that differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn completed_result_lookup_preserves_evidence() -> Result<(), ServerJourneyError> {
    use wyrd_client::verify::Execution;

    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let server = cluster.server(0).ok_or("missing mixed node")?;
    let tenant = cluster.data_tenant_id();
    server
        .ensure_builtin_table_for_test(tenant, "traces", "spans")
        .await?;
    let older = ResultLayoutJourney::start(&cluster).await?;
    let now = Utc::now();
    for days in [0, 1, 3] {
        older
            .publish(now - chrono::Duration::days(days), Report::Drift, false)
            .await?;
    }
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let (checkout, _) = seed.service("result-checkout").await?;
    let (admin, schema) = span_writer(server).await?;
    let verifier = seed
        .verifier(
            "result-lookup",
            &trace_spec(&[("names", "$.spans[*].name", json!("retrieve"))]),
        )
        .await?;
    let (passing, failing) = (fresh_trace()?, fresh_trace()?);
    let start = nanos_from_now(0);
    for (trace, name) in [(passing, "retrieve"), (failing, "rank")] {
        native_spans(
            &admin,
            uuid::Uuid::now_v7(),
            &trace_batch(&schema, trace, &[(1, name, start)]),
        )
        .await?;
    }
    server.flush_bifrost().await?;
    let mut executions = Vec::new();
    for trace in [passing, failing] {
        let accepted: Value = admin
            .request_json(
                reqwest::Method::POST,
                "/v1/verification/enqueue",
                Some(&trace_body(&verifier, &checkout, trace)),
            )
            .await?;
        executions.push(await_queued(&seed, &accepted).await?);
    }
    for execution in &executions {
        let (pending, work) = ScanEvidence::of(&cluster, "pending status", async {
            Ok(Execution::connect_with(*execution, &admin)
                .await?
                .status()
                .await?)
        })
        .await?;
        if pending.result.is_some() || !ScanEvidence::is(work.queries, 0) {
            return Err(format!("a pending status read a result: {pending:?} {work:?}").into());
        }
    }

    let (rows, collection) = ScanEvidence::of(
        &cluster,
        "two trace executions",
        settle_all(server, &seed, executions.len()),
    )
    .await?;
    if rows
        .iter()
        .any(|row| row.status != "completed" || row.attempts != 1 || row.result_id.is_none())
    {
        return Err(format!("both executions complete on attempt one: {rows:?}").into());
    }
    if !ScanEvidence::is(collection.queries, 2) {
        return Err(format!("each execution collects its trace once: {collection:?}").into());
    }
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    for (execution, verdict) in executions.iter().zip(["passed", "failed"]) {
        let (state, work) = ScanEvidence::of(&cluster, &format!("{verdict} status"), async {
            Ok(Execution::connect_with(*execution, &admin)
                .await?
                .status()
                .await?)
        })
        .await?;
        let result = state
            .result
            .ok_or("a completed status carries its result")?;
        let read = serde_json::to_value(&result.outcome)?;
        if result.execution_id != *execution || read["verdict"] != verdict {
            return Err(format!("{execution} read another result: {read}").into());
        }
        if !ScanEvidence::is(work.queries, 1) || work.files > 1.0 {
            return Err(format!("one exact result read scans one file: {work:?}").into());
        }
    }

    let other = server.seed_tenant("result-lookup-other").await?;
    let outsider = server
        .bootstrap_service_in_tenant(other, "result-outsider", &["admin"])
        .await?;
    let outsider = native_client(
        server,
        outsider
            .api_key()
            .ok_or("the outsider has an API key")?
            .expose_secret(),
    )?;
    for (caller, execution) in [(&outsider, executions[0]), (&admin, ExecutionId::new_v7())] {
        let refused = match Execution::connect_with(execution, caller)
            .await?
            .status()
            .await
        {
            Ok(state) => return Err(format!("{execution} is visible: {state:?}").into()),
            Err(refused) => refused,
        };
        if !matches!(refused, WyrdError::VerificationExecutionNotFound { .. }) {
            return Err(format!("not-found expected, saw {refused:?}").into());
        }
    }
    cluster.shutdown().await?;
    Ok(())
}

/// A scheduled Custom Drift window scores exactly its own subject's `score`
/// rows in `[start, end)` with one Oracle statement per execution
/// (Scenario 2 of TASK-007; REQ-026, REQ-040; AC-008, AC-014).
///
/// Three Card-bound Services are bound monthly to one Custom Drift Verifier
/// whose score is the window mean. `inside` writes `score` 3 at the inclusive
/// month start and 5 a minute ago, plus 1000 a microsecond before the start
/// and an hour after the due end; `other` writes 50 in the window; `empty`
/// writes nothing. Every record also carries a `latency` series. The
/// production runtime then scores `inside` at 4 and `other` at 50, while
/// `empty` stays unscored with no feature row, and the three executions cost
/// three Oracle statements.
///
/// # Errors
/// Returns cluster, seeding, client, runtime, or query errors, or the first
/// population that differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn window_lookup_preserves_population() -> Result<(), ServerJourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let server = cluster.server(0).ok_or("missing mixed node")?;
    let tenant = cluster.data_tenant_id();
    server
        .ensure_builtin_table_for_test(tenant, "drift", "observations")
        .await?;
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let verifier = seed
        .custom_drift_verifier("window-mean", "score", 0.0, 1000.0)
        .await?;
    let now = Utc::now();
    let start = now
        .date_naive()
        .with_day(1)
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .ok_or("the current month has no first instant")?
        .and_utc();
    let inside_scores = [
        (start, 3.0),
        (observed_in_current_month(now)?, 5.0),
        (start - chrono::Duration::microseconds(1), 1000.0),
        (now + chrono::Duration::hours(1), 1000.0),
    ];
    let mut bindings = Vec::new();
    for (name, scores) in [
        ("window-inside", &inside_scores[..]),
        (
            "window-other",
            &[(observed_in_current_month(now)?, 50.0)][..],
        ),
        ("window-empty", &[][..]),
    ] {
        let card = server
            .bootstrap_service_in_tenant(tenant, name, &[])
            .await?
            .card_ref()
            .ok_or("a service bootstrap returned a user")?
            .clone();
        let Bootstrap::Machine { id, api_key, .. } = server
            .credential_registered_service(&card, &["admin"])
            .await?
        else {
            return Err("a service credential returned a user".into());
        };
        let subject = card.uid.clone().ok_or("the subject Card has no UID")?;
        let writer = Bifrost::connect_with(
            &native_client(server, api_key.expose_secret())?.for_workload(subject.clone()),
            BifrostOptions::default(),
        )
        .await?;
        for (at, score) in scores {
            let record = format!("{name}-{}", uuid::Uuid::now_v7());
            writer
                .write_batch(
                    DRIFT_OBSERVATIONS,
                    &observation_batch(&record, &subject, *at, *score)?,
                )
                .await?;
        }
        seed.activate(id, &subject).await?;
        let binding = seed
            .bind_schedule(&subject, &subject, &verifier, MONTHLY, Vec::new())
            .await?;
        seed.make_binding_due(binding).await?;
        bindings.push(binding);
    }
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;

    let (rows, work) = ScanEvidence::of(
        &cluster,
        "three drift windows",
        settle_all(server, &seed, bindings.len()),
    )
    .await?;
    if rows
        .iter()
        .any(|row| row.status != "completed" || row.result_id.is_none())
    {
        return Err(format!("every window completes with a result: {rows:?}").into());
    }
    if !ScanEvidence::is(work.queries, 3) {
        return Err(format!("each window is one statement: {work:?}").into());
    }
    let unwritten = server
        .state()
        .bifrost
        .scribe_outbox()
        .settle(std::time::Instant::now() + WAIT)
        .await;
    if unwritten != 0 {
        return Err(format!("{unwritten} results were never written").into());
    }
    server.flush_bifrost().await?;
    cluster.refresh_oracle_snapshots().await?;
    for (binding, expected) in bindings.iter().zip([Some("4.0"), Some("50.0"), None]) {
        let scores = query_texts(
            server,
            tenant,
            format!(
                "SELECT CAST(score AS VARCHAR) FROM bifrost.drift.result_features \
                 WHERE binding_id = '{binding}' AND feature = 'score'"
            ),
        )
        .await?;
        let scored: Vec<_> = expected
            .map(|score| vec![Some(score.to_owned())])
            .into_iter()
            .collect();
        if scores != scored {
            return Err(format!("binding {binding} scored {scores:?}, not {expected:?}").into());
        }
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Interleaved support conversations each form their own group snapshot of
/// exactly their turns, in recorded order, with one Oracle statement per
/// group (Scenario 3 of TASK-007; REQ-026, REQ-040; AC-008, AC-014).
///
/// The `support` Service declares a per-turn `turn-check` and a `group_end`
/// `conversation-verifier` whose `turns` slot reads turn inputs and whose
/// `answered` slot reads turn results. The Service enqueues conversation
/// `long` (`a`, a blank answer, `c`) interleaved with conversation `short`
/// (`z`). The production runtime judges the four turns, then hands each
/// expired group off once: `long` lists its three turns in order under both
/// slots and fails on the blank turn, `short` lists only its turn and
/// passes, and the whole runtime pass costs one statement per group, since
/// the turns read no warehouse evidence.
///
/// # Errors
/// Returns cluster, declaration, credential, route, runtime, or SDK
/// errors, or the first snapshot that differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn group_lookup_preserves_source_occurrences() -> Result<(), ServerJourneyError> {
    use wyrd_client::verify::Execution;

    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let server = cluster.server(0).ok_or("missing mixed node")?;
    let tenant = cluster.data_tenant_id();
    for table in ["execution_inputs", "results"] {
        server
            .ensure_builtin_table_for_test(tenant, "verification", table)
            .await?;
    }
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant).await?;
    let admin = server.bootstrap_service("group-admin", &["admin"]).await?;
    let admin = native_client(
        server,
        admin
            .api_key()
            .ok_or("the admin has an API key")?
            .expose_secret(),
    )?;
    let receipt = Box::pin(
        wyrd_client::cards::Cards::connect_with(&admin)
            .await?
            .declare_from_path(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../fixtures/cards/verify_in_real_time/support.yaml"
            )),
    )
    .await?;
    let uid = |name: &str| {
        receipt
            .outcomes
            .iter()
            .find(|outcome| outcome.card_ref.name.as_str() == name)
            .and_then(|outcome| outcome.card_ref.uid.clone())
            .ok_or(format!("the receipt names {name}"))
    };
    let (agent, turn_check) = (uid("assistant-agent")?, uid("turn-check")?);
    let root = receipt.root.uid.clone().ok_or("the Service has a UID")?;
    let machine = server
        .credential_registered_service(&receipt.root, &["editor"])
        .await?;
    let token = server
        .exchange_api_key_for_workload(
            machine.api_key().ok_or("the Service has an API key")?,
            &root,
        )
        .await?;
    let service = native_client(server, &token)?;
    let mut turns: BTreeMap<&str, Vec<ExecutionId>> = BTreeMap::new();
    for (group, answer) in [("long", "a"), ("short", "z"), ("long", ""), ("long", "c")] {
        let accepted: Value = service
            .request_json(
                reqwest::Method::POST,
                "/v1/verification/enqueue",
                Some(&json!({
                    "verifier_card_uid": turn_check,
                    "card_uid": agent,
                    "group_id": group,
                    "inputs": {"turn": {"kind": "json", "value": {"answer": answer}}},
                })),
            )
            .await?;
        turns
            .entry(group)
            .or_default()
            .push(await_queued(&seed, &accepted).await?);
    }

    let (rows, work) = ScanEvidence::of(
        &cluster,
        "four turns and two groups",
        settle_all(server, &seed, 6),
    )
    .await?;
    if rows.iter().any(|row| row.status != "completed") {
        return Err(format!("every execution completes: {rows:?}").into());
    }
    if !ScanEvidence::is(work.queries, 2) {
        return Err(format!("each group is read by one statement: {work:?}").into());
    }
    let mut groups = BTreeMap::new();
    for execution in seed.executions().await? {
        let state = Execution::connect_with(execution, &admin)
            .await?
            .status()
            .await?;
        if let (Some(group), Some(snapshot)) = (state.group_id.clone(), state.group.clone()) {
            let verdict =
                serde_json::to_value(&state.result.ok_or("a group result")?.outcome)?["verdict"]
                    .clone();
            let listed: BTreeMap<String, Vec<ExecutionId>> = snapshot
                .sources
                .into_iter()
                .map(|(slot, source)| (slot.to_string(), source.execution_ids))
                .collect();
            groups.insert(group.to_string(), (listed, verdict));
        }
    }
    for (group, verdict) in [("long", "failed"), ("short", "passed")] {
        let own = &turns[group];
        let expected = (
            BTreeMap::from([
                ("answered".to_owned(), own.clone()),
                ("turns".to_owned(), own.clone()),
            ]),
            json!(verdict),
        );
        if groups.get(group) != Some(&expected) {
            return Err(format!("{group} snapshot differs: {groups:?}").into());
        }
    }
    cluster.shutdown().await?;
    Ok(())
}

/// Call MCP `bifrost.query` on `server` as `token` with `arguments`, and
/// return the structured content and whether the call was a tool error.
///
/// Hosted Agent `bifrost.query` and MCP `bifrost.query` collect through the
/// same bounded query owner, so this is the declared-SQL path an Agent check
/// reaches, minus its `tool:invoke` authorization.
///
/// # Errors
/// Returns the transport error, or a description when the call carries no
/// structured content.
async fn mcp_query(
    server: &WyrdTestServer,
    token: &str,
    arguments: Value,
) -> Result<(Value, bool), ServerJourneyError> {
    use rmcp::ClientServiceExt as _;
    use rmcp::transport::StreamableHttpClientTransport;
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;

    let base_url = server
        .base_url()
        .ok_or("the server has no public listener")?;
    let mut config = StreamableHttpClientTransportConfig::with_uri(format!("{base_url}/mcp"));
    config.allow_stateless = true;
    config.custom_headers.insert(
        http::HeaderName::from_static("x-wyrd-access-token"),
        http::HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    let client = ()
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(reqwest::Client::new(), config),
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;
    let result = client
        .call_tool(
            rmcp::model::CallToolRequestParams::new("bifrost.query").with_arguments(
                arguments
                    .as_object()
                    .ok_or("query arguments are an object")?
                    .clone(),
            ),
        )
        .await?;
    Ok((
        result
            .structured_content
            .ok_or("bifrost.query returns structured content")?,
        result.is_error == Some(true),
    ))
}

/// Declared SQL over a custom table returns exactly its own filtered,
/// projected rows from live, published, Bloom-absent and Bloom-added data,
/// with the filter reaching the scan, and its bounds still refuse rather than
/// truncate (Scenario 4 of TASK-007; REQ-026, REQ-040; AC-008, AC-014).
///
/// The admin writes `eu` rows 1 and 2 and `us` row 3 as two published files
/// with no Bloom column, adds a `region` Bloom column, publishes `eu` row 4
/// and `us` row 5 as two more files, and leaves `eu` row 6 live. A
/// workload-bound Service selecting `region = 'eu'` over MCP reads rows 1, 2,
/// 4 and 6, in order, although none of them carries its workload, reading
/// only the two `eu` files' row groups, and with `LIMIT 3` reads exactly rows
/// 1, 2 and 4. The same statement under three rows or ten bytes, past a
/// one-millisecond deadline, or as a caller with no read grant is refused
/// with no rows.
///
/// # Errors
/// Returns cluster, registration, ingest, credential, or MCP errors, or the
/// first answer or refusal that differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn custom_query_preserves_evidence_and_limits() -> Result<(), ServerJourneyError> {
    let cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed()).await?;
    let server = cluster.server(0).ok_or("missing mixed node")?;
    let key = |name: &'static str, roles: &'static [&'static str]| async move {
        let service = server.bootstrap_service(name, roles).await?;
        let uid = service
            .card_ref()
            .and_then(|card| card.uid.clone())
            .ok_or("the Service has a Card UID")?;
        let key = service
            .api_key()
            .ok_or("the Service has an API key")?
            .clone();
        Ok::<_, ServerJourneyError>((uid, key))
    };
    let (_, admin_key) = key("custom-admin", &["admin"]).await?;
    let admin = native_client(server, admin_key.expose_secret())?;
    let fqn = format!(
        "bifrost.datasets.custom_lookup_{}",
        uuid::Uuid::now_v7().simple()
    );
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("region", DataType::Utf8, false),
    ]));
    let bifrost = Bifrost::connect_with(
        &admin,
        BifrostOptions {
            table: Some(TableConfig::from_arrow(&fqn, Arc::clone(&schema))?),
            ..BifrostOptions::default()
        },
    )
    .await?;
    bifrost.register().await?;
    let write = |ids: Vec<i64>, region: &'static str| {
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(arrow::array::Int64Array::from(ids.clone())),
                Arc::new(StringArray::from(vec![region; ids.len()])),
            ],
        );
        let (bifrost, fqn) = (&bifrost, &fqn);
        async move {
            bifrost.write_batch(fqn, &batch?).await?;
            Ok::<_, ServerJourneyError>(())
        }
    };
    write(vec![1, 2], "eu").await?;
    server.flush_bifrost().await?;
    write(vec![3], "us").await?;
    server.flush_bifrost().await?;
    let (namespace, name) = fqn.rsplit_once('.').ok_or("the FQN names its table")?;
    let before = bifrost.describe_table(namespace, name).await?;
    bifrost
        .alter_table(
            namespace,
            name,
            &wyrd_spec::vala::api::AlterTableRequest {
                expected_table_version: before.table_version,
                physical_layout: Some(wyrd_spec::vala::api::PhysicalLayoutEdit {
                    bloom_columns: Some(vec!["region".to_owned()]),
                    ..wyrd_spec::vala::api::PhysicalLayoutEdit::default()
                }),
                add_columns: None,
                rename_columns: None,
            },
        )
        .await?;
    write(vec![4], "eu").await?;
    server.flush_bifrost().await?;
    write(vec![5], "us").await?;
    server.flush_bifrost().await?;
    write(vec![6], "eu").await?;
    cluster.refresh_oracle_snapshots().await?;

    let (agent, agent_key) = key("custom-agent", &["admin"]).await?;
    let agent = server
        .exchange_api_key_for_workload(&agent_key, &agent)
        .await?;
    let sql = format!("SELECT id FROM {fqn} WHERE region = 'eu' ORDER BY id");
    let ((answer, refused), work) = ScanEvidence::of(
        &cluster,
        "custom eu lookup",
        mcp_query(server, &agent, json!({ "sql": sql })),
    )
    .await?;
    if refused || answer["rows"] != json!([[1], [2], [4], [6]]) {
        return Err(format!("the declared SQL changed meaning: {answer}").into());
    }
    // The file count subtracts open-time statistics skips, which may land
    // after the terminal; the two `eu` files' row groups are exact.
    if !ScanEvidence::is(work.queries, 1) || !ScanEvidence::is(work.row_groups, 2) {
        return Err(format!("the region filter reaches the scan: {work:?}").into());
    }
    let (top, refused) =
        mcp_query(server, &agent, json!({ "sql": format!("{sql} LIMIT 3") })).await?;
    if refused || top["rows"] != json!([[1], [2], [4]]) {
        return Err(format!("the limit keeps the lowest ids: {top}").into());
    }

    let (_, outsider_key) = key("custom-outsider", &[]).await?;
    let outsider = server.exchange_api_key(&outsider_key).await?;
    for (caller, arguments, code) in [
        (
            &agent,
            json!({ "sql": sql, "max_rows": 3 }),
            "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        ),
        (
            &agent,
            json!({ "sql": sql, "max_bytes": 10 }),
            "WYRD_VALA_413_QUERY_RESULT_TOO_LARGE",
        ),
        (&agent, json!({ "sql": sql, "deadline_ms": 1 }), "504"),
        (&outsider, json!({ "sql": sql }), "403"),
    ] {
        let (problem, refused) = mcp_query(server, caller, arguments.clone()).await?;
        let seen = problem["code"].as_str().unwrap_or_default();
        if !refused || !seen.contains(code) || problem.get("rows").is_some() {
            return Err(format!("{arguments} must refuse with {code}: {problem}").into());
        }
    }
    cluster.shutdown().await?;
    Ok(())
}
