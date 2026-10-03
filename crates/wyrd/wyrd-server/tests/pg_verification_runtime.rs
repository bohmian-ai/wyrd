//! Integration coverage for the supervised verification runtime against real
//! Postgres and a real bound server's Bifrost write path.
//!
//! Every test boots the production bound server (gRPC, Gate, Scribe), seeds
//! one tenant through [`VerificationFixture`], and composes a
//! [`VerificationRuntime`] on that server's own state with a scripted engine.
//! Coordination deadlines are PostgreSQL's, so a test that needs one to elapse
//! places the row itself in the past through the fixture. Results travel
//! through the server's internal capture writer to its own Scribe, wrapped in
//! a [`PublicationFault`] that records every submitted batch; run state is
//! read back from `wyrd.verifier_runs` and audit from `vala.audit_staging`,
//! whose publisher is disabled so staged rows stay observable.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::Float64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use datafusion::parquet::arrow::ArrowWriter;
use sha2::{Digest as _, Sha256};
use sqlx::{AssertSqlSafe, PgPool, Postgres, Transaction};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_drift::{DriftReport, DriftVerdict, FeatureDriftReport};
use wyrd_server::verification::drift::DRIFT_INVALID;
use wyrd_server::verification::engines::{EngineOutcome, VerifierReport};
use wyrd_server::verification::fault::{PublicationFault, SentBatch};
use wyrd_server::verification::fitter::{BaselineFitter, FitGate};
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::observations::ObservationRunSink;
use wyrd_server::verification::runner::{EngineScript, VERIFIER_UNAVAILABLE};
use wyrd_server::verification::{CapabilityCrash, RuntimeLimits, VerificationRuntime};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::DriftMethod;
use wyrd_spec::card::verifier::DriftBaselineState;
use wyrd_spec::ids::{BindingId, CardUid, FeatureName, VerificationRunId};
use wyrd_spec::verification::{DriftWindow, FrozenTarget, VerificationError};
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::storage::artifact_metadata::{self, NewArtifactMetadata};
use wyrd_sql::queries::verifier_runs::{ObservationRecord, TerminalStatus};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::logs::LogCapture;
use wyrd_testing::verification::{RunRow, VerificationFixture};

/// Upper bound on every wait for the runtime to make progress.
const WAIT: Duration = Duration::from_secs(30);

/// Summary table every completed result ends with.
const RESULTS: &str = "vala.verification.results";

/// Drift detail table.
const FEATURES: &str = "vala.drift.result_features";

/// One bound server, one seeded tenant, and the controls a runtime is built with.
struct Harness {
    /// The production bound server whose Scribe receives results.
    server: WyrdTestServer,
    /// Seeded verification state of the fixture tenant.
    seed: VerificationFixture,
    /// Active Service Card the runs verify.
    subject: CardUid,
    /// Active Custom Drift Verifier Card the runs execute.
    verifier: CardUid,
    /// Superuser pool for audit staging assertions.
    assertion: PgPool,
    /// Faults and the submitted-batch record every runtime writes through.
    fault: PublicationFault,
    /// Highest staged audit sequence before the test acted.
    audit_floor: i64,
}

impl Harness {
    /// Boot a bound server and seed its fixture tenant with one subject and
    /// one Verifier.
    ///
    /// # Panics
    /// Panics when the server, seeding, or the audit floor read fails.
    async fn start() -> Self {
        let server = WyrdTestServer::builder()
            .without_audit_publication_for_test()
            .start_bound()
            .await
            .expect("bound server starts");
        let tenant = server.pg_fixture().data_tenant_id();
        let (seed, subject, verifier) = seed_tenant(&server, tenant).await;
        let assertion = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("fixture exposes a superuser pool");
        let mut harness = Self {
            server,
            seed,
            subject,
            verifier,
            assertion,
            fault: PublicationFault::default(),
            audit_floor: 0,
        };
        harness.audit_floor = harness.max_audit_seq().await;
        harness
    }

    /// Runtime bounds fast enough for tests: short polls and backoff, and a
    /// one-minute lease a test can expire in the database.
    fn limits() -> RuntimeLimits {
        RuntimeLimits {
            lease: Duration::from_secs(60),
            execution_timeout: Duration::from_secs(20),
            drain_grace: Duration::from_secs(10),
            poll_interval: Duration::from_millis(50),
            restart_backoff: Duration::from_millis(300),
            ..RuntimeLimits::default()
        }
    }

    /// Compose a full runtime writing results through this server's Scribe
    /// wrapped in the harness's [`PublicationFault`].
    ///
    /// # Panics
    /// Panics when the runtime cannot compose.
    fn runtime(
        &self,
        limits: RuntimeLimits,
        script: &EngineScript,
        crash: &CapabilityCrash,
    ) -> VerificationRuntime {
        VerificationRuntime::builder(self.server.state())
            .limits(limits)
            .engine_script(script.clone())
            .publication_fault(self.fault.clone())
            .crash_switch(crash.clone())
            .build()
            .expect("the runtime composes")
    }

    /// Spawn a full runtime with `script` and no crash; returns its stop
    /// token and task.
    fn spawn(&self, limits: RuntimeLimits, script: &EngineScript) -> RunningRuntime {
        RunningRuntime::spawn(self.runtime(limits, script, &CapabilityCrash::default()))
    }

    /// Enqueue one manual direct run over the hour before now.
    ///
    /// # Panics
    /// Panics when the queue refuses the run.
    async fn enqueue(&self) -> VerificationRunId {
        let now = Utc::now();
        self.seed
            .enqueue_direct(
                &self.verifier,
                &self.subject,
                DriftWindow {
                    start: now - chrono::Duration::hours(1),
                    end: now,
                },
            )
            .await
            .expect("run enqueues")
    }

    /// Bring `binding`'s schedule cursor due in the database.
    ///
    /// # Panics
    /// Panics when the update fails.
    async fn make_due(&self, binding: BindingId) {
        self.seed
            .make_binding_due(binding)
            .await
            .expect("binding is due");
    }

    /// Bring `run`'s lease or retry deadline due in the database.
    ///
    /// The queue's deadlines are PostgreSQL's, so a test that needs one to
    /// elapse moves the row instead of a process clock.
    ///
    /// # Panics
    /// Panics when the update fails.
    async fn expire(&self, run: VerificationRunId) {
        self.seed
            .expire_deadlines(run)
            .await
            .expect("deadlines expire");
    }

    /// Poll `run` until `done` holds, returning its final state.
    ///
    /// # Panics
    /// Panics when the run cannot be read or `done` never holds within [`WAIT`].
    async fn wait_run(&self, run: VerificationRunId, done: impl Fn(&RunRow) -> bool) -> RunRow {
        wait_run_in(&self.seed, run, done).await
    }

    /// The destination table of every result batch the runtime submitted to
    /// Scribe, in submission order, resends included.
    fn writes(&self) -> Vec<String> {
        self.fault
            .sent()
            .into_iter()
            .map(|sent| sent.table)
            .collect()
    }

    /// Every staged audit operation for the tenant since the floor.
    ///
    /// # Panics
    /// Panics when the staging read fails.
    async fn audit_operations(&self) -> Vec<String> {
        self.server
            .wait_oracle_audit_staged(std::time::Duration::from_secs(30))
            .await
            .expect("audit outbox settles");
        sqlx::query_scalar(
            "SELECT operation FROM vala.audit_staging \
             WHERE data_tenant_id = $1 AND seq > $2 ORDER BY seq",
        )
        .bind(self.seed.tenant().as_uuid())
        .bind(self.audit_floor)
        .fetch_all(&self.assertion)
        .await
        .expect("staged audit operations")
    }

    /// Flush the server's Scribe and count the tenant's durable rows per
    /// `namespace.table`.
    ///
    /// Counts come from the published file list, so a replay Scribe
    /// deduplicated contributes no rows while a fresh batch does.
    ///
    /// # Panics
    /// Panics when the flush or the file-list read fails.
    async fn durable_rows(&self) -> BTreeMap<String, i64> {
        self.server
            .flush_bifrost()
            .await
            .expect("Scribe flushes the published results");
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT namespace || '.' || table_name, SUM(row_count)::bigint \
             FROM vala.file_list WHERE data_tenant_id = $1 GROUP BY 1",
        )
        .bind(self.seed.tenant().as_uuid())
        .fetch_all(&self.assertion)
        .await
        .expect("durable row counts");
        rows.into_iter().collect()
    }

    /// Open a superuser transaction holding `SHARE` on `table`.
    ///
    /// Reads still proceed, but every insert or update of `table` blocks until
    /// the returned transaction ends. On `wyrd.verifier_runs` this parks a
    /// scheduler occurrence transaction at its enqueue and a runner claim
    /// transaction at its first update; on `wyrd.drift_baselines` it parks a
    /// fitter claim transaction. Each stays inside its uncommitted tenant
    /// transaction.
    ///
    /// # Panics
    /// Panics when the transaction or lock cannot be taken.
    async fn block_writes(&self, table: &'static str) -> Transaction<'static, Postgres> {
        let mut blocker = self.assertion.begin().await.expect("blocker begins");
        // `table` is a test-constant identifier, not caller input.
        sqlx::query(AssertSqlSafe(format!("LOCK TABLE {table} IN SHARE MODE")))
            .execute(&mut *blocker)
            .await
            .expect("table locks");
        blocker
    }

    /// Wait until a backend is blocked writing `table` and return its PID.
    ///
    /// Polls `pg_locks` for an ungranted `RowExclusiveLock`, so the caller
    /// knows the runtime is parked inside its tenant transaction.
    ///
    /// # Panics
    /// Panics when no writer blocks within [`WAIT`].
    async fn wait_blocked_writer(&self, table: &'static str) -> i32 {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let pid: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_locks \
                 WHERE relation = $1::text::regclass \
                   AND mode = 'RowExclusiveLock' AND NOT granted \
                 LIMIT 1",
            )
            .bind(table)
            .fetch_optional(&self.assertion)
            .await
            .expect("lock waiters read");
            if let Some(pid) = pid {
                return pid;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no runtime write blocked on {table}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Wait until backend `pid` has left its transaction: it is idle in the
    /// pool or its connection is closed.
    ///
    /// Every open transaction holds its own `virtualxid` lock, which
    /// `pg_locks` shows to any role, so its absence proves the parked
    /// transaction ended by commit or rollback.
    ///
    /// # Panics
    /// Panics when the backend stays in a transaction past [`WAIT`].
    async fn wait_backend_settled(&self, pid: i32) {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let open: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_locks \
                 WHERE pid = $1 AND locktype = 'virtualxid')",
            )
            .bind(pid)
            .fetch_one(&self.assertion)
            .await
            .expect("backend locks read");
            if !open {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "backend {pid} never left its transaction"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// The stored schedule cursor of `binding`.
    ///
    /// # Panics
    /// Panics when the binding cannot be read.
    async fn cursor(&self, binding: BindingId) -> Option<DateTime<Utc>> {
        sqlx::query_scalar(
            "SELECT next_run_at FROM wyrd.verification_bindings WHERE binding_id = $1",
        )
        .bind(binding.as_uuid())
        .fetch_one(&self.assertion)
        .await
        .expect("binding cursor reads")
    }

    /// Wait until a backend is blocked acquiring advisory lock `key` and
    /// return its PID.
    ///
    /// # Panics
    /// Panics when no backend blocks on `key` within [`WAIT`].
    async fn wait_advisory_waiter(&self, key: i64) -> i32 {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let pid: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_locks \
                 WHERE locktype = 'advisory' AND classid = 0 AND objid = $1::oid \
                   AND NOT granted \
                 LIMIT 1",
            )
            .bind(key)
            .fetch_optional(&self.assertion)
            .await
            .expect("advisory waiters read");
            if let Some(pid) = pid {
                return pid;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no backend blocked on advisory lock {key}"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// The stored lease of `run`: `(lease_token, lease_expires_at)`.
    ///
    /// # Panics
    /// Panics when the run cannot be read.
    async fn lease(&self, run: VerificationRunId) -> (Option<Uuid>, Option<DateTime<Utc>>) {
        sqlx::query_as(
            "SELECT lease_token, lease_expires_at FROM wyrd.verifier_runs WHERE run_id = $1",
        )
        .bind(run.as_uuid())
        .fetch_one(&self.assertion)
        .await
        .expect("run lease reads")
    }

    /// Operator dispatches durably created for `run`.
    ///
    /// # Panics
    /// Panics when the dispatch table cannot be read.
    async fn dispatches(&self, run: VerificationRunId) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.operator_dispatches WHERE run_id = $1")
            .bind(run.as_uuid())
            .fetch_one(&self.assertion)
            .await
            .expect("dispatches read")
    }

    /// Poll until the tenant has exactly one run and return it.
    ///
    /// # Panics
    /// Panics when no run exists within [`WAIT`] or more than one does.
    async fn wait_single_run(&self) -> VerificationRunId {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let runs = self.seed.runs().await.expect("runs read");
            assert!(runs.len() <= 1, "one occurrence yields one run: {runs:?}");
            if let Some(run) = runs.first() {
                return *run;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the occurrence was never scheduled"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Highest staged audit sequence of the tenant.
    ///
    /// # Panics
    /// Panics when the staging read fails.
    async fn max_audit_seq(&self) -> i64 {
        self.server
            .wait_oracle_audit_staged(std::time::Duration::from_secs(30))
            .await
            .expect("audit outbox settles");
        sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), 0) FROM vala.audit_staging WHERE data_tenant_id = $1",
        )
        .bind(self.seed.tenant().as_uuid())
        .fetch_one(&self.assertion)
        .await
        .expect("audit floor reads")
    }
}

