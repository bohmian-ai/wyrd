//! Integration coverage for the supervised verification runtime against real
//! Postgres and a real bound server's Bifrost write path.
//!
//! Every test boots the production bound server (gRPC, Gate, Scribe), seeds
//! one tenant through [`VerificationFixture`], and composes a
//! [`VerificationRuntime`] on that server's own state with a scripted engine.
//! Coordination deadlines are PostgreSQL's, so a test that needs one to elapse
//! places the row itself in the past through the fixture. Results travel as the tenant SYSTEM writer over gRPC to
//! the server's Scribe; run state is read back from `wyrd.verifier_runs` and
//! every Gate decision from `vala.audit_staging`, whose publisher is disabled
//! so staged rows stay observable.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, PgPool, Postgres, Transaction};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_drift::{DriftReport, DriftVerdict, FeatureDriftReport};
use wyrd_server::verification::drift::DRIFT_INVALID;
use wyrd_server::verification::engines::{EngineOutcome, VerifierReport};
use wyrd_server::verification::fitter::{BaselineFitter, FitGate};
use wyrd_server::verification::health::RuntimeCapability;
use wyrd_server::verification::permits::VerifierPermits;
use wyrd_server::verification::publisher::{PublicationFault, SentBatch};
use wyrd_server::verification::runner::{EngineScript, RESULT_PUBLICATION_FAILED};
use wyrd_server::verification::{CapabilityCrash, RuntimeLimits, VerificationRuntime};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::DriftMethod;
use wyrd_spec::card::verifier::DriftBaselineState;
use wyrd_spec::ids::{BindingId, CardUid, FeatureName, VerificationRunId};
use wyrd_spec::verification::{DriftWindow, FrozenTarget, VerificationError};
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::verifier_runs::TerminalStatus;
use wyrd_testing::WyrdTestServer;
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
            publication_timeout: Duration::from_secs(20),
            drain_grace: Duration::from_secs(10),
            poll_interval: Duration::from_millis(50),
            restart_backoff: Duration::from_millis(300),
            ..RuntimeLimits::default()
        }
    }

    /// Compose a full runtime publishing through this server's gRPC listener.
    ///
    /// # Panics
    /// Panics when the server is not bound or the runtime cannot compose.
    fn runtime(
        &self,
        limits: RuntimeLimits,
        script: &EngineScript,
        fault: &PublicationFault,
        crash: &CapabilityCrash,
    ) -> VerificationRuntime {
        VerificationRuntime::builder(self.server.state())
            .limits(limits)
            .ingest_endpoint(self.server.grpc_url().expect("bound server serves gRPC"))
            .engine_script(script.clone())
            .publication_fault(fault.clone())
            .crash_switch(crash.clone())
            .build()
            .expect("the runtime composes")
    }

    /// Spawn a full runtime with `script` and default faults; returns its
    /// stop token and task.
    fn spawn(&self, limits: RuntimeLimits, script: &EngineScript) -> RunningRuntime {
        RunningRuntime::spawn(self.runtime(
            limits,
            script,
            &PublicationFault::default(),
            &CapabilityCrash::default(),
        ))
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

    /// Every Gate write decision staged for the tenant since the floor, as
    /// `(principal_kind, resource, outcome)` in staging order.
    ///
    /// # Panics
    /// Panics when the staging read fails.
    async fn writes(&self) -> Vec<(String, String, String)> {
        sqlx::query_as(
            "SELECT principal_kind, resource, outcome FROM vala.audit_staging \
             WHERE data_tenant_id = $1 AND operation = 'bifrost.record.write' AND seq > $2 \
             ORDER BY seq",
        )
        .bind(self.seed.tenant().as_uuid())
        .bind(self.audit_floor)
        .fetch_all(&self.assertion)
        .await
        .expect("staged write decisions")
    }

    /// Every staged audit operation for the tenant since the floor.
    ///
    /// # Panics
    /// Panics when the staging read fails.
    async fn audit_operations(&self) -> Vec<String> {
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

    /// Wait until exactly `count` Gate write decisions are staged.
    ///
    /// # Panics
    /// Panics when the count is not reached within [`WAIT`].
    async fn wait_writes(&self, count: usize) -> Vec<(String, String, String)> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let writes = self.writes().await;
            if writes.len() >= count {
                return writes;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "expected {count} staged writes, saw {writes:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
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

    /// Open a superuser transaction holding `SHARE` on `wyrd.verifier_runs`.
    ///
    /// Reads still proceed, but every run insert or update blocks until the
    /// returned transaction ends, which parks a scheduler occurrence
    /// transaction at its enqueue and a runner claim transaction at its first
    /// update, each inside its still-uncommitted tenant transaction.
    ///
    /// # Panics
    /// Panics when the transaction or lock cannot be taken.
    async fn block_run_writes(&self) -> Transaction<'static, Postgres> {
        let mut blocker = self.assertion.begin().await.expect("blocker begins");
        sqlx::query("LOCK TABLE wyrd.verifier_runs IN SHARE MODE")
            .execute(&mut *blocker)
            .await
            .expect("run table locks");
        blocker
    }

    /// Wait until a backend is blocked writing `wyrd.verifier_runs` and
    /// return its PID.
    ///
    /// Polls `pg_locks` for an ungranted `RowExclusiveLock`, so the caller
    /// knows the runtime is parked inside its tenant transaction.
    ///
    /// # Panics
    /// Panics when no writer blocks within [`WAIT`].
    async fn wait_blocked_writer(&self) -> i32 {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let pid: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_locks \
                 WHERE relation = 'wyrd.verifier_runs'::regclass \
                   AND mode = 'RowExclusiveLock' AND NOT granted \
                 LIMIT 1",
            )
            .fetch_optional(&self.assertion)
            .await
            .expect("lock waiters read");
            if let Some(pid) = pid {
                return pid;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "no runtime write blocked on the run table"
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

/// The `(principal_kind, resource, outcome)` of one allowed SYSTEM write.
fn system_write(table: &str) -> (String, String, String) {
    ("system".to_owned(), table.to_owned(), "allowed".to_owned())
}

/// A completed Drift run writes its feature details and then its summary as
/// the tenant SYSTEM writer, completes with a result, stages no audit beyond
/// those two Gate decisions, and records the runtime metrics.
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
        harness.writes().await,
        vec![system_write(FEATURES), system_write(RESULTS)]
    );
    assert_eq!(
        harness.audit_operations().await,
        vec![
            "bifrost.record.write".to_owned(),
            "bifrost.record.write".to_owned()
        ],
        "claims and settlements stage no audit; only the Gate's write decisions do"
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
        rendered.contains(r#"wyrd_verification_run_attempts_total{implementation="drift"} 1"#),
        "{rendered}"
    );
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
    assert_eq!(harness.writes().await, vec![system_write(RESULTS)]);
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
    assert!(harness.writes().await.is_empty());
    runtime.stop().await;
}

/// A summary that is not acknowledged leaves the acknowledged details written,
/// retries the run instead of completing it, and the next attempt publishes a
/// fresh detail batch and summary before completing. The fresh detail batch
/// carries a new batch ID and Scribe keeps both attempts' detail rows, so a
/// fresh `write_batch` is never treated as a deduplicated replay.
///
/// # Panics
/// Panics when the failed attempt completes, the retry is not scheduled with
/// the publication code, the writes differ from the expected sequence, the
/// fresh batch reuses a batch ID, or the durable row counts differ.
#[tokio::test]
async fn unacknowledged_summary_retries_with_a_fresh_result() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.push(EngineOutcome::Completed(drifting_report()));
    script.push(EngineOutcome::Completed(drifting_report()));
    let fault = PublicationFault::default();
    fault.fail_next(RESULTS);
    let runtime = RunningRuntime::spawn(harness.runtime(
        Harness::limits(),
        &script,
        &fault,
        &CapabilityCrash::default(),
    ));
    let run = harness.enqueue().await;

    let row = harness.wait_run(run, status("retrying")).await;
    assert_eq!(row.error_code.as_deref(), Some(RESULT_PUBLICATION_FAILED));
    assert_eq!(row.result_id, None);
    assert_eq!(harness.writes().await, vec![system_write(FEATURES)]);

    harness.expire(run).await;
    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 2);
    assert_eq!(
        harness.writes().await,
        vec![
            system_write(FEATURES),
            system_write(FEATURES),
            system_write(RESULTS)
        ]
    );
    let features = sent_to(&fault, FEATURES);
    assert_eq!(features.len(), 2, "one detail batch per attempt");
    assert_ne!(
        features[0].batch_id, features[1].batch_id,
        "the retried attempt's write_batch sealed a fresh batch, not a replay"
    );
    runtime.stop().await;
    let rows = harness.durable_rows().await;
    assert_eq!(
        rows.get(FEATURES),
        Some(&4),
        "both attempts' two feature rows stay durable; a fresh batch is never deduplicated: {rows:?}"
    );
    assert_eq!(rows.get(RESULTS), Some(&1), "{rows:?}");
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
    let fault = PublicationFault::default();
    fault.lose_ack_next(RESULTS);
    let runtime = RunningRuntime::spawn(harness.runtime(
        Harness::limits(),
        &script,
        &fault,
        &CapabilityCrash::default(),
    ));
    let run = harness.enqueue().await;

    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 1, "the replay settled inside one attempt");
    assert!(row.result_id.is_some());
    let summaries = sent_to(&fault, RESULTS);
    assert_eq!(
        summaries.len(),
        2,
        "the unacknowledged summary was resent once"
    );
    assert_eq!(
        summaries[0], summaries[1],
        "the replay carries the identical table, batch ID, and sealed bytes"
    );
    let details = sent_to(&fault, FEATURES);
    assert_eq!(details.len(), 1);
    assert_ne!(details[0].batch_id, summaries[0].batch_id);
    assert_eq!(
        harness.writes().await,
        vec![
            system_write(FEATURES),
            system_write(RESULTS),
            system_write(RESULTS)
        ],
        "each request, replay included, is one Gate decision"
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
    assert!(harness.writes().await.is_empty());
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
    assert!(harness.writes().await.is_empty());
    runtime.stop().await;
}

