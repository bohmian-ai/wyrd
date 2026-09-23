//! Verification runtime journey on a role-separated cluster.
//!
//! The runner lives on an Oracle-only node with no local Scribe, so the only
//! way its result can reach Bifrost is the tenant SYSTEM token over gRPC to
//! the Scribe node's public ingest endpoint. The journey schedules one
//! binding-created run, lets the runtime execute and publish a non-empty Drift
//! result, and reads the summary and its feature rows back through the
//! server's own query entry, asserting every identity they carry.

use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use wyrd_server::query::scheduled::ScheduledQueryCaller;
use wyrd_server::verification::VerificationRuntime;
use wyrd_server::verification::engines::{EngineOutcome, VerifierReport};
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::runner::EngineScript;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_spec::vala::managed_columns::{CARD_UID, PRINCIPAL_ID, RUN_ID, WYRD_EVENT_TIME};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};
use wyrd_testing::verification::VerificationFixture;

use super::query::{ServerJourneyError, scheduled_context};

/// Upper bound on the wait for the remote runner to settle the run.
const WAIT: Duration = Duration::from_secs(60);

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
        visibility: VisibilityMode::PublishedOnly,
        freshness: FreshnessPolicy::Strict,
        deadline_ms: Some(30_000),
    })
    .await?;
    Ok(outcome.rows)
}