/// A spawned runtime and the token that stops it.
struct RunningRuntime {
    /// Cancels the runtime.
    stop: CancellationToken,
    /// The supervised runtime task.
    task: JoinHandle<()>,
}

impl RunningRuntime {
    /// Spawn `runtime` on the test's Tokio runtime.
    fn spawn(runtime: VerificationRuntime) -> Self {
        let stop = CancellationToken::new();
        let task = tokio::spawn(runtime.run(stop.clone()));
        Self { stop, task }
    }

    /// Cancel the runtime and wait for its drain to finish.
    ///
    /// # Panics
    /// Panics when the runtime panics or does not return within [`WAIT`].
    async fn stop(self) {
        self.stop.cancel();
        tokio::time::timeout(WAIT, self.task)
            .await
            .expect("the runtime drains within the wait")
            .expect("the runtime task does not panic");
    }
}

/// Provision `tenant` and register one subject Service and one Drift Verifier.
///
/// # Panics
/// Panics when any seed write fails.
async fn seed_tenant(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> (VerificationFixture, CardUid, CardUid) {
    let seed = VerificationFixture::provision(server.state().postgres.wyrd(), tenant)
        .await
        .expect("tenant provisions");
    let (subject, _) = seed.service("svc").await.expect("subject registers");
    let verifier = seed
        .drift_verifier("drift")
        .await
        .expect("verifier registers");
    (seed, subject, verifier)
}

/// Poll `run` in `seed` until `done` holds.
///
/// # Panics
/// Panics when the run cannot be read or `done` never holds within [`WAIT`].
async fn wait_run_in(
    seed: &VerificationFixture,
    run: VerificationRunId,
    done: impl Fn(&RunRow) -> bool,
) -> RunRow {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let row = seed.run(run).await.expect("run reads");
        if done(&row) {
            return row;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run never reached the expected state; last {row:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Poll `ready` until it holds.
///
/// # Panics
/// Panics when `ready` never holds within [`WAIT`].
async fn wait_until(what: &str, ready: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !ready() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// A Drift report over two features, one drifting, so the verdict fails.
///
/// # Panics
/// Panics when a static feature name is invalid.
fn drifting_report() -> VerifierReport {
    let features = [
        ("latency", 3.0, DriftVerdict::Drift),
        ("tokens", 0.1, DriftVerdict::NoDrift),
    ]
    .into_iter()
    .map(|(name, score, verdict)| {
        let feature = FeatureName::new(name).expect("feature name");
        (
            feature.clone(),
            FeatureDriftReport {
                feature,
                score,
                threshold: 1.0,
                verdict,
                evidence: None,
            },
        )
    })
    .collect::<BTreeMap<_, _>>();
    VerifierReport::Drift(Some(DriftReport {
        method: DriftMethod::Custom,
        features,
        verdict: DriftVerdict::Drift,
    }))
}

/// Whether `row` has reached `status`.
fn status(status: &'static str) -> impl Fn(&RunRow) -> bool {
    move |row| row.status == status
}

/// The sealed batches `fault` recorded for `table`, in send order.
fn sent_to(fault: &PublicationFault, table: &str) -> Vec<SentBatch> {
    fault
        .sent()
        .into_iter()
        .filter(|sent| sent.table == table)
        .collect()
}

/// A completed Drift run writes its feature details and then its summary
/// through the internal result writer, completes with a result, stages no
/// audit, and records the runtime metrics.
///
/// # Panics
/// Panics when the run does not complete, the writes differ in order, kind,
/// or count, any other audit row is staged, or a metric is missing.
#[tokio::test]
async fn completed_run_publishes_details_then_summary_and_records_metrics() {
    let metrics = wyrd_server::app::metrics::install_recorder().expect("recorder installs");
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(drifting_report()));
    let runtime = harness.spawn(Harness::limits(), &script);
    let run = harness.enqueue().await;

    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 1);
    assert!(
        row.result_id.is_some(),
        "a completed run points at its result"
    );
    assert_eq!(row.error_code, None);
    assert_eq!(
        harness.writes(),
        vec![FEATURES.to_owned(), RESULTS.to_owned()]
    );
    assert_eq!(
        harness.audit_operations().await,
        Vec::<String>::new(),
        "claims, settlements, and internal result writes evaluate no permission and stage no audit"
    );
    runtime.stop().await;

    let rendered = metrics.render();
    for name in [
        "wyrd_verification_queue_depth",
        "wyrd_verification_active_runs",
        "wyrd_verification_run_attempts_total",
        "wyrd_verification_run_duration_seconds",
        "wyrd_verification_capability_up",
    ] {
        assert!(rendered.contains(name), "{name} is exported:\n{rendered}");
    }
    assert!(
        rendered
            .contains(r#"wyrd_verification_run_attempts_total{kind="unknown",mode="queued"} 1"#),
        "{rendered}"
    );
}

/// Every attempt records its PostgreSQL-measured queue wait and its owned
/// phase durations, every terminal settlement records trigger-to-terminal
/// latency once, and each attempt is one correlated `verification.attempt`
/// trace spanning claim, load, evidence read, engine, publication, and
/// settlement with only bounded, scrubbed attributes.
///
/// Run one retries and then completes, run two is cancelled by the engine,
/// and run three executes the real Drift engine, which reads its evidence
/// through Bifrost.
///
/// # Panics
/// Panics when a series count differs from the attempts that produced it,
/// a retry records a terminal latency, an attempt trace lacks a phase span,
/// or any span attribute carries the tenant ID.
#[tokio::test]
async fn attempts_record_queue_wait_phases_terminal_latency_and_one_trace() {
    let (telemetry, traces) =
        wyrd_server::install_capture_runtime(wyrd_telemetry::TelemetryConfig::default())
            .expect("production-shaped telemetry installs");
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Retry(VerificationError {
        code: "engine_unavailable".to_owned(),
        message: "the engine did not respond".to_owned(),
    }));
    script.push(EngineOutcome::Completed(drifting_report()));
    script.push(EngineOutcome::Terminal(
        TerminalStatus::Cancelled,
        VerificationError {
            code: "cancelled".to_owned(),
            message: "the engine was cancelled".to_owned(),
        },
    ));
    let runtime = harness.spawn(Harness::limits(), &script);

    let retried = harness.enqueue().await;
    harness.wait_run(retried, status("retrying")).await;
    harness.expire(retried).await;
    harness.wait_run(retried, status("completed")).await;
    let cancelled = harness.enqueue().await;
    harness.wait_run(cancelled, status("cancelled")).await;
    let profiled = harness
        .seed
        .custom_drift_verifier("telemetry-drift", "score", 1.0, 0.5)
        .await
        .expect("profiled verifier registers");
    let now = Utc::now();
    let real = harness
        .seed
        .enqueue_direct(
            &profiled,
            &harness.subject,
            DriftWindow {
                start: now - chrono::Duration::hours(1),
                end: now,
            },
        )
        .await
        .expect("run enqueues");
    let real_row = harness
        .wait_run(real, |row| {
            !matches!(row.status.as_str(), "pending" | "running" | "retrying")
        })
        .await;
    runtime.stop().await;

    let rendered = telemetry.prometheus().render();
    let count = |series: &str| -> u64 {
        rendered
            .lines()
            .find_map(|line| line.strip_prefix(series)?.strip_prefix(' '))
            .map_or(0, |value| value.parse().expect("a count is an integer"))
    };
    // The scripted attempts execute the fixture's profile-less Drift
    // Verifier, which cannot be classified; the real run is a Custom Drift.
    let real_published = u64::from(real_row.result_id.is_some());
    for (kind, attempts, published, completed) in [
        ("unknown", 3, 1, 1),
        ("drift_custom", 1, real_published, real_published),
    ] {
        let manual = format!(r#"kind="{kind}",origin="manual""#);
        let queued = format!(r#"kind="{kind}",mode="queued""#);
        assert_eq!(
            count(&format!(
                "wyrd_verification_queue_wait_seconds_count{{{manual}}}"
            )),
            attempts,
            "one queue wait per {kind} attempt:\n{rendered}"
        );
        assert_eq!(
            count(&format!("wyrd_verification_run_attempts_total{{{queued}}}")),
            attempts,
            "one {kind} attempt per claim:\n{rendered}"
        );
        for phase in ["load", "engine", "settlement"] {
            assert_eq!(
                count(&format!(
                    r#"wyrd_verification_phase_duration_seconds_count{{{queued},phase="{phase}"}}"#
                )),
                attempts,
                "{kind} {phase} is timed once per attempt:\n{rendered}"
            );
        }
        for phase in ["input_read", "prepare"] {
            assert_eq!(
                count(&format!(
                    r#"wyrd_verification_phase_duration_seconds_count{{{queued},phase="{phase}"}}"#
                )),
                u64::from(kind == "drift_custom"),
                "only the real engine reads evidence and prepares:\n{rendered}"
            );
        }
        assert_eq!(
            count(&format!(
                r#"wyrd_verification_phase_duration_seconds_count{{{queued},phase="publication"}}"#
            )),
            published,
            "only completed {kind} reports publish:\n{rendered}"
        );
        let overhead: u64 = ["completed", "retrying", "cancelled"]
            .iter()
            .map(|outcome| {
                count(&format!(
                    r#"wyrd_verification_engine_overhead_seconds_count{{{queued},outcome="{outcome}"}}"#
                ))
            })
            .sum();
        assert_eq!(
            overhead, attempts,
            "every {kind} engine execution reports its overhead once:\n{rendered}"
        );
        assert!(
            rendered
                .lines()
                .any(|line| line == format!("wyrd_verification_active_runs{{{queued}}} 0")),
            "the {kind} active gauge returns to zero:\n{rendered}"
        );
        assert_eq!(
            count(&format!(
                r#"wyrd_verification_trigger_to_terminal_seconds_count{{{manual},outcome="completed"}}"#
            )),
            completed,
            "{rendered}"
        );
    }
    let unknown = r#"kind="unknown",mode="queued""#;
    for outcome in ["retrying", "cancelled"] {
        assert_eq!(
            count(&format!(
                r#"wyrd_verification_run_failures_total{{{unknown},outcome="{outcome}"}}"#
            )),
            1,
            "{outcome} is an unsuccessful execution outcome:\n{rendered}"
        );
    }
    let terminal = |outcome: &str| {
        count(&format!(
            r#"wyrd_verification_trigger_to_terminal_seconds_count{{kind="unknown",origin="manual",outcome="{outcome}"}}"#
        ))
    };
    assert_eq!(terminal("cancelled"), 1, "{rendered}");
    assert_eq!(
        terminal("retrying"),
        0,
        "a retry is not terminal:\n{rendered}"
    );
    assert!(
        !rendered.contains("implementation="),
        "the pooled implementation label is gone:\n{rendered}"
    );
    assert!(
        rendered
            .lines()
            .filter(|line| line.starts_with("wyrd_verification_"))
            .all(|line| !line.contains("tenant")),
        "no verification series carries a tenant label:\n{rendered}"
    );

    let spans = traces.finished_since(0);
    let attempt = |run: VerificationRunId, attempt: &str| {
        spans
            .iter()
            .find(|span| {
                span.name == "verification.attempt"
                    && span.attributes.get("run_id") == Some(&run.to_string())
                    && span.attributes.get("attempt").map(String::as_str) == Some(attempt)
            })
            .unwrap_or_else(|| panic!("attempt {attempt} of {run} is traced: {spans:#?}"))
    };
    let children = |root: &wyrd_telemetry::CapturedSpan| -> Vec<&str> {
        spans
            .iter()
            .filter(|span| span.trace_id == root.trace_id && span.span_id != root.span_id)
            .map(|span| span.name.as_str())
            .collect()
    };

    let first = attempt(retried, "1");
    assert_eq!(
        first.attributes.get("outcome").map(String::as_str),
        Some("retrying")
    );
    assert_eq!(
        first.attributes.get("error_code").map(String::as_str),
        Some("engine_unavailable")
    );
    assert!(matches!(
        first.status,
        wyrd_telemetry::CapturedSpanStatus::Error(_)
    ));
    let completed = attempt(retried, "2");
    assert_ne!(
        first.trace_id, completed.trace_id,
        "each attempt is its own trace"
    );
    assert_eq!(
        completed.attributes.get("outcome").map(String::as_str),
        Some("completed")
    );
    assert_eq!(
        completed.attributes.get("origin").map(String::as_str),
        Some("manual")
    );
    assert_eq!(
        completed.attributes.get("kind").map(String::as_str),
        Some("unknown")
    );
    assert_eq!(
        completed.attributes.get("mode").map(String::as_str),
        Some("queued")
    );
    assert!(
        completed.attributes.contains_key("task_start_delay_us"),
        "the queued attempt records its spawn-to-first-execution delay: {completed:?}"
    );
    assert!(!matches!(
        completed.status,
        wyrd_telemetry::CapturedSpanStatus::Error(_)
    ));
    let names = children(completed);
    for phase in [
        "claim",
        "verification.load",
        "verification.engine",
        "verification.publish",
        "verification.settle",
        "complete",
    ] {
        assert!(
            names.contains(&phase),
            "{phase} joins the attempt trace: {names:?}"
        );
    }
    let cancel = attempt(cancelled, "1");
    assert_eq!(
        cancel.attributes.get("outcome").map(String::as_str),
        Some("cancelled")
    );
    assert_eq!(
        cancel.attributes.get("error_code").map(String::as_str),
        Some("cancelled")
    );
    assert!(matches!(
        cancel.status,
        wyrd_telemetry::CapturedSpanStatus::Error(_)
    ));
    let real_attempt = attempt(real, "1");
    assert_eq!(
        real_attempt.attributes.get("kind").map(String::as_str),
        Some("drift_custom")
    );
    let real_children = children(real_attempt);
    for child in ["verification.evidence_read", "verification.prepare"] {
        assert!(
            real_children.contains(&child),
            "the real engine's {child} joins its attempt trace: {real_children:?}"
        );
    }

    let tenant = harness.server.pg_fixture().data_tenant_id().to_string();
    for span in spans
        .iter()
        .filter(|span| span.name.starts_with("verification."))
    {
        for value in span.attributes.values() {
            assert!(
                !value.contains(&tenant),
                "{} leaks the tenant: {span:?}",
                span.name
            );
        }
    }
}

/// An unscored Drift execution publishes only its summary.
///
/// # Panics
/// Panics when the run does not complete or anything but one summary is written.
#[tokio::test]
async fn unscored_drift_publishes_only_the_summary() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let runtime = harness.spawn(Harness::limits(), &script);
    let run = harness.enqueue().await;

    harness.wait_run(run, status("completed")).await;
    assert_eq!(harness.writes(), vec![RESULTS.to_owned()]);
    runtime.stop().await;
}

/// Without a scripted outcome the real Drift engine refuses the fixture
/// Verifier, which declares no profile, settling the run `errored` with
/// `drift_invalid` and publishing nothing.
///
/// # Panics
/// Panics when the run is not errored with that code or anything is written.
#[tokio::test]
async fn unscorable_verifier_errors_without_publishing() {
    let harness = Harness::start().await;
    let runtime = harness.spawn(Harness::limits(), &EngineScript::default());
    let run = harness.enqueue().await;

    let row = harness.wait_run(run, status("errored")).await;
    assert_eq!(row.error_code.as_deref(), Some(DRIFT_INVALID));
    assert_eq!(row.result_id, None, "no verdict is fabricated");
    assert!(harness.writes().is_empty());
    runtime.stop().await;
}

/// A result batch Scribe durably accepted but whose acknowledgement was lost
/// is resent inside the same publication attempt as the identical sealed
/// batch — same table, batch ID, and Arrow bytes — and Scribe acknowledges the
/// replay without a second row. The run completes on its first attempt.
///
/// # Panics
/// Panics when the run retries or fails, the replay differs from the original
/// sealed batch, or Scribe keeps a duplicate summary row.
#[tokio::test]
async fn lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(drifting_report()));
    let fault = &harness.fault;
    fault.lose_ack_next(RESULTS);
    let runtime = RunningRuntime::spawn(harness.runtime(
        Harness::limits(),
        &script,
        &CapabilityCrash::default(),
    ));
    let run = harness.enqueue().await;

    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 1, "the replay settled inside one attempt");
    assert!(row.result_id.is_some());
    let summaries = sent_to(fault, RESULTS);
    assert_eq!(
        summaries.len(),
        2,
        "the unacknowledged summary was resent once"
    );
    assert_eq!(
        summaries[0], summaries[1],
        "the replay carries the identical table, batch ID, and sealed bytes"
    );
    let details = sent_to(fault, FEATURES);
    assert_eq!(details.len(), 1);
    assert_ne!(details[0].batch_id, summaries[0].batch_id);
    assert_eq!(
        harness.writes(),
        vec![FEATURES.to_owned(), RESULTS.to_owned(), RESULTS.to_owned()],
        "each submission, replay included, is recorded"
    );
    runtime.stop().await;
    let rows = harness.durable_rows().await;
    assert_eq!(
        rows.get(RESULTS),
        Some(&1),
        "Scribe deduplicated the replayed summary: {rows:?}"
    );
    assert_eq!(rows.get(FEATURES), Some(&2), "{rows:?}");
}

/// Retryable engine failures back off and exhaust the attempt budget into
/// `errored` carrying the engine's code.
///
/// # Panics
/// Panics when a retry is not scheduled or the final state differs.
#[tokio::test]
async fn retryable_engine_failures_exhaust_to_errored() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    for _ in 0..3 {
        script.push(EngineOutcome::Retry(VerificationError {
            code: "engine_unavailable".to_owned(),
            message: "the engine did not respond".to_owned(),
        }));
    }
    let runtime = harness.spawn(Harness::limits(), &script);
    let run = harness.enqueue().await;

    for attempt in 1..=2 {
        let row = harness
            .wait_run(run, move |row| {
                row.status == "retrying" && row.attempts == attempt
            })
            .await;
        assert_eq!(row.error_code.as_deref(), Some("engine_unavailable"));
        harness.expire(run).await;
    }
    let row = harness.wait_run(run, status("errored")).await;
    assert_eq!(row.attempts, 3);
    assert_eq!(row.error_code.as_deref(), Some("engine_unavailable"));
    assert!(harness.writes().is_empty());
    runtime.stop().await;
}