/// Permits are taken before claiming: a busy tenant stops at its own ceiling,
/// another tenant still gets capacity, the process never exceeds the global
/// ceiling, and every run completes once capacity frees.
///
/// # Panics
/// Panics when a ceiling is exceeded, the second tenant is starved, or a run
/// does not complete.
#[tokio::test]
async fn permits_cap_each_tenant_and_share_the_process() {
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
    for _ in 0..6 {
        script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    }
    let mut busy = Vec::new();
    for _ in 0..4 {
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
    let runtime = harness.spawn(
        RuntimeLimits {
            global_permits: 3,
            tenant_permits: 2,
            ..Harness::limits()
        },
        &script,
    );

    wait_until("three executions", || script.entered() == 3).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(script.entered(), 3, "the global ceiling holds across polls");
    let mut running_busy = 0;
    for run in &busy {
        if harness.seed.run(*run).await.expect("run reads").status == "running" {
            running_busy += 1;
        }
    }
    let mut running_quiet = 0;
    for run in &quiet {
        if other.run(*run).await.expect("run reads").status == "running" {
            running_quiet += 1;
        }
    }
    assert_eq!(running_busy, 2, "the busy tenant stops at its ceiling");
    assert_eq!(running_quiet, 1, "the other tenant is not starved");

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

/// Baseline fits draw from the runner's permits: while a tenant's four
/// Verifier runs hold its whole share, its due fit stays unclaimed and
/// another tenant's fit proceeds; releasing the runs lets the same row be
/// claimed.
///
/// Each fit here settles `failed` because the fixture's baseline Card has no
/// Parquet artifact; leaving `pending` is the claim this test observes.
///
/// # Panics
/// Panics when the saturated tenant's fit is claimed, the other tenant's fit
/// is starved, or the released row is never claimed.
#[tokio::test]
async fn baseline_fits_share_the_verifier_permits() {
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
    let deadline = tokio::time::Instant::now() + WAIT;
    while baseline_state(&harness.server, &other, &other_verifier).await
        == DriftBaselineState::Pending
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the other tenant's fit was starved"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        baseline_state(&harness.server, &harness.seed, &harness.verifier).await,
        DriftBaselineState::Pending,
        "a tenant at its permit ceiling leaves its fit unclaimed"
    );

    script.release();
    for run in busy {
        harness.wait_run(run, status("completed")).await;
    }
    let deadline = tokio::time::Instant::now() + WAIT;
    while baseline_state(&harness.server, &harness.seed, &harness.verifier).await
        == DriftBaselineState::Pending
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the released tenant's fit was never claimed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    runtime.stop().await;
}

/// An expired lease is reclaimed by another runner that completes the run;
/// the original holder's late settlement is fenced and changes nothing.
///
/// # Panics
/// Panics when the run is not reclaimed, the stale holder overwrites the
/// result, or the attempts differ.
#[tokio::test]
async fn expired_lease_is_reclaimed_and_the_stale_holder_is_fenced() {
    let harness = Harness::start().await;
    let stale_script = EngineScript::default();
    stale_script.hold();
    stale_script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let stale = harness.spawn(
        RuntimeLimits {
            global_permits: 1,
            ..Harness::limits()
        },
        &stale_script,
    );
    let run = harness.enqueue().await;
    wait_until("the first claim", || stale_script.entered() == 1).await;

    harness.expire(run).await;
    let fresh_script = EngineScript::default();
    fresh_script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let fresh = harness.spawn(Harness::limits(), &fresh_script);
    let row = harness.wait_run(run, status("completed")).await;
    assert_eq!(row.attempts, 2, "the reclaim is the second attempt");

    stale_script.release();
    harness.wait_writes(2).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness.seed.run(run).await.expect("run reads"),
        row,
        "the stale holder's settlement is fenced"
    );
    stale.stop().await;
    fresh.stop().await;
}

