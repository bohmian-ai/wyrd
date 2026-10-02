//! Verification journey benchmark.
//!
//! Reads as the production journey it measures. An operator starts one
//! release `wyrd-server` through the local-development guide (`migrate`,
//! serve, `setup`) for eight tenants, exporting the server's own sampled
//! traces to a local OTLP collector. Each tenant administrator registers a
//! Service whose Model is Drift-verified on a minutely schedule and whose
//! Agent is Eval-verified on arriving observations with a local HTTP
//! Operator for failed verdicts. Two public application clients per tenant
//! then drive the offered-load profile (warmup, steady, burst, drain): every
//! iteration opens a Run, emits one Drift and one Eval observation, writes
//! one custom dataset row, and exports OTLP spans, logs, and metrics. Each
//! administrator requests one manual Drift run at the start of steady.
//!
//! Evidence comes from client tallies, `/metrics` scrapes at every phase
//! boundary, the server cgroup, the sampled traces, the tenant's own
//! read-back, and the run queue read by the database owner. The report
//! reconciles offered against acknowledged, activated, settled, and backlog
//! work and fails on any mismatch; capacity below the offered rate is
//! labelled, not failed.
//!
//! Run through `mise run bench:verification:journey`. The report and server
//! log land in `target/verification-journey/`. Exits nonzero when any check
//! fails.

mod collector;
mod report;
mod tenant;
mod traffic;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use chrono::Utc;
use secrecy::ExposeSecret as _;
use sqlx::PgPool;
use tokio::time::Instant;
use wyrd_client::{Verification, WyrdClient};
use wyrd_spec::verification::{
    StartVerificationRunRequest, VerificationRunInput, VerificationRunTarget,
};
use wyrd_testing::release_server::LocalServer;

use collector::Collector;
use report::{PhaseRecord, Report, Snapshot, TenantTally, Totals};
use tenant::{TABLE, Tenant};
use traffic::{Client, EVIDENCE_MARKER, PROFILE, PhaseLoad};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Tenants the server is set up for.
const TENANTS: usize = 8;

/// Concurrent application clients per tenant.
const CLIENTS_PER_TENANT: usize = 2;

/// Fraction of its own traces the server samples and exports.
const SAMPLE_RATIO: &str = "0.05";

/// Window the manual Drift run analyzes, ending when it is requested.
const MANUAL_WINDOW: chrono::TimeDelta = chrono::TimeDelta::minutes(5);

/// Times the queue tally and the settled-run metric are re-read until they
/// describe one instant.
const STABLE_READS: usize = 20;

/// Run rows per tenant by origin and status, read by the database owner.
const RUNS_SQL: &str = "SELECT data_tenant_id::text AS tenant, origin, status, COUNT(*) AS n \
     FROM wyrd.verifier_runs GROUP BY 1, 2, 3";

/// Operator dispatch rows per tenant by status, read by the database owner.
const DISPATCHES_SQL: &str = "SELECT data_tenant_id::text AS tenant, status, COUNT(*) AS n \
     FROM wyrd.operator_dispatches GROUP BY 1, 2";