/// Input-admission deferrals are backpressure, not failed attempts: a run
/// deferred more times than its attempt budget still completes on its first
/// charged attempt.
///
/// # Panics
/// Panics when the run does not complete or a deferral was charged.
#[tokio::test]
async fn deferred_engine_admission_requeues_without_spending_attempts() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    for _ in 0..4 {
        script.push(EngineOutcome::Deferred(VerificationError {
            code: "input_admission_refused".to_owned(),
            message: "the input read was refused at admission".to_owned(),
        }));
    }
    script.push(EngineOutcome::Completed(drifting_report()));
    let runtime = harness.spawn(Harness::limits(), &script);
    let run = harness.enqueue().await;

    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 1, "no deferral is charged an attempt");
    assert_eq!(row.error_code, None);
    runtime.stop().await;
}

/// An engine cancellation settles `cancelled`, and an execution past its
/// deadline settles `timed_out`; neither carries a verdict.
///
/// # Panics
/// Panics when either run reaches another state or a result is written.
#[tokio::test]
async fn cancellation_and_deadline_settle_without_a_verdict() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Terminal(
        TerminalStatus::Cancelled,
        VerificationError {
            code: "cancelled".to_owned(),
            message: "the engine was cancelled".to_owned(),
        },
    ));
    let runtime = harness.spawn(
        RuntimeLimits {
            execution_timeout: Duration::from_millis(500),
            ..Harness::limits()
        },
        &script,
    );
    let cancelled = harness.enqueue().await;
    let row = harness.wait_run(cancelled, status("cancelled")).await;
    assert_eq!(row.result_id, None);

    script.hold();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let slow = harness.enqueue().await;
    let row = harness.wait_run(slow, status("timed_out")).await;
    assert_eq!(row.error_code.as_deref(), Some("execution_timed_out"));
    assert_eq!(row.result_id, None);
    script.release();
    assert!(harness.writes().is_empty());
    runtime.stop().await;
}