/// A crashed runner degrades health, restarts, and — once the lost lease
/// expires — reclaims and completes the run exactly once.
///
/// # Panics
/// Panics when health does not degrade and recover, the run is not
/// completed on its second attempt, or more than one summary is written.
#[tokio::test]
async fn crashed_runner_restarts_and_reclaims_without_duplicates() {
    let harness = Harness::start().await;
    let script = EngineScript::default();
    script.hold();
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    script.push(EngineOutcome::Completed(VerifierReport::Drift(None)));
    let crash = CapabilityCrash::default();
    let runtime = RunningRuntime::spawn(harness.runtime(
        Harness::limits(),
        &script,
        &PublicationFault::default(),
        &crash,
    ));
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
    assert_eq!(harness.writes().await, vec![system_write(RESULTS)]);
    runtime.stop().await;
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
    assert!(harness.writes().await.is_empty());
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
    assert_eq!(harness.writes().await, vec![system_write(RESULTS)]);
}

/// Concurrent schedulers ticking the same due occurrence, and a scheduler
/// restarted after them, create exactly one run; a scheduler-only runtime
/// does not require the runner, so health is not degraded.
///
/// # Panics
/// Panics when a duplicate run is created, the runner is composed without an
/// endpoint, or health degrades.
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
    let scheduler_only = || {
        VerificationRuntime::builder(harness.server.state())
            .limits(Harness::limits())
            .build()
            .expect("the scheduler composes")
    };
    let first = scheduler_only();
    assert!(first.composes(RuntimeCapability::Scheduler));
    assert!(
        !first.composes(RuntimeCapability::Runner),
        "no endpoint, no runner"
    );
    let first = RunningRuntime::spawn(first);
    let second = RunningRuntime::spawn(scheduler_only());
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

    let restarted = RunningRuntime::spawn(scheduler_only());
    tokio::time::sleep(Duration::from_millis(300)).await;
    restarted.stop().await;
    let runs = harness.seed.runs().await.expect("runs read");
    assert_eq!(runs.len(), 1, "one run per occurrence");
    assert_eq!(
        harness.seed.run(runs[0]).await.expect("run reads").status,
        "pending",
        "a scheduler-only runtime never claims"
    );
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

    let blocker = harness.block_run_writes().await;
    let runtime = RunningRuntime::spawn(
        VerificationRuntime::builder(harness.server.state())
            .limits(Harness::limits())
            .build()
            .expect("the scheduler composes"),
    );
    let pid = harness.wait_blocked_writer().await;
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

    let blocker = harness.block_run_writes().await;
    let runtime = harness.spawn(Harness::limits(), &script);
    let pid = harness.wait_blocked_writer().await;

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
    assert!(harness.writes().await.is_empty(), "nothing published");
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
    assert!(harness.writes().await.is_empty(), "nothing published");

    sqlx::query("DROP TRIGGER test_hold_claim_commit ON wyrd.verifier_runs")
        .execute(&harness.assertion)
        .await
        .expect("hold trigger drops");
    sqlx::query("DROP FUNCTION wyrd.test_hold_claim_commit()")
        .execute(&harness.assertion)
        .await
        .expect("hold function drops");
}