/// The journey: start, provision, drive the profile, reconcile, stop.
///
/// Returns whether every check passed.
///
/// # Errors
///
/// Returns a server, client, or evidence failure that stopped the journey.
async fn benchmark() -> Result<bool> {
    let output = PathBuf::from("target/verification-journey");
    if output.exists() {
        std::fs::remove_dir_all(&output)?;
    }
    std::fs::create_dir_all(&output)?;

    let collector = Collector::start().await?;
    let started = std::time::Instant::now();
    let slugs: Vec<String> = (0..TENANTS).map(|index| format!("vbench{index}")).collect();
    let slug_refs: Vec<&str> = slugs.iter().map(String::as_str).collect();
    let server = LocalServer::start(
        &release_binary()?,
        &slug_refs,
        &[
            ("WYRD_OTLP_ENDPOINT", collector.trace_endpoint()),
            ("WYRD_OTLP_PROTOCOL", "grpc"),
            ("WYRD_OTLP_SAMPLE_RATIO", SAMPLE_RATIO),
        ],
    )
    .await?;
    let owner = PgPool::connect(&std::env::var("WYRD_TEST_DATABASE_ADMIN_URL")?).await?;
    let mut tenants = Vec::new();
    for setup in server.tenants() {
        tenants.push(
            Tenant::provision(
                setup,
                &collector.operator_url(&setup.slug),
                &output.join("tenants"),
            )
            .await?,
        );
    }
    let mut clients = Vec::new();
    for (index, tenant) in tenants.iter().enumerate() {
        for _ in 0..CLIENTS_PER_TENANT {
            let slot = clients.len();
            clients.push(Client::start(tenant, index, slot).await?);
        }
    }
    let setup_seconds = started.elapsed().as_secs_f64();
    let shape = (tenants.len(), clients.len());

    let mut snapshots = vec![snapshot(&server).await?];
    let mut phases = Vec::new();
    let mut flushes = Vec::new();
    let mut manual_requested = vec![0_u64; tenants.len()];
    let mut offered = vec![0_u64; tenants.len()];
    let mut submitted = vec![0_u64; tenants.len()];
    for phase in PROFILE {
        if phase.name == "steady" {
            for (index, tenant) in tenants.iter().enumerate() {
                request_manual_drift(tenant).await?;
                manual_requested[index] += 1;
            }
        }
        let peak = server.memory_peak()?;
        let measured = std::time::Instant::now();
        let start = Instant::now();
        let mut load = PhaseLoad::new()?;
        if phase.rate == 0 {
            for client in &clients {
                flushes.push(client.finish().await);
            }
            tokio::time::sleep_until(start + std::time::Duration::from_secs(phase.seconds)).await;
        } else {
            let total = clients.len();
            let running: Vec<_> = clients
                .into_iter()
                .map(|mut client| {
                    tokio::spawn(async move {
                        let load = client.drive(phase, total, start).await;
                        (client, load)
                    })
                })
                .collect();
            clients = Vec::new();
            for task in running {
                let (client, client_load) = task.await?;
                let client_load = client_load?;
                offered[client.tenant] += client_load.planned;
                submitted[client.tenant] += client_load.submitted;
                load.merge(&client_load)?;
                clients.push(client);
            }
        }
        phases.push(PhaseRecord {
            phase,
            load,
            seconds: measured.elapsed().as_secs_f64(),
            peak_memory: peak.read()?,
        });
        snapshots.push(snapshot(&server).await?);
    }

    let mut tallies = Vec::new();
    let operator_posts = collector.operator_posts().await;
    for (index, tenant) in tenants.iter().enumerate() {
        let mut acked = traffic::Evidence::default();
        for client in clients.iter().filter(|client| client.tenant == index) {
            acked.add(&client.evidence);
        }
        tallies.push(TenantTally {
            slug: tenant.slug.clone(),
            offered: offered[index],
            submitted: submitted[index],
            acked,
            stored: stored_evidence(tenant).await?,
            results: tenant
                .count("SELECT COUNT(*) AS n FROM vala.verification.results")
                .await?,
            foreign_results: tenant
                .count(&format!(
                    "SELECT COUNT(*) AS n FROM vala.verification.results \
                     WHERE subject_card_uid NOT IN ('{}', '{}')",
                    tenant.model_uid, tenant.agent_uid
                ))
                .await?,
            operator_posts: operator_posts.get(&tenant.slug).copied().unwrap_or(0),
            manual_requested: manual_requested[index],
            ..TenantTally::default()
        });
    }
    let terminal_metric = settle_queue(&server, &owner, &tenants, &mut tallies).await?;
    let enqueue_failures = server
        .metrics()
        .await?
        .sum("verification_observation_enqueue_failures_total", &[]);

    let shutdown = server
        .stop(&output.join("server.log"))
        .map_err(|error| error.to_string());
    let mut forbidden: Vec<String> = tenants
        .iter()
        .map(|tenant| tenant.service_key.expose_secret().to_owned())
        .collect();
    forbidden.push(EVIDENCE_MARKER.to_owned());
    let tenant_ids: Vec<String> = tenants
        .iter()
        .map(|tenant| tenant.tenant_id.clone())
        .collect();
    let report = Report {
        setup_seconds,
        shape,
        phases,
        snapshots,
        flushes,
        tenants: tallies,
        totals: Totals {
            enqueue_failures: enqueue_failures as u64,
            terminal_metric,
        },
        traces: collector.finish(&tenant_ids, &forbidden),
        sample_ratio: SAMPLE_RATIO,
        shutdown,
    };
    let rendered = report.render();
    println!("{rendered}");
    report.write_to(&output)?;
    Ok(report.passed())
}

/// Scrapes `/metrics` and the server cgroup's CPU counter.
///
/// # Errors
///
/// Returns the scrape failure.
async fn snapshot(server: &LocalServer) -> Result<Snapshot> {
    Ok(Snapshot {
        metrics: server.metrics().await?,
        cpu_us: server.cgroup_stat("cpu.stat", "usage_usec"),
    })
}

/// Requests one direct Drift run of `tenant`'s Verifier over its Model for
/// the last [`MANUAL_WINDOW`], as the administrator.
///
/// # Errors
///
/// Returns the refused request.
async fn request_manual_drift(tenant: &Tenant) -> Result<()> {
    let end = Utc::now();
    Verification::with_client(WyrdClient::clone(&tenant.admin))
        .start_run(
            &StartVerificationRunRequest {
                target: VerificationRunTarget::Verifier {
                    verifier_uid: tenant.drift_verifier_uid.clone(),
                    subject_card_uid: tenant.model_uid.clone(),
                },
                input: VerificationRunInput::DriftWindow {
                    start: end - MANUAL_WINDOW,
                    end,
                },
            },
            None,
        )
        .await?;
    Ok(())
}