/// Verifier claims exceed the former process and tenant ceilings while held,
/// and another tenant's work also starts before any execution is released.
///
/// # Panics
/// Panics when a former ceiling still blocks work or a run fails to complete.
#[tokio::test]
async fn verifier_runs_execute_beyond_the_former_permit_ceilings() {
    let harness = Harness::start().await;
    let other_tenant = DataTenantId::new_v7();
    harness
        .server
        .pg_fixture()
        .seed_additional_tenant_with_uuid(other_tenant, "verification-other")
        .await
        .expect("second tenant seeds");
    let (other, other_subject, other_verifier) = seed_tenant(&harness.server, other_tenant).await;
    let script = EngineScript::default();
    script.hold();
    for _ in 0..22 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    }
    let mut busy = Vec::new();
    for _ in 0..20 {
        busy.push(harness.enqueue().await);
    }
    let now = Utc::now();
    let mut quiet = Vec::new();
    for _ in 0..2 {
        quiet.push(
            other
                .enqueue_direct(
                    &other_verifier,
                    &other_subject,
                    DriftWindow {
                        start: now - chrono::Duration::hours(1),
                        end: now,
                    },
                )
                .await
                .expect("second tenant run enqueues"),
        );
    }
    let runtime = harness.spawn(Harness::limits(), &script);
    wait_until("all 22 executions before release", || {
        script.entered() == 22
    })
    .await;
    for run in &busy {
        assert_eq!(
            harness.seed.run(*run).await.expect("run reads").status,
            "running"
        );
    }
    for run in &quiet {
        assert_eq!(other.run(*run).await.expect("run reads").status, "running");
    }
    script.release();
    for run in busy {
        harness.wait_run(run, status("completed")).await;
    }
    for run in quiet {
        wait_run_in(&other, run, status("completed")).await;
    }
    runtime.stop().await;
}

/// Insert a pending baseline fit of `verifier` from `data` in `seed`'s tenant.
///
/// # Panics
/// Panics when the insert or its commit fails.
async fn pending_baseline(
    server: &WyrdTestServer,
    seed: &VerificationFixture,
    verifier: &CardUid,
    data: &CardUid,
) {
    let mut conn = server
        .state()
        .postgres
        .wyrd()
        .tenant_conn(seed.tenant())
        .await
        .expect("tenant connection opens");
    DriftBaselineQueue::default()
        .insert_pending(&mut conn, verifier, data)
        .await
        .expect("baseline inserts");
    conn.commit().await.expect("baseline commits");
}

/// Read the baseline state of `verifier` in `seed`'s tenant.
///
/// # Panics
/// Panics when the read fails or the baseline does not exist.
async fn baseline_state(
    server: &WyrdTestServer,
    seed: &VerificationFixture,
    verifier: &CardUid,
) -> DriftBaselineState {
    let mut conn = server
        .state()
        .postgres
        .wyrd()
        .tenant_conn(seed.tenant())
        .await
        .expect("tenant connection opens");
    DriftBaselineQueue::default()
        .status(&mut conn, verifier)
        .await
        .expect("baseline reads")
        .expect("baseline exists")
        .state
}

/// Baseline fits proceed for both tenants while Verifier executions are held.
/// Each fit fails because the fixture's Data Card has no Parquet artifact;
/// leaving `pending` proves it was claimed without waiting for Verifier slots.
///
/// # Panics
/// Panics when held Verifier runs prevent a baseline claim.
#[tokio::test]
async fn baseline_fits_do_not_wait_for_verifier_executions() {
    let harness = Harness::start().await;
    let other_tenant = DataTenantId::new_v7();
    harness
        .server
        .pg_fixture()
        .seed_additional_tenant_with_uuid(other_tenant, "verification-fit-other")
        .await
        .expect("second tenant seeds");
    let (other, other_subject, other_verifier) = seed_tenant(&harness.server, other_tenant).await;
    let script = EngineScript::default();
    script.hold();
    let mut busy = Vec::new();
    for _ in 0..4 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
        busy.push(harness.enqueue().await);
    }
    let runtime = harness.spawn(Harness::limits(), &script);
    wait_until("four executions", || script.entered() == 4).await;
    pending_baseline(
        &harness.server,
        &harness.seed,
        &harness.verifier,
        &harness.subject,
    )
    .await;
    pending_baseline(&harness.server, &other, &other_verifier, &other_subject).await;
    for (seed, verifier) in [
        (&harness.seed, &harness.verifier),
        (&other, &other_verifier),
    ] {
        let deadline = tokio::time::Instant::now() + WAIT;
        let state = loop {
            let state = baseline_state(&harness.server, seed, verifier).await;
            if !matches!(
                state,
                DriftBaselineState::Pending | DriftBaselineState::Building
            ) {
                break state;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "held Verifier runs blocked a baseline fit"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert_eq!(state, DriftBaselineState::Failed);
    }
    script.release();
    for run in busy {
        harness.wait_run(run, status("completed")).await;
    }
    runtime.stop().await;
}

/// An expired lease is reclaimed by another runner that completes the run;
/// the original holder, released afterwards, cannot store a result, so its
/// work never reaches Bifrost and the run is unchanged.
///
/// # Panics
/// Panics when the run is not reclaimed, the stale holder writes a batch or
/// changes the run, or the attempts differ.
#[tokio::test]
async fn expired_lease_is_reclaimed_and_the_stale_holder_is_fenced() {
    let harness = Harness::start().await;
    let stale_script = EngineScript::default();
    stale_script.hold();
    stale_script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let stale = harness.spawn(Harness::limits(), &stale_script);
    let run = harness.enqueue().await;
    wait_until("the first claim", || stale_script.entered() == 1).await;

    // Close the stale runner's claim loop while its held attempt drains, so
    // only the fresh runner can reclaim the deliberately expired lease.
    stale.stop.cancel();
    harness.expire(run).await;
    let fresh_script = EngineScript::default();
    fresh_script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let fresh = harness.spawn(Harness::limits(), &fresh_script);
    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 2, "the reclaim is the second attempt");

    stale_script.release();
    stale.stop().await;
    assert_eq!(
        harness.writes(),
        vec![RESULTS.to_owned()],
        "only the fresh claimant's result reaches Bifrost"
    );
    assert_eq!(
        harness.seed.run(run).await.expect("run reads"),
        row,
        "the stale holder changes nothing"
    );
    fresh.stop().await;
}

/// A crashed runner degrades health, is restarted, and — once the lost lease
/// expires — reclaims and completes the run exactly once. The exit is visible
/// through health, one structured `verification capability crashed` error
/// event naming the runner, and the runtime metrics: one runner restart and
/// the runner's `capability_up` gauge back at one.
///
/// # Panics
/// Panics when health does not degrade and recover, the run is not
/// completed on its second attempt, more than one summary is written, or the
/// crash is missing from the captured trace or the rendered metrics.
#[tokio::test]
async fn crashed_runner_restarts_and_reclaims_without_duplicates() {
    let metrics = wyrd_server::app::metrics::install_recorder().expect("recorder installs");
    let logs = LogCapture::install();
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let crash = CapabilityCrash::default();
    let runtime = RunningRuntime::spawn(harness.runtime(Harness::limits(), &script, &crash));
    let health = std::sync::Arc::clone(&harness.server.state().verification);
    wait_until("runtime health", || {
        health.is_composed() && !health.is_degraded()
    })
    .await;
    let run = harness.enqueue().await;
    wait_until("the first claim", || script.entered() == 1).await;

    crash.crash_next(RuntimeCapability::Runner);
    wait_until("runner down", || !health.is_up(RuntimeCapability::Runner)).await;
    assert!(health.is_degraded(), "a required capability is absent");
    wait_until("runner restarted", || !health.is_degraded()).await;
    script.release();
    assert_eq!(
        harness.seed.run(run).await.expect("run reads").status,
        "running"
    );

    harness.expire(run).await;
    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 2);
    assert_eq!(harness.writes(), vec![RESULTS.to_owned()]);

    let rendered = metrics.render();
    assert!(
        rendered.contains(r#"wyrd_verification_capability_restarts_total{capability="runner"} 1"#),
        "one runner restart is counted:\n{rendered}"
    );
    assert!(
        rendered.contains(r#"wyrd_verification_capability_up{capability="runner"} 1"#),
        "the restarted runner reports up:\n{rendered}"
    );
    let crashes: Vec<String> = logs
        .text()
        .lines()
        .filter(|line| line.contains("verification capability crashed"))
        .map(str::to_owned)
        .collect();
    assert_eq!(crashes.len(), 1, "one crash event: {crashes:?}");
    assert!(
        crashes[0].contains("ERROR") && crashes[0].contains(r#"capability="runner""#),
        "the crash event is an error naming the runner: {}",
        crashes[0]
    );
    runtime.stop().await;
}

/// Shutdown stops claiming the moment it begins, drains for at most the
/// grace, and leaves unfinished work recoverable under its own identity.
///
/// One run is executing when shutdown begins and a second is enqueued right
/// after: the second is never claimed, the first is released with its attempt
/// refunded once the grace elapses, and the runtime returns within the grace
/// plus settlement slack. A fresh runtime then claims both under their
/// original run IDs and completes each on one charged attempt.
///
/// # Panics
/// Panics when the late run is claimed during shutdown, the drain outlives its
/// bound, a run is not released or not recovered under its own ID, or the
/// recovered attempts differ.
#[tokio::test]
async fn shutdown_stops_claims_drains_bounded_and_restart_recovers_identity() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    for _ in 0..3 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    }
    let grace = Duration::from_secs(1);
    let runtime = harness.spawn(
        RuntimeLimits {
            drain_grace: grace,
            ..Harness::limits()
        },
        &script,
    );
    let inflight = harness.enqueue().await;
    wait_until("the in-flight claim", || script.entered() == 1).await;

    let started = tokio::time::Instant::now();
    let stopping = tokio::spawn(runtime.stop());
    let late = harness.enqueue().await;
    stopping.await.expect("the runtime stops");
    let drained = started.elapsed();
    assert!(
        drained >= grace && drained < grace + Duration::from_secs(5),
        "shutdown waits out the grace and no longer: {drained:?}"
    );
    assert_eq!(script.entered(), 1, "no execution starts after shutdown");
    for run in [inflight, late] {
        let row = harness.seed.run(run).await.expect("run reads");
        assert_eq!(
            (row.status.as_str(), row.attempts),
            ("pending", 0),
            "{run} is unclaimed or released with its attempt refunded"
        );
        assert_eq!(
            harness.lease(run).await.1,
            None,
            "{run} holds no live lease"
        );
    }
    assert!(harness.writes().is_empty());

    script.release();
    let restarted = harness.spawn(Harness::limits(), &script);
    for run in [inflight, late] {
        let row = harness.wait_run(run, status("completed")).await;
        assert_eq!(row.attempts, 1, "{run} completes on one charged attempt");
        assert!(row.result_id.is_some());
    }
    assert_eq!(
        harness.seed.runs().await.expect("runs read").len(),
        2,
        "recovery reuses the durable identities rather than enqueueing anew"
    );
    restarted.stop().await;
}