/// A runner that crashes after its detail batch is durably acknowledged but
/// while its summary is blocked leaves a partial result that neither completes
/// the run nor dispatches an Operator; once the lease expires the same run is
/// reclaimed within its attempt budget, and completion and dispatch happen
/// only after that later attempt receives every required acknowledgement.
///
/// # Panics
/// Panics when the partial detail completes or dispatches, a different run is
/// executed, the attempt count differs, or the final rows differ.
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
    script.push(EngineOutcome::Completed(drifting_report()));
    let fault = PublicationFault::default();
    fault.hang_next(RESULTS);
    let crash = CapabilityCrash::default();
    let health = std::sync::Arc::clone(&harness.server.state().verification);
    let runtime =
        RunningRuntime::spawn(harness.runtime(Harness::limits(), &script, &fault, &crash));
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
        sent_to(&fault, RESULTS).is_empty(),
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
        harness.writes().await,
        vec![
            system_write(FEATURES),
            system_write(FEATURES),
            system_write(RESULTS)
        ]
    );
    runtime.stop().await;
    let rows = harness.durable_rows().await;
    assert_eq!(rows.get(FEATURES), Some(&4), "{rows:?}");
    assert_eq!(rows.get(RESULTS), Some(&1), "{rows:?}");
}

/// Write a small Parquet artifact for `data` and register its metadata, so a
/// fit of a baseline pinned to `data` decodes and settles `ready`.
///
/// # Panics
/// Panics when the Parquet encode, the storage write, or the metadata insert
/// fails.
async fn baseline_artifact(server: &WyrdTestServer, seed: &VerificationFixture, data: &CardUid) {
    use arrow::array::Float64Array;
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::parquet::arrow::ArrowWriter;
    use wyrd_sql::queries::storage::artifact_metadata::{self, NewArtifactMetadata};

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

    let sha256 = {
        use base64::Engine as _;
        use sha2::Digest as _;
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&bytes))
    };
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
            Arc::new(VerifierPermits::new(
                limits.global_permits,
                limits.tenant_permits,
            )),
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