/// Reads back every signal `tenant` stored, through its own query surface.
///
/// # Errors
///
/// Returns a query failure.
async fn stored_evidence(tenant: &Tenant) -> Result<traffic::Evidence> {
    Ok(traffic::Evidence {
        drift: tenant
            .count("SELECT COUNT(DISTINCT run_id) AS n FROM vala.drift.observations")
            .await?,
        eval: tenant
            .count("SELECT COUNT(*) AS n FROM vala.eval.observations")
            .await?,
        failing_eval: tenant
            .count(&format!(
                "SELECT COUNT(*) AS n FROM vala.verification.results \
                 WHERE verdict = 'failed' AND subject_card_uid = '{}'",
                tenant.agent_uid
            ))
            .await?,
        custom: tenant
            .count(&format!("SELECT COUNT(*) AS n FROM {TABLE}"))
            .await?,
        spans: tenant
            .count("SELECT COUNT(*) AS n FROM vala.traces.spans")
            .await?,
        logs: tenant
            .count("SELECT COUNT(*) AS n FROM vala.logs.records")
            .await?,
        points: tenant
            .count("SELECT COUNT(*) AS n FROM vala.metrics.points")
            .await?,
    })
}

/// Fills each tally's run, completion, and dispatch counts from the queue and
/// returns the settled-run metric read at the same instant.
///
/// Scheduled Drift keeps settling, so the queue is read on both sides of the
/// scrape until the two reads agree.
///
/// # Errors
///
/// Returns a query or scrape failure, or a queue that never held still.
async fn settle_queue(
    server: &LocalServer,
    owner: &PgPool,
    tenants: &[Tenant],
    tallies: &mut [TenantTally],
) -> Result<u64> {
    let mut before = queue(owner).await?;
    for _ in 0..STABLE_READS {
        let metric = server
            .metrics()
            .await?
            .sum("wyrd_verification_trigger_to_terminal_seconds_count", &[]);
        let after = queue(owner).await?;
        if after == before {
            let (runs, dispatches) = after;
            for (tenant, tally) in tenants.iter().zip(tallies.iter_mut()) {
                tally.runs = runs.get(&tenant.tenant_id).cloned().unwrap_or_default();
                tally.completed = tally
                    .runs
                    .iter()
                    .filter(|((_, status), _)| status == "completed")
                    .map(|(_, count)| count)
                    .sum();
                tally.dispatches = dispatches
                    .get(&tenant.tenant_id)
                    .cloned()
                    .unwrap_or_default();
            }
            return Ok(metric as u64);
        }
        before = after;
    }
    Err("the run queue kept changing while it was read".into())
}

/// Run rows by tenant, then `(origin, status)`.
type RunCounts = BTreeMap<String, BTreeMap<(String, String), u64>>;

/// Dispatch rows by tenant, then status.
type DispatchCounts = BTreeMap<String, BTreeMap<String, u64>>;

/// One read of the run and dispatch queues.
///
/// # Errors
///
/// Returns the query failure.
async fn queue(owner: &PgPool) -> Result<(RunCounts, DispatchCounts)> {
    let mut runs = RunCounts::new();
    for (tenant, origin, status, count) in
        sqlx::query_as::<_, (String, String, String, i64)>(RUNS_SQL)
            .fetch_all(owner)
            .await?
    {
        runs.entry(tenant)
            .or_default()
            .insert((origin, status), u64::try_from(count)?);
    }
    let mut dispatches = DispatchCounts::new();
    for (tenant, status, count) in sqlx::query_as::<_, (String, String, i64)>(DISPATCHES_SQL)
        .fetch_all(owner)
        .await?
    {
        dispatches
            .entry(tenant)
            .or_default()
            .insert(status, u64::try_from(count)?);
    }
    Ok((runs, dispatches))
}

/// The release `wyrd-server` built beside this binary.
///
/// # Errors
///
/// Returns an error when it has not been built.
fn release_binary() -> Result<PathBuf> {
    let binary = std::env::current_exe()?.with_file_name("wyrd-server");
    if binary.is_file() {
        Ok(binary)
    } else {
        Err(format!("{} is not built; run through mise", binary.display()).into())
    }
}

/// Installs a stderr log subscriber when `WYRD_LOG`, else `RUST_LOG`, is set,
/// so client-side failures read alongside the server log.
fn install_tracing() {
    let Ok(filter) = std::env::var("WYRD_LOG").or_else(|_| std::env::var("RUST_LOG")) else {
        return;
    };
    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr)
            .finish(),
    );
}

/// Runs the benchmark and exits nonzero on any failed check or error.
#[tokio::main]
async fn main() -> ExitCode {
    install_tracing();
    match benchmark().await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("verification journey benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