/// Shutdown stops claiming and, once the drain grace elapses, releases a run
/// still executing with its attempt refunded.
///
/// # Panics
/// Panics when the runtime does not stop or the run is not released.
#[tokio::test]
async fn shutdown_releases_runs_still_in_flight_after_the_grace() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let runtime = harness.spawn(
        RuntimeLimits {
            drain_grace: Duration::from_millis(300),
            ..Harness::limits()
        },
        &script,
    );
    let run = harness.enqueue().await;
    wait_until("the claim", || script.entered() == 1).await;

    runtime.stop().await;
    let row = harness.seed.run(run).await.expect("run reads");
    assert_eq!((row.status.as_str(), row.attempts), ("pending", 0));
    assert!(
        !harness
            .server
            .state()
            .verification
            .is_up(RuntimeCapability::Runner)
    );
    assert!(harness.writes().is_empty());
}

/// Shutdown waits for a run that finishes within the drain grace and lets it
/// publish and complete.
///
/// # Panics
/// Panics when the run does not complete before the runtime stops.
#[tokio::test]
async fn shutdown_drains_runs_that_finish_within_the_grace() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let runtime = harness.spawn(Harness::limits(), &script);
    let run = harness.enqueue().await;
    wait_until("the claim", || script.entered() == 1).await;

    let stopping = tokio::spawn(runtime.stop());
    tokio::time::sleep(Duration::from_millis(200)).await;
    script.release();
    stopping.await.expect("the runtime stops");
    let row = harness.seed.run(run).await.expect("run reads");
    assert_eq!(row.status, "completed");
    assert_eq!(harness.writes(), vec![RESULTS.to_owned()]);
}

/// Concurrent runtimes ticking the same due occurrence, and a runtime
/// restarted after them, create exactly one run, and health is not degraded.
///
/// # Panics
/// Panics when a duplicate run is created, the scheduler is not composed, or
/// health degrades.
#[tokio::test]
async fn schedulers_create_one_run_per_occurrence_across_ticks_and_restart() {
    let harness = Harness::start().await;
    let (owner, principal) = harness
        .seed
        .service("owner")
        .await
        .expect("owner registers");
    let binding = harness
        .seed
        .bind_schedule(&owner, &owner, &harness.verifier, "0 2 * * *", Vec::new())
        .await
        .expect("binding projects");
    harness
        .seed
        .activate(principal)
        .await
        .expect("owner activates");
    harness.make_due(binding).await;
    let runtime = || {
        VerificationRuntime::builder(harness.server.state())
            .limits(Harness::limits())
            .build()
            .expect("the runtime composes")
    };
    let first = runtime();
    assert!(first.composes(RuntimeCapability::Scheduler));
    let first = RunningRuntime::spawn(first);
    let second = RunningRuntime::spawn(runtime());
    let deadline = tokio::time::Instant::now() + WAIT;
    while harness.seed.runs().await.expect("runs read").is_empty() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the occurrence was never scheduled"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!harness.server.state().verification.is_degraded());
    first.stop().await;
    second.stop().await;

    let restarted = RunningRuntime::spawn(runtime());
    tokio::time::sleep(Duration::from_millis(300)).await;
    restarted.stop().await;
    let runs = harness.seed.runs().await.expect("runs read");
    assert_eq!(runs.len(), 1, "one run per occurrence");
}

/// The production bound server composes the runtime when enabled, reports it
/// ready, and still shuts down cleanly; an ordinary test server composes none.
///
/// # Panics
/// Panics when composition, readiness, or shutdown differ.
#[tokio::test]
async fn bound_server_composes_the_runtime_and_reports_it_ready() {
    let plain = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("plain server starts");
    assert!(!plain.state().verification.is_composed());
    plain.shutdown().await.expect("plain server shuts down");

    let server = WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .start_bound()
        .await
        .expect("server starts");
    let health = std::sync::Arc::clone(&server.state().verification);
    wait_until("runtime up", || {
        health.is_up(RuntimeCapability::Scheduler) && health.is_up(RuntimeCapability::Runner)
    })
    .await;
    let readiness = std::sync::Arc::clone(&server.state().readiness);
    wait_until("verification readiness", || {
        readiness
            .load()
            .verification
            .as_ref()
            .is_some_and(|probe| probe.ok)
    })
    .await;
    server.shutdown().await.expect("server shuts down cleanly");
    assert!(!health.is_up(RuntimeCapability::Runner));
}

/// A scheduler cancelled while its occurrence transaction is parked holding
/// the due binding admits nothing: the uncommitted enqueue and cursor advance
/// roll back together once the lock is released, and no run exists.
///
/// # Panics
/// Panics when the scheduler never blocks, does not stop while blocked, or a
/// run or cursor advance commits after cancellation.
#[tokio::test]
async fn cancelled_scheduler_rolls_back_its_blocked_occurrence() {
    let harness = Harness::start().await;
    let (owner, principal) = harness
        .seed
        .service("owner")
        .await
        .expect("owner registers");
    let binding = harness
        .seed
        .bind_schedule(&owner, &owner, &harness.verifier, "0 2 * * *", Vec::new())
        .await
        .expect("binding projects");
    harness
        .seed
        .activate(principal)
        .await
        .expect("owner activates");
    harness.make_due(binding).await;
    let armed = harness.cursor(binding).await;
    assert!(armed.is_some(), "activation armed the schedule cursor");

    let blocker = harness.block_writes("wyrd.verifier_runs").await;
    let runtime = RunningRuntime::spawn(
        VerificationRuntime::builder(harness.server.state())
            .limits(Harness::limits())
            .build()
            .expect("the scheduler composes"),
    );
    let pid = harness.wait_blocked_writer("wyrd.verifier_runs").await;
    let holds_binding: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE pid = $1 AND granted \
         AND relation = 'wyrd.verification_bindings'::regclass)",
    )
    .bind(pid)
    .fetch_one(&harness.assertion)
    .await
    .expect("binding locks read");
    assert!(
        holds_binding,
        "the blocked occurrence holds the due binding"
    );

    runtime.stop().await;
    blocker.rollback().await.expect("blocker releases");
    harness.wait_backend_settled(pid).await;

    assert!(
        harness.seed.runs().await.expect("runs read").is_empty(),
        "no run commits after cancellation"
    );
    assert_eq!(
        harness.cursor(binding).await,
        armed,
        "the cursor advance rolled back with the enqueue"
    );
}

/// Advisory lock key the scheduler commit-race test's deferred trigger waits on.
const SCHEDULE_COMMIT_LOCK: i64 = 0x5752_5343;

/// A scheduler cancelled while an occurrence's `COMMIT` is in flight awaits
/// that commit to a known result before closing: exactly one run and its
/// atomic cursor advance are admitted, no later occurrence starts, and a
/// restart preserves that state while scheduling only the untouched binding.
///
/// A test-scoped deferred constraint trigger on run inserts makes the
/// occurrence's `COMMIT` wait on an advisory lock the test holds, so the run
/// insert and cursor advance are applied and the commit selected when `stop`
/// fires.
///
/// # Panics
/// Panics when the commit never blocks, the scheduler closes before its
/// selected commit resolves, a second occurrence starts after cancellation,
/// or restart duplicates or loses the committed occurrence.
#[tokio::test]
async fn scheduler_awaits_its_selected_commit_before_closing() {
    let harness = Harness::start().await;
    let mut bindings = Vec::new();
    for name in ["first", "second"] {
        let (owner, principal) = harness.seed.service(name).await.expect("owner registers");
        bindings.push(
            harness
                .seed
                .bind_schedule(&owner, &owner, &harness.verifier, "0 2 * * *", Vec::new())
                .await
                .expect("binding projects"),
        );
        harness
            .seed
            .activate(principal)
            .await
            .expect("owner activates");
    }
    let mut armed = Vec::new();
    for binding in &bindings {
        harness.make_due(*binding).await;
        armed.push(harness.cursor(*binding).await);
    }
    sqlx::query(
        "CREATE FUNCTION wyrd.test_hold_schedule_commit() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
         PERFORM pg_advisory_xact_lock(TG_ARGV[0]::bigint); RETURN NULL; END $$",
    )
    .execute(&harness.assertion)
    .await
    .expect("hold function creates");
    sqlx::query("GRANT EXECUTE ON FUNCTION wyrd.test_hold_schedule_commit() TO PUBLIC")
        .execute(&harness.assertion)
        .await
        .expect("hold function grants");
    sqlx::query(AssertSqlSafe(format!(
        "CREATE CONSTRAINT TRIGGER test_hold_schedule_commit \
         AFTER INSERT ON wyrd.verifier_runs \
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW \
         EXECUTE FUNCTION wyrd.test_hold_schedule_commit('{SCHEDULE_COMMIT_LOCK}')"
    )))
    .execute(&harness.assertion)
    .await
    .expect("hold trigger creates");
    let mut holder = harness.assertion.acquire().await.expect("holder connects");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCHEDULE_COMMIT_LOCK)
        .execute(&mut *holder)
        .await
        .expect("commit lock holds");

    let scheduler_only = || {
        VerificationRuntime::builder(harness.server.state())
            .limits(Harness::limits())
            .build()
            .expect("the scheduler composes")
    };
    let mut runtime = RunningRuntime::spawn(scheduler_only());
    let pid = harness.wait_advisory_waiter(SCHEDULE_COMMIT_LOCK).await;
    runtime.stop.cancel();
    assert!(
        tokio::time::timeout(Duration::from_millis(300), &mut runtime.task)
            .await
            .is_err(),
        "the scheduler does not close while its selected commit is unresolved"
    );
    assert_eq!(
        harness.wait_advisory_waiter(SCHEDULE_COMMIT_LOCK).await,
        pid,
        "the selected commit is still in flight"
    );
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEDULE_COMMIT_LOCK)
        .execute(&mut *holder)
        .await
        .expect("commit lock releases");
    runtime.stop().await;

    let runs = harness.seed.runs().await.expect("runs read");
    assert_eq!(
        runs.len(),
        1,
        "exactly the selected occurrence was admitted"
    );
    let mut cursors = Vec::new();
    for binding in &bindings {
        cursors.push(harness.cursor(*binding).await);
    }
    let advanced: Vec<usize> = (0..bindings.len())
        .filter(|index| cursors[*index] != armed[*index])
        .collect();
    assert_eq!(
        advanced.len(),
        1,
        "one atomic cursor advance, no later occurrence: {cursors:?} from {armed:?}"
    );

    sqlx::query("DROP TRIGGER test_hold_schedule_commit ON wyrd.verifier_runs")
        .execute(&harness.assertion)
        .await
        .expect("hold trigger drops");
    sqlx::query("DROP FUNCTION wyrd.test_hold_schedule_commit()")
        .execute(&harness.assertion)
        .await
        .expect("hold function drops");
    let restarted = RunningRuntime::spawn(scheduler_only());
    let deadline = tokio::time::Instant::now() + WAIT;
    while harness.seed.runs().await.expect("runs read").len() < 2 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the untouched occurrence was never scheduled"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    restarted.stop().await;
    let after = harness.seed.runs().await.expect("runs read");
    assert_eq!(after.len(), 2, "one run per occurrence across restart");
    assert!(
        after.contains(&runs[0]),
        "the admitted run survives restart"
    );
    let committed = advanced[0];
    assert_eq!(
        harness.cursor(bindings[committed]).await,
        cursors[committed],
        "restart preserves the admitted cursor advance"
    );
}

/// A runner cancelled while its claim transaction is parked admits nothing:
/// the uncommitted claim rolls back once the lock is released, so the run
/// stays pending with no attempt charged and never executes.
///
/// # Panics
/// Panics when the runner never blocks, does not stop while blocked, or the
/// run is leased, charged, executed, or published after cancellation.
#[tokio::test]
async fn cancelled_runner_rolls_back_its_blocked_claim() {
    let harness = Harness::start().await;
    let run = harness.enqueue().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));

    let blocker = harness.block_writes("wyrd.verifier_runs").await;
    let runtime = harness.spawn(Harness::limits(), &script);
    let pid = harness.wait_blocked_writer("wyrd.verifier_runs").await;

    runtime.stop().await;
    blocker.rollback().await.expect("blocker releases");
    harness.wait_backend_settled(pid).await;

    let row = harness.seed.run(run).await.expect("run reads");
    assert_eq!(
        (row.status.as_str(), row.attempts),
        ("pending", 0),
        "the claim rolled back: no lease and no charged attempt"
    );
    assert_eq!(script.entered(), 0, "nothing executed");
    assert!(harness.writes().is_empty(), "nothing published");
}

/// Advisory lock key the commit-race test's deferred trigger waits on.
const CLAIM_COMMIT_LOCK: i64 = 0x5752_4144;

/// A claim whose commit completes after shutdown began is released through
/// the fenced release transition with its attempt refunded, and never
/// executes or publishes.
///
/// A test-scoped deferred constraint trigger on the run makes the claim's
/// `COMMIT` wait on an advisory lock the test holds, so the claim is already
/// applied and its commit in flight when `stop` fires; releasing the lock lets
/// the commit win the race.
///
/// # Panics
/// Panics when the commit never blocks, the committed claim is not released
/// with its attempt refunded, or the run executes or publishes.
#[tokio::test]
async fn claim_committed_after_cancellation_is_released_unexecuted() {
    let harness = Harness::start().await;
    let run = harness.enqueue().await;
    assert_eq!(harness.lease(run).await, (None, None), "never claimed");
    sqlx::query(
        "CREATE FUNCTION wyrd.test_hold_claim_commit() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
         PERFORM pg_advisory_xact_lock(TG_ARGV[0]::bigint); RETURN NULL; END $$",
    )
    .execute(&harness.assertion)
    .await
    .expect("hold function creates");
    sqlx::query("GRANT EXECUTE ON FUNCTION wyrd.test_hold_claim_commit() TO PUBLIC")
        .execute(&harness.assertion)
        .await
        .expect("hold function grants");
    // The run ID is a typed UUID the test minted, not caller input.
    sqlx::query(AssertSqlSafe(format!(
        "CREATE CONSTRAINT TRIGGER test_hold_claim_commit \
         AFTER UPDATE ON wyrd.verifier_runs \
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW \
         WHEN (NEW.run_id = '{run}'::uuid AND NEW.status = 'running') \
         EXECUTE FUNCTION wyrd.test_hold_claim_commit('{CLAIM_COMMIT_LOCK}')"
    )))
    .execute(&harness.assertion)
    .await
    .expect("hold trigger creates");
    let mut holder = harness.assertion.acquire().await.expect("holder connects");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(CLAIM_COMMIT_LOCK)
        .execute(&mut *holder)
        .await
        .expect("commit lock holds");

    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let runtime = harness.spawn(Harness::limits(), &script);
    harness.wait_advisory_waiter(CLAIM_COMMIT_LOCK).await;
    runtime.stop.cancel();
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(CLAIM_COMMIT_LOCK)
        .execute(&mut *holder)
        .await
        .expect("commit lock releases");
    runtime.stop().await;

    let row = harness.seed.run(run).await.expect("run reads");
    assert_eq!(
        (row.status.as_str(), row.attempts),
        ("pending", 0),
        "the committed claim was released with its attempt refunded"
    );
    let (token, expires) = harness.lease(run).await;
    assert!(
        token.is_some(),
        "the claim committed: its now-fenced lease token remains"
    );
    assert_eq!(expires, None, "the released run holds no live lease");
    assert_eq!(script.entered(), 0, "the released claim never executed");
    assert!(harness.writes().is_empty(), "nothing published");

    sqlx::query("DROP TRIGGER test_hold_claim_commit ON wyrd.verifier_runs")
        .execute(&harness.assertion)
        .await
        .expect("hold trigger drops");
    sqlx::query("DROP FUNCTION wyrd.test_hold_claim_commit()")
        .execute(&harness.assertion)
        .await
        .expect("hold function drops");
}

/// A runner that crashes after its result is stored and its detail batch is
/// durably acknowledged, but while its summary is blocked, leaves a partial
/// result that neither completes the run nor dispatches an Operator. Once the
/// lease expires the same run is reclaimed and writes its stored result
/// without executing again: the replayed detail batch is byte-identical, so
/// each result table holds exactly one copy, and completion and dispatch
/// happen only after every stored batch is acknowledged.
///
/// # Panics
/// Panics when the partial detail completes or dispatches, the run executes
/// twice, the replay differs from the stored batch, the attempt count differs,
/// or the final rows differ.
#[tokio::test]
async fn crash_after_detail_ack_reclaims_the_same_run_before_dispatch() {
    let harness = Harness::start().await;
    let (owner, principal) = harness
        .seed
        .service("owner")
        .await
        .expect("owner registers");
    let binding = harness
        .seed
        .bind_schedule(
            &owner,
            &owner,
            &harness.verifier,
            "0 2 * * *",
            vec![FrozenTarget::Digest("sha256:operator".to_owned())],
        )
        .await
        .expect("binding projects");
    harness
        .seed
        .activate(principal)
        .await
        .expect("owner activates");
    harness.make_due(binding).await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(drifting_report()));
    let fault = &harness.fault;
    fault.hang_next(RESULTS);
    let crash = CapabilityCrash::default();
    let health = std::sync::Arc::clone(&harness.server.state().verification);
    let runtime = RunningRuntime::spawn(harness.runtime(Harness::limits(), &script, &crash));
    wait_until("runtime health", || {
        health.is_composed() && !health.is_degraded()
    })
    .await;
    let run = harness.wait_single_run().await;

    let deadline = tokio::time::Instant::now() + WAIT;
    while harness.durable_rows().await.get(FEATURES) != Some(&2) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the detail batch never became durable"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        sent_to(fault, RESULTS).is_empty(),
        "the summary is blocked before it is sent"
    );
    crash.crash_next(RuntimeCapability::Runner);
    wait_until("runner down", || !health.is_up(RuntimeCapability::Runner)).await;
    wait_until("runner restarted", || !health.is_degraded()).await;

    let partial = harness.seed.run(run).await.expect("run reads");
    assert_eq!(
        (partial.status.as_str(), partial.attempts, partial.result_id),
        ("running", 1, None),
        "a durable detail without its summary never completes the run"
    );
    assert_eq!(
        harness.dispatches(run).await,
        0,
        "no dispatch from a partial result"
    );
    assert!(
        !harness.durable_rows().await.contains_key(RESULTS),
        "no summary was written"
    );

    harness.expire(run).await;
    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(
        row.attempts, 2,
        "the same run was reclaimed as its second attempt"
    );
    assert!(row.result_id.is_some());
    assert_eq!(harness.seed.runs().await.expect("runs read"), vec![run]);
    assert_eq!(
        harness.dispatches(run).await,
        1,
        "the failed binding result dispatches its Operator once completed"
    );
    assert_eq!(
        script.entered(),
        1,
        "the stored result is never re-executed"
    );
    assert_eq!(
        harness.writes(),
        vec![FEATURES.to_owned(), FEATURES.to_owned(), RESULTS.to_owned()]
    );
    let features = sent_to(fault, FEATURES);
    assert_eq!(
        features[0], features[1],
        "the replay writes the stored table, batch ID, and bytes"
    );
    runtime.stop().await;
    let rows = harness.durable_rows().await;
    assert_eq!(
        rows.get(FEATURES),
        Some(&2),
        "Scribe absorbs the replayed detail batch: {rows:?}"
    );
    assert_eq!(rows.get(RESULTS), Some(&1), "{rows:?}");
}

/// Write a small Parquet artifact for `data` and register its metadata, so a
/// fit of a baseline pinned to `data` decodes and settles `ready`.
///
/// # Panics
/// Panics when the Parquet encode, the storage write, or the metadata insert
/// fails.
async fn baseline_artifact(server: &WyrdTestServer, seed: &VerificationFixture, data: &CardUid) {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "latency_ms",
        DataType::Float64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0]))],
    )
    .expect("baseline batch");
    let mut bytes = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut bytes, schema, None).expect("parquet writer");
    writer.write(&batch).expect("batch writes");
    writer.close().expect("writer closes");

    let sha256 = STANDARD.encode(Sha256::digest(&bytes));
    let path = wyrd_storage::tenant_path::build(seed.tenant(), data.as_str(), "data/data.parquet");
    let storage = &server.state().storage;
    storage
        .operator()
        .write(&path, bytes.clone())
        .await
        .expect("artifact writes");
    let mut conn = server
        .state()
        .postgres
        .wyrd()
        .tenant_conn(seed.tenant())
        .await
        .expect("tenant connection opens");
    artifact_metadata::insert(
        &mut conn,
        NewArtifactMetadata {
            storage_path: &path,
            card_uid: data.as_str(),
            size_bytes: i64::try_from(bytes.len()).expect("artifact size fits"),
            sha256: &sha256,
            content_type: Some("application/vnd.apache.parquet"),
            sse_marker: None,
            backend: storage.backend(),
        },
    )
    .await
    .expect("artifact metadata inserts");
    conn.commit().await.expect("artifact metadata commits");
}

/// Read `(state, attempts)` of `verifier`'s baseline row.
///
/// # Panics
/// Panics when the read fails or the row does not exist.
async fn baseline_row(harness: &Harness, verifier: &CardUid) -> (String, i32) {
    sqlx::query_as(
        "SELECT state, attempts FROM wyrd.drift_baselines \
          WHERE data_tenant_id = $1 AND verifier_uid = $2",
    )
    .bind(harness.seed.tenant().as_uuid())
    .bind(verifier.as_uuid())
    .fetch_one(&harness.assertion)
    .await
    .expect("baseline row reads")
}

/// A fitter over `harness`'s server with `drain_grace`, holding fits at `gate`.
///
/// # Panics
/// Panics when the server has no operator pool.
fn fitter(harness: &Harness, drain_grace: Duration, gate: &FitGate) -> Arc<BaselineFitter> {
    let state = harness.server.state();
    let limits = RuntimeLimits {
        drain_grace,
        ..Harness::limits()
    };
    Arc::new(
        BaselineFitter::new(
            state.postgres.wyrd().clone(),
            state
                .postgres
                .operator_pool()
                .expect("server has an operator pool"),
            Arc::clone(&state.storage),
            &limits,
        )
        .with_fit_gate(gate.clone()),
    )
}

/// Baseline fitting follows the runtime's shutdown drain.
///
/// A fit admitted before shutdown finishes inside the drain grace and settles
/// `ready`. A fit still running when a short grace elapses is cancelled,
/// awaited, and released to `pending` with its attempt refunded. After
/// shutdown a pass admits no claim, so the released row stays unclaimed.
///
/// # Panics
/// Panics when a fitter does not drain within [`WAIT`], an admitted fit does
/// not settle `ready`, a cut-off fit is not released, or a claim is admitted
/// after shutdown.
#[tokio::test]
async fn baseline_fit_drains_within_grace_then_releases() {
    let harness = Harness::start().await;
    baseline_artifact(&harness.server, &harness.seed, &harness.subject).await;
    let gate = FitGate::default();
    gate.hold();

    let draining = fitter(&harness, Duration::from_secs(10), &gate);
    let stop = CancellationToken::new();
    let task = tokio::spawn(Arc::clone(&draining).run(stop.clone()));
    pending_baseline(
        &harness.server,
        &harness.seed,
        &harness.verifier,
        &harness.subject,
    )
    .await;
    wait_until("the first fit is admitted", || gate.entered() == 1).await;
    stop.cancel();
    assert!(!task.is_finished(), "an admitted fit holds the drain open");
    assert_eq!(
        baseline_row(&harness, &harness.verifier).await,
        ("building".to_owned(), 1)
    );
    gate.release();
    tokio::time::timeout(WAIT, task)
        .await
        .expect("the fitter drains within the wait")
        .expect("the fitter does not panic");
    assert_eq!(
        baseline_row(&harness, &harness.verifier).await,
        ("ready".to_owned(), 1),
        "a fit admitted before shutdown settles inside the grace"
    );

    let cut_off = harness
        .seed
        .drift_verifier("drift-cut-off")
        .await
        .expect("verifier registers");
    gate.hold();
    let releasing = fitter(&harness, Duration::from_millis(100), &gate);
    let stop = CancellationToken::new();
    let task = tokio::spawn(Arc::clone(&releasing).run(stop.clone()));
    pending_baseline(&harness.server, &harness.seed, &cut_off, &harness.subject).await;
    wait_until("the second fit is admitted", || gate.entered() == 2).await;
    stop.cancel();
    tokio::time::timeout(WAIT, task)
        .await
        .expect("the fitter releases within the wait")
        .expect("the fitter does not panic");
    assert_eq!(
        baseline_row(&harness, &cut_off).await,
        ("pending".to_owned(), 0),
        "a fit past the grace is released with its attempt refunded"
    );

    let settled = releasing.pass(&stop).await.expect("pass reads due tenants");
    assert_eq!(settled, 0, "no claim is admitted after shutdown");
    assert_eq!(gate.entered(), 2, "no fit starts after shutdown");
    assert_eq!(
        baseline_row(&harness, &cut_off).await,
        ("pending".to_owned(), 0)
    );
}

/// A fitter stopped while its claim transaction is parked admits nothing:
/// the uncommitted claim rolls back once the lock is released, so the
/// baseline stays pending with no attempt charged and no fit starts.
///
/// # Panics
/// Panics when the fitter never blocks, does not stop while blocked, or the
/// baseline is leased, charged, or fitted after shutdown.
#[tokio::test]
async fn stopped_fitter_rolls_back_its_blocked_claim() {
    let harness = Harness::start().await;
    pending_baseline(
        &harness.server,
        &harness.seed,
        &harness.verifier,
        &harness.subject,
    )
    .await;
    let gate = FitGate::default();
    let blocker = harness.block_writes("wyrd.drift_baselines").await;
    let stop = CancellationToken::new();
    let task = tokio::spawn(fitter(&harness, Duration::from_secs(10), &gate).run(stop.clone()));
    let pid = harness.wait_blocked_writer("wyrd.drift_baselines").await;

    stop.cancel();
    tokio::time::timeout(WAIT, task)
        .await
        .expect("the fitter stops while its claim is blocked")
        .expect("the fitter does not panic");
    blocker.rollback().await.expect("blocker releases");
    harness.wait_backend_settled(pid).await;

    assert_eq!(
        baseline_row(&harness, &harness.verifier).await,
        ("pending".to_owned(), 0),
        "the claim rolled back: no lease and no charged attempt"
    );
    assert_eq!(gate.entered(), 0, "no fit started");
}

/// Advisory lock key the fitter commit-race test's deferred trigger waits on.
const FIT_CLAIM_COMMIT_LOCK: i64 = 0x5752_4446;

/// A fit claim whose commit completes after shutdown began is released
/// through the fenced release transition with its attempt refunded, and is
/// never fitted.
///
/// A test-scoped deferred constraint trigger on the baseline makes the
/// claim's `COMMIT` wait on an advisory lock the test holds, so the claim is
/// already applied and its commit in flight when `stop` fires; releasing the
/// lock lets the commit win the race.
///
/// # Panics
/// Panics when the commit never blocks, the committed claim is not released
/// with its attempt refunded, or the claim is fitted.
#[tokio::test]
async fn fit_claim_committed_after_shutdown_is_released_unfitted() {
    let harness = Harness::start().await;
    pending_baseline(
        &harness.server,
        &harness.seed,
        &harness.verifier,
        &harness.subject,
    )
    .await;
    sqlx::query(
        "CREATE FUNCTION wyrd.test_hold_fit_claim_commit() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
         PERFORM pg_advisory_xact_lock(TG_ARGV[0]::bigint); RETURN NULL; END $$",
    )
    .execute(&harness.assertion)
    .await
    .expect("hold function creates");
    sqlx::query("GRANT EXECUTE ON FUNCTION wyrd.test_hold_fit_claim_commit() TO PUBLIC")
        .execute(&harness.assertion)
        .await
        .expect("hold function grants");
    // The Verifier UID is a typed UUID the fixture minted, not caller input.
    sqlx::query(AssertSqlSafe(format!(
        "CREATE CONSTRAINT TRIGGER test_hold_fit_claim_commit \
         AFTER UPDATE ON wyrd.drift_baselines \
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW \
         WHEN (NEW.verifier_uid = '{}'::uuid AND NEW.state = 'building') \
         EXECUTE FUNCTION wyrd.test_hold_fit_claim_commit('{FIT_CLAIM_COMMIT_LOCK}')",
        harness.verifier.as_uuid()
    )))
    .execute(&harness.assertion)
    .await
    .expect("hold trigger creates");
    let mut holder = harness.assertion.acquire().await.expect("holder connects");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(FIT_CLAIM_COMMIT_LOCK)
        .execute(&mut *holder)
        .await
        .expect("commit lock holds");

    let gate = FitGate::default();
    let stop = CancellationToken::new();
    let task = tokio::spawn(fitter(&harness, Duration::from_secs(10), &gate).run(stop.clone()));
    harness.wait_advisory_waiter(FIT_CLAIM_COMMIT_LOCK).await;
    stop.cancel();
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(FIT_CLAIM_COMMIT_LOCK)
        .execute(&mut *holder)
        .await
        .expect("commit lock releases");
    tokio::time::timeout(WAIT, task)
        .await
        .expect("the fitter stops after its late commit")
        .expect("the fitter does not panic");

    assert_eq!(
        baseline_row(&harness, &harness.verifier).await,
        ("pending".to_owned(), 0),
        "the committed claim was released with its attempt refunded"
    );
    let token: Option<Uuid> = sqlx::query_scalar(
        "SELECT lease_token FROM wyrd.drift_baselines \
          WHERE data_tenant_id = $1 AND verifier_uid = $2",
    )
    .bind(harness.seed.tenant().as_uuid())
    .bind(harness.verifier.as_uuid())
    .fetch_one(&harness.assertion)
    .await
    .expect("lease token reads");
    assert!(
        token.is_some(),
        "the claim committed: its now-fenced lease token remains"
    );
    assert_eq!(gate.entered(), 0, "the released claim was never fitted");

    sqlx::query("DROP TRIGGER test_hold_fit_claim_commit ON wyrd.drift_baselines")
        .execute(&harness.assertion)
        .await
        .expect("hold trigger drops");
    sqlx::query("DROP FUNCTION wyrd.test_hold_fit_claim_commit()")
        .execute(&harness.assertion)
        .await
        .expect("hold function drops");
}

/// Add or drop a constraint that makes PostgreSQL refuse every new run row,
/// standing in for a database that cannot accept the outbox's writes.
///
/// # Panics
/// Panics when the DDL fails.
async fn refuse_run_writes(assertion: &PgPool, refuse: bool) {
    let ddl = if refuse {
        "ALTER TABLE wyrd.verifier_runs ADD CONSTRAINT outbox_outage CHECK (false) NOT VALID"
    } else {
        "ALTER TABLE wyrd.verifier_runs DROP CONSTRAINT outbox_outage"
    };
    sqlx::query(ddl)
        .execute(assertion)
        .await
        .expect("outage constraint changes");
}

/// The tenant's observation run record IDs, sorted.
///
/// # Panics
/// Panics when the runs cannot be read.
async fn observation_records(seed: &VerificationFixture) -> Vec<String> {
    let mut records: Vec<String> = seed
        .observation_runs()
        .await
        .expect("observation runs read")
        .into_iter()
        .map(|run| run.record_id)
        .collect();
    records.sort();
    records
}

/// The Eval run-request outbox keeps every request while PostgreSQL refuses
/// its writes and retries; once writes succeed, exactly one run exists per
/// record, a repeated request adds none, graceful shutdown flushes what it
/// holds, and a request still unwritten at the shutdown deadline is reported.
///
/// # Panics
/// Panics when a request is dropped during the outage, a run is duplicated or
/// missing, or shutdown misreports what it left unwritten.
#[tokio::test]
async fn observation_outbox_retains_through_an_outage_and_flushes_at_shutdown() {
    let harness = Harness::start().await;
    let seed = &harness.seed;
    let (owner, principal) = seed.service("eval-owner").await.expect("owner registers");
    let verifier = seed
        .verifier(
            "eval",
            &serde_json::json!({ "implementation": { "kind": "eval", "spec": { "tasks": {} } } }),
        )
        .await
        .expect("Eval Verifier registers");
    seed.bind_observations(&owner, &verifier)
        .await
        .expect("binding projects");
    seed.activate(principal).await.expect("owner activates");
    let tenant = harness.server.pg_fixture().data_tenant_id();
    let record = |record_id: &str| ObservationRecord {
        subject: owner.clone(),
        writer: owner.clone(),
        record_id: record_id.to_owned(),
        event_time: Utc::now(),
    };
    let postgres = harness.server.state().postgres.wyrd().clone();
    let outbox = ObservationRunSink::outbox(postgres.clone());

    refuse_run_writes(&harness.assertion, true).await;
    outbox.stage(tenant, record("r-1"));
    outbox.stage(tenant, record("r-2"));
    assert_eq!(
        outbox
            .settle(std::time::Instant::now() + Duration::from_millis(400))
            .await,
        2,
        "the outage drops no request"
    );
    assert!(observation_records(seed).await.is_empty());

    refuse_run_writes(&harness.assertion, false).await;
    outbox.stage(tenant, record("r-1"));
    outbox.stage(tenant, record("r-3"));
    assert_eq!(
        outbox.settle(std::time::Instant::now() + WAIT).await,
        0,
        "the outbox recovers once writes succeed"
    );
    assert_eq!(observation_records(seed).await, ["r-1", "r-2", "r-3"]);

    outbox.stage(tenant, record("r-4"));
    assert_eq!(
        outbox.shutdown(std::time::Instant::now() + WAIT).await,
        0,
        "graceful shutdown flushes the queue"
    );
    assert_eq!(
        observation_records(seed).await,
        ["r-1", "r-2", "r-3", "r-4"]
    );

    let stranded = ObservationRunSink::outbox(postgres);
    refuse_run_writes(&harness.assertion, true).await;
    stranded.stage(tenant, record("r-5"));
    assert_eq!(
        stranded
            .shutdown(std::time::Instant::now() + Duration::from_millis(300))
            .await,
        1,
        "a request unwritten at the deadline is reported"
    );
    refuse_run_writes(&harness.assertion, false).await;
}

/// The hour before now, the window every manual run in these tests covers.
fn last_hour() -> DriftWindow {
    let now = Utc::now();
    DriftWindow {
        start: now - chrono::Duration::hours(1),
        end: now,
    }
}

/// The runner's first logged claim round.
///
/// # Panics
/// Panics when the runner logged no claim round.
fn first_runner_round(logs: &LogCapture) -> String {
    logs.text()
        .lines()
        .find(|line| line.contains("claim round") && line.contains(r#"capability="runner""#))
        .map(str::to_owned)
        .expect("the runner logs its claim rounds")
}

/// With more than 64 tenants each holding a claimable run, the runner's
/// first claim round considers and claims every one of them.
///
/// # Panics
/// Panics when seeding fails, the first round misses a tenant, or a run does
/// not complete.
#[tokio::test]
async fn every_tenant_is_claimed_in_the_first_round() {
    let logs = LogCapture::install();
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    let mut runs = vec![(harness.seed.clone(), harness.enqueue().await)];
    for index in 1..70 {
        let tenant = DataTenantId::new_v7();
        harness
            .server
            .pg_fixture()
            .seed_additional_tenant_with_uuid(tenant, &format!("claim-round-{index}"))
            .await
            .expect("tenant seeds");
        let (seed, subject, verifier) = seed_tenant(&harness.server, tenant).await;
        let run = seed
            .enqueue_direct(&verifier, &subject, last_hour())
            .await
            .expect("run enqueues");
        runs.push((seed, run));
    }
    for _ in 0..70 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    }
    let runtime = harness.spawn(Harness::limits(), &script);
    wait_until("all 70 executions", || script.entered() == 70).await;
    let round = first_runner_round(&logs);
    assert!(
        round.contains("due=70") && round.contains("claimed=70"),
        "the first round claims every tenant: {round}"
    );
    script.release();
    for (seed, run) in &runs {
        wait_run_in(seed, *run, status("completed")).await;
    }
    runtime.stop().await;
}

/// A second run of the same Verifier on one process resolves it from the
/// process cache without reading its Card, and a run whose Verifier was
/// deleted settles `errored` without executing or writing.
///
/// # Panics
/// Panics when the second run reads the Card, the deleted Verifier's run
/// executes or settles otherwise, or anything more is written.
#[tokio::test]
async fn verifier_cards_are_cached_and_a_deleted_verifier_errors() {
    let logs = LogCapture::install();
    let harness = Harness::start().await;
    let script = EngineScript::default();
    for _ in 0..2 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    }
    let runtime = harness.spawn(Harness::limits(), &script);
    for _ in 0..2 {
        let run = harness.enqueue().await;
        harness.wait_run(run, status("completed")).await;
    }
    runtime.stop().await;
    let misses = logs
        .text()
        .lines()
        .filter(|line| line.contains("Verifier Card cache miss"))
        .count();
    assert_eq!(misses, 1, "only the first run reads the Verifier Card");

    let orphan = harness.enqueue().await;
    sqlx::query("UPDATE wyrd.cards SET status = 'deleted' WHERE card_uid = $1")
        .bind(harness.verifier.as_uuid())
        .execute(&harness.assertion)
        .await
        .expect("the Verifier is deleted");
    let runtime = harness.spawn(Harness::limits(), &script);
    let row = harness.wait_run(orphan, status("errored")).await;
    assert_eq!(row.error_code.as_deref(), Some(VERIFIER_UNAVAILABLE));
    assert_eq!(script.entered(), 2, "a deleted Verifier never executes");
    assert_eq!(
        harness.writes(),
        vec![RESULTS.to_owned(), RESULTS.to_owned()]
    );
    runtime.stop().await;
}

/// A run executing longer than its lease keeps it through renewal on the
/// database clock, and a renewal that finds its token gone cancels the work,
/// which then stores and writes nothing.
///
/// # Panics
/// Panics when the lease is not extended, the run is reclaimed while live,
/// the taken token does not cancel the work, or anything is stored or
/// written.
#[tokio::test]
async fn renewal_keeps_a_long_run_and_a_taken_token_cancels_it() {
    let logs = LogCapture::install();
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Completed(drifting_report()));
    let lease = Duration::from_secs(3);
    let runtime = harness.spawn(
        RuntimeLimits {
            lease,
            ..Harness::limits()
        },
        &script,
    );
    let run = harness.enqueue().await;
    wait_until("the claim", || script.entered() == 1).await;
    let (token, claimed_expiry) = harness.lease(run).await;

    tokio::time::sleep(lease * 2).await;
    let row = harness.seed.run(run).await.expect("run reads");
    assert_eq!(
        (row.status.as_str(), row.attempts),
        ("running", 1),
        "renewal keeps the lease past its length"
    );
    let (renewed_token, renewed_expiry) = harness.lease(run).await;
    assert_eq!(renewed_token, token, "renewal keeps the claim's token");
    assert!(renewed_expiry > claimed_expiry, "renewal extends the lease");

    sqlx::query("UPDATE wyrd.verifier_runs SET lease_token = gen_random_uuid() WHERE run_id = $1")
        .bind(run.as_uuid())
        .execute(&harness.assertion)
        .await
        .expect("the token is taken");
    wait_until("the taken lease cancels the work", || {
        logs.text().contains("a held Verifier run lease was taken")
    })
    .await;
    script.release();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(harness.writes().is_empty(), "cancelled work writes nothing");
    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.verifier_run_results WHERE run_id = $1")
            .bind(run.as_uuid())
            .fetch_one(&harness.assertion)
            .await
            .expect("stored results read");
    assert_eq!(stored, 0, "cancelled work stores nothing");
    runtime.stop().await;
}

/// Two hundred held runs released at once on the eight-connection test pool
/// all complete on their first attempt: no settlement fails and no
/// connection acquire times out, because a run holds a connection only to
/// claim, store, settle, and renew.
///
/// # Panics
/// Panics when a run does not complete on its first attempt, or a settlement
/// failure or pool timeout is logged.
#[tokio::test]
async fn two_hundred_released_runs_complete_on_their_first_attempt() {
    let logs = LogCapture::install();
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    let mut runs = Vec::new();
    for _ in 0..200 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
        runs.push(harness.enqueue().await);
    }
    let runtime = harness.spawn(Harness::limits(), &script);
    wait_until("all 200 executions", || script.entered() == 200).await;
    script.release();
    for run in runs {
        let row = harness.wait_run(run, status("completed")).await;
        assert_eq!(row.attempts, 1, "{row:?}");
    }
    runtime.stop().await;
    let text = logs.text();
    assert!(
        !text.contains("verification settlement failed"),
        "no settlement fails"
    );
    assert!(
        !text.contains("pool timed out"),
        "no connection acquire times out"
    );
}

/// A run refused by a full shared resource returns to the queue without
/// consuming an attempt, and the process claims nothing new until one of its
/// running runs finishes.
///
/// # Panics
/// Panics when the refused run is charged an attempt, is claimed again while
/// the other run still runs, or does not complete afterwards.
#[tokio::test]
async fn a_refused_run_pauses_claiming_until_a_running_run_finishes() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    script.push_unheld(EngineOutcome::Deferred(VerificationError {
        code: "input_admission_refused".to_owned(),
        message: "the input read was refused at admission".to_owned(),
    }));
    script.push_unheld(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let runtime = harness.spawn(Harness::limits(), &script);
    let running = harness.enqueue().await;
    wait_until("the running run executes", || script.entered() == 1).await;
    let refused = harness.enqueue().await;
    let row = harness
        .wait_run(refused, |row| row.attempts == 0 && row.status == "pending")
        .await;
    assert_eq!(row.error_code, None, "a refusal is not a failure");

    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        script.entered(),
        2,
        "nothing is claimed while the refusal pauses claiming"
    );
    script.release();
    harness.wait_run(running, status("completed")).await;
    let row = harness.wait_run(refused, status("completed")).await;
    assert_eq!(row.attempts, 1, "the refusal consumed no attempt");
    runtime.stop().await;
}
