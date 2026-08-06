//! Transactional outbox audit owner for retained Oracle queries.

use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, BifrostQueryReadDecision, BifrostSecurityViolation, OracleAudit,
    VerifiedSecurityContext,
};
use vala_sql::ValaPostgres;
use wyrd_spec::vala::api::{AuditDecision, AuditDetail, AuditEvent, AuditResult};
use wyrd_spec::vala::error::BifrostError;

use super::audit_wal::{
    AuditWal, AuditWalAppendCommand, AuditWalRecord, AuditWalWriter, AuditWalWriterControl,
    OracleAuditWalConfig,
};

static TEMP_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl From<&crate::config::OracleRuntimeConfig> for OracleAuditWalConfig {
    /// Projects server configuration into the WAL owner's validated bounds.
    fn from(config: &crate::config::OracleRuntimeConfig) -> Self {
        Self {
            audit_wal_root: config.audit_wal_root.clone(),
            audit_wal_max_records: config.audit_wal_max_records,
            audit_wal_max_bytes: config.audit_wal_max_bytes,
            audit_wal_max_age_seconds: config.audit_wal_max_age_seconds,
            audit_relay_batch_records: config.audit_relay_batch_records,
            audit_relay_attempt_timeout_ms: config.audit_relay_attempt_timeout_ms,
            audit_relay_backoff_initial_ms: config.audit_relay_backoff_initial_ms,
            audit_relay_backoff_max_ms: config.audit_relay_backoff_max_ms,
            audit_relay_shutdown_timeout_ms: config.audit_relay_shutdown_timeout_ms,
        }
    }
}

/// Report returned when an Oracle audit publisher is asked to stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditShutdownReport {
    /// Number of records checkpointed during shutdown.
    pub relayed: u64,
    /// Number of records still durable in the local WAL.
    pub backlog_records: u64,
    /// Bytes still durable in the local WAL.
    pub backlog_bytes: u64,
    /// Age of the oldest remaining accepted record.
    pub oldest_backlog_age: Option<Duration>,
}

/// Closed relay result used by the production audit metric owner.
#[derive(Clone, Copy)]
enum AuditRelayOutcome {
    /// SQL commit and WAL checkpoint both completed.
    Committed,
    /// The record remains durable and will be retried.
    RetriedTransient,
    /// Relay stopped after a durable commit/checkpoint failure.
    Failed,
}

impl AuditRelayOutcome {
    /// Returns the bounded metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::RetriedTransient => "retried_transient",
            Self::Failed => "failed",
        }
    }
}

/// Closed relay failure reason used by the production audit metric owner.
#[derive(Clone, Copy)]
enum AuditRelayFailureReason {
    /// Postgres transaction failed.
    Postgres,
    /// Relay attempt exceeded its timeout.
    Timeout,
    /// WAL checkpoint or framing failed.
    Serialization,
}

impl AuditRelayFailureReason {
    /// Returns the bounded metric label.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Timeout => "timeout",
            Self::Serialization => "serialization",
        }
    }
}

/// Test-support projection of the relay owner's closed metric domains.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditTelemetryLabelDomains {
    /// Relay outcomes emitted by the publisher.
    pub outcomes: [&'static str; 3],
    /// Relay failure reasons emitted by the publisher.
    pub failure_reasons: [&'static str; 3],
}

/// Returns relay label domains directly from the production emitter enums.
#[cfg(feature = "test-support")]
#[must_use]
pub const fn audit_telemetry_label_domains() -> AuditTelemetryLabelDomains {
    AuditTelemetryLabelDomains {
        outcomes: [
            AuditRelayOutcome::Committed.as_str(),
            AuditRelayOutcome::RetriedTransient.as_str(),
            AuditRelayOutcome::Failed.as_str(),
        ],
        failure_reasons: [
            AuditRelayFailureReason::Postgres.as_str(),
            AuditRelayFailureReason::Timeout.as_str(),
            AuditRelayFailureReason::Serialization.as_str(),
        ],
    }
}

/// Guard that pauses the production relay immediately before its SQL attempt.
#[cfg(feature = "test-support")]
pub struct AuditRelayPauseGuard {
    /// Shared control state resumed on guard drop.
    control: std::sync::Arc<RelayControl>,
}

#[cfg(feature = "test-support")]
impl Drop for AuditRelayPauseGuard {
    /// Resumes the sole relay when the test no longer owns the pause guard.
    fn drop(&mut self) {
        self.control.paused.store(false, Ordering::Release);
        self.control.notify.notify_waiters();
    }
}

struct RelayControl {
    /// Pauses relay SQL at the fault-injection seam.
    paused: AtomicBool,
    /// Stops relay after a successful commit before checkpoint advancement.
    fail_after_commit: AtomicBool,
    /// Wakes the relay when a pause guard is dropped.
    notify: Notify,
}

/// Owns local acceptance and the one bounded background relay into Postgres.
pub struct OracleAuditPublisher {
    /// Recoverable local WAL protected by its root lock.
    wal: std::sync::Arc<Mutex<AuditWal>>,
    /// Bounded ingress to the sole grouped WAL writer.
    writer_tx: Mutex<Option<mpsc::Sender<AuditWalAppendCommand>>>,
    /// Retained writer task joined during shutdown.
    writer: Mutex<Option<JoinHandle<()>>>,
    /// One-shot grouped-write failure seam.
    writer_fail_next_group: std::sync::Arc<AtomicBool>,
    /// Writer pause and sync-count controls used by deterministic test seams.
    writer_control: std::sync::Arc<AuditWalWriterControl>,
    /// Shared SQL handle used only by the background relay.
    vala: ValaPostgres,
    /// Cancels relay work during ordered shutdown.
    cancel: CancellationToken,
    /// Wakes relay after a local fsync acknowledgement.
    wake: std::sync::Arc<Notify>,
    /// Test hooks wrapping the production relay state machine.
    control: std::sync::Arc<RelayControl>,
    /// Sole retained relay task handle.
    relay: Mutex<Option<JoinHandle<()>>>,
    /// Bounded retry, batch, and shutdown settings.
    config: OracleAuditWalConfig,
}

impl OracleAuditPublisher {
    /// Opens and recovers the configured root before Oracle role activation.
    pub(crate) fn new(
        vala: ValaPostgres,
        config: OracleAuditWalConfig,
    ) -> Result<std::sync::Arc<Self>, BifrostError> {
        let root = config.audit_wal_root.clone().unwrap_or_else(|| {
            let sequence = TEMP_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            std::env::temp_dir().join(format!(
                "wyrd-oracle-audit-{}-{sequence}",
                std::process::id()
            ))
        });
        let wal = AuditWal::recover(&root, &config)?;
        let wal = std::sync::Arc::new(Mutex::new(wal));
        let writer_capacity = config.audit_wal_max_records.clamp(1, 1024);
        let (writer_tx, writer_fail_next_group, writer_control, writer) =
            AuditWalWriter::spawn(std::sync::Arc::clone(&wal), writer_capacity);
        let cancel = CancellationToken::new();
        let publisher = std::sync::Arc::new(Self {
            wal,
            writer_tx: Mutex::new(Some(writer_tx)),
            writer: Mutex::new(Some(writer)),
            writer_fail_next_group,
            writer_control,
            vala,
            cancel: cancel.clone(),
            wake: std::sync::Arc::new(Notify::new()),
            control: std::sync::Arc::new(RelayControl {
                paused: AtomicBool::new(false),
                fail_after_commit: AtomicBool::new(false),
                notify: Notify::new(),
            }),
            relay: Mutex::new(None),
            config,
        });
        for outcome in [
            AuditRelayOutcome::Committed,
            AuditRelayOutcome::RetriedTransient,
            AuditRelayOutcome::Failed,
        ] {
            metrics::counter!("oracle_audit_relay_total", "outcome" => outcome.as_str())
                .increment(0);
        }
        for reason in [
            AuditRelayFailureReason::Postgres,
            AuditRelayFailureReason::Timeout,
            AuditRelayFailureReason::Serialization,
        ] {
            metrics::counter!(
                "oracle_audit_relay_failures_total",
                "reason" => reason.as_str()
            )
            .increment(0);
        }
        metrics::gauge!("oracle_audit_wal_records").set(0.0);
        metrics::gauge!("oracle_audit_wal_bytes").set(0.0);
        metrics::gauge!("oracle_audit_oldest_record_age_seconds").set(0.0);
        metrics::gauge!("oracle_audit_relay_lag_seconds").set(0.0);
        metrics::gauge!("oracle_audit_relay_batch_size").set(0.0);
        if let Ok(wal) = publisher.wal.try_lock() {
            let (records, bytes, oldest) = wal.snapshot();
            metrics::gauge!("oracle_audit_wal_records").set(records as f64);
            metrics::gauge!("oracle_audit_wal_bytes").set(bytes as f64);
            metrics::gauge!("oracle_audit_oldest_record_age_seconds")
                .set(oldest.map_or(0.0, |age| age.as_secs_f64()));
        }
        let task_owner = std::sync::Arc::clone(&publisher);
        let handle = tokio::spawn(async move { task_owner.relay_loop().await });
        if let Ok(mut slot) = publisher.relay.try_lock() {
            *slot = Some(handle);
        }
        Ok(publisher)
    }

    /// Enqueues a read decision and waits for the local frame fsync.
    pub async fn publish_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        let event = build_event(
            context,
            "bifrost.query.read_decision",
            AuditResult::Success,
            decision.into_detail(),
        );
        self.append(context.data_tenant_id, event).await
    }

    /// Performs one bounded blocking WAL append and wakes the relay.
    async fn append(
        &self,
        tenant: wyrd_spec::DataTenantId,
        event: AuditEvent,
    ) -> Result<(), BifrostError> {
        let started = Instant::now();
        let (ack_tx, ack_rx) = oneshot::channel();
        let command = AuditWalAppendCommand {
            tenant,
            event,
            ack: ack_tx,
        };
        let sender = self
            .writer_tx
            .lock()
            .await
            .as_ref()
            .cloned()
            .ok_or(BifrostError::QueryAuditUnavailable)?;
        sender
            .try_send(command)
            .map_err(|_| BifrostError::QueryAuditUnavailable)?;
        ack_rx
            .await
            .map_err(|_| BifrostError::QueryAuditUnavailable)??;
        metrics::histogram!("oracle_audit_append_duration_seconds")
            .record(started.elapsed().as_secs_f64());
        self.record_wal_metrics().await;
        self.wake.notify_one();
        Ok(())
    }

    /// Publishes the bounded local WAL residual gauges used by operators.
    async fn record_wal_metrics(&self) {
        let (records, bytes, oldest) = self.wal.lock().await.snapshot();
        metrics::gauge!("oracle_audit_wal_records").set(records as f64);
        metrics::gauge!("oracle_audit_wal_bytes").set(bytes as f64);
        metrics::gauge!("oracle_audit_oldest_record_age_seconds")
            .set(oldest.map_or(0.0, |age| age.as_secs_f64()));
    }

    /// Drains accepted records until the caller's deadline and reports residue.
    pub async fn shutdown(&self, deadline: Instant) -> AuditShutdownReport {
        let deadline = effective_shutdown_deadline(
            Instant::now(),
            deadline,
            self.config.audit_relay_shutdown_timeout_ms,
        );
        let before = self.wal.lock().await.snapshot();
        self.writer_tx.lock().await.take();
        let mut writer = self.writer.lock().await;
        if let Some(mut task) = writer.take() {
            let _ =
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut task).await;
            if !task.is_finished() {
                task.abort();
            }
        }
        self.wake.notify_waiters();
        self.cancel.cancel();
        let mut slot = self.relay.lock().await;
        if let Some(mut task) = slot.take() {
            let _ =
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), &mut task).await;
            if !task.is_finished() {
                task.abort();
            }
        }
        let snapshot = self.wal.lock().await.snapshot();
        self.record_wal_metrics().await;
        AuditShutdownReport {
            relayed: before.0.saturating_sub(snapshot.0),
            backlog_records: snapshot.0,
            backlog_bytes: snapshot.1,
            oldest_backlog_age: snapshot.2,
        }
    }

    /// Aborts background WAL tasks so an abrupt test restart releases the root lock.
    #[cfg(feature = "test-support")]
    pub async fn abort_for_test(&self) {
        self.writer_tx.lock().await.take();
        if let Some(task) = self.writer.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        self.cancel.cancel();
        self.wake.notify_waiters();
        if let Some(task) = self.relay.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
    }

    /// Pauses the production relay before its next Postgres transaction.
    #[cfg(feature = "test-support")]
    pub fn pause_relay_before_postgres(&self) -> AuditRelayPauseGuard {
        self.control.paused.store(true, Ordering::Release);
        AuditRelayPauseGuard {
            control: std::sync::Arc::clone(&self.control),
        }
    }

    /// Terminates the relay after its next successful Postgres commit.
    #[cfg(feature = "test-support")]
    pub fn fail_after_next_postgres_commit_before_checkpoint(&self) {
        self.control
            .fail_after_commit
            .store(true, Ordering::Release);
    }

    /// Forces the next grouped local sync to fan out a durable failure.
    #[cfg(feature = "test-support")]
    pub fn fail_next_wal_group_for_test(&self) {
        self.writer_fail_next_group.store(true, Ordering::Release);
    }

    /// Pauses the sole writer before it receives a group for saturation tests.
    #[cfg(feature = "test-support")]
    pub fn pause_writer_before_group_for_test(&self) -> super::audit_wal::AuditWalWriterPauseGuard {
        self.writer_control.paused.store(true, Ordering::Release);
        super::audit_wal::AuditWalWriterPauseGuard {
            control: std::sync::Arc::clone(&self.writer_control),
        }
    }

    /// Returns the number of completed grouped append-and-sync operations.
    #[cfg(feature = "test-support")]
    pub fn writer_sync_count_for_test(&self) -> u64 {
        self.writer_control.sync_count.load(Ordering::Acquire)
    }

    /// Returns the pending record, byte, and age snapshot used by restart tests.
    #[cfg(feature = "test-support")]
    pub fn wal_snapshot(&self) -> (u64, u64, Option<Duration>) {
        self.wal
            .try_lock()
            .map_or((0, 0, None), |wal| wal.snapshot())
    }

    /// Relays bounded snapshots and checkpoints only after SQL commit.
    async fn relay_loop(self: std::sync::Arc<Self>) {
        let mut backoff = Duration::from_millis(self.config.audit_relay_backoff_initial_ms);
        loop {
            let records = self
                .wal
                .lock()
                .await
                .pending(self.config.audit_relay_batch_records);
            metrics::gauge!("oracle_audit_relay_batch_size").set(records.len() as f64);
            if records.is_empty() {
                if self.cancel.is_cancelled() {
                    return;
                }
                tokio::select! { _ = self.wake.notified() => {}, _ = self.cancel.cancelled() => {} }
                continue;
            }
            let mut progressed = false;
            for record in records {
                while self.control.paused.load(Ordering::Acquire) {
                    self.control.notify.notified().await;
                }
                let result = tokio::time::timeout(
                    Duration::from_millis(self.config.audit_relay_attempt_timeout_ms),
                    relay_record(&self.vala, &record),
                )
                .await;
                match result {
                    Ok(Ok(())) => {
                        if self.control.fail_after_commit.swap(false, Ordering::AcqRel) {
                            return;
                        }
                        if self.wal.lock().await.checkpoint(record.lsn).is_err() {
                            record_relay_metric(
                                AuditRelayOutcome::Failed,
                                Some(AuditRelayFailureReason::Serialization),
                            );
                            return;
                        }
                        record_relay_metric(AuditRelayOutcome::Committed, None);
                        metrics::gauge!("oracle_audit_relay_lag_seconds")
                            .set(record_age_seconds(&record).unwrap_or_default());
                        self.record_wal_metrics().await;
                        progressed = true;
                        backoff = Duration::from_millis(self.config.audit_relay_backoff_initial_ms);
                    }
                    Err(_) => {
                        record_relay_metric(
                            AuditRelayOutcome::RetriedTransient,
                            Some(AuditRelayFailureReason::Timeout),
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_millis(
                            self.config.audit_relay_backoff_max_ms,
                        ));
                        break;
                    }
                    Ok(Err(())) => {
                        record_relay_metric(
                            AuditRelayOutcome::RetriedTransient,
                            Some(AuditRelayFailureReason::Postgres),
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_millis(
                            self.config.audit_relay_backoff_max_ms,
                        ));
                        break;
                    }
                }
            }
            if !progressed && self.cancel.is_cancelled() {
                return;
            }
        }
    }
}

/// Computes accepted-record age for the relay lag gauge.
fn record_age_seconds(record: &AuditWalRecord) -> Option<f64> {
    let accepted = u64::try_from(record.accepted_at_micros).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_micros() as u64;
    Some(now.saturating_sub(accepted) as f64 / 1_000_000.0)
}

/// Emits one bounded relay outcome and optional failure reason.
fn record_relay_metric(outcome: AuditRelayOutcome, reason: Option<AuditRelayFailureReason>) {
    metrics::counter!("oracle_audit_relay_total", "outcome" => outcome.as_str()).increment(1);
    if let Some(reason) = reason {
        metrics::counter!("oracle_audit_relay_failures_total", "reason" => reason.as_str())
            .increment(1);
    }
}

#[async_trait]
impl OracleAudit for OracleAuditPublisher {
    /// Accepts the immutable read decision in the local WAL before any row read.
    async fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        self.publish_read_decision(context, decision).await
    }

    /// Accepts a verified security violation in the local WAL.
    async fn append_security_violation(
        &self,
        context: VerifiedSecurityContext,
        violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        let event = build_event(
            &context.query,
            "bifrost.query.security_violation",
            AuditResult::Failure,
            AuditDetail::BifrostSecurityViolation {
                violation: violation.violation,
                phase: violation.phase,
                query_digest: context.query_digest,
            },
        );
        self.append(context.query.data_tenant_id, event).await
    }
}

/// Caps a caller deadline at the configured shutdown drain window.
fn effective_shutdown_deadline(
    started_at: Instant,
    caller_deadline: Instant,
    timeout_ms: u64,
) -> Instant {
    caller_deadline.min(started_at + Duration::from_millis(timeout_ms))
}

/// Builds the scrubbed event shared by local read and violation acceptance.
fn build_event(
    context: &AuthorizedQueryContext,
    operation: &str,
    result: AuditResult,
    detail: AuditDetail,
) -> AuditEvent {
    AuditEvent::new(
        context.request_id.clone(),
        context.trace_id.clone(),
        operation.to_owned(),
        "bifrost.query".to_owned(),
        context.principal.card_ref().cloned(),
        context.principal.id,
        context.principal.kind.tag(),
        context.auth_method,
        context.permission.clone(),
        AuditDecision::Allow,
        result,
        "scrubbed Bifrost query decision".to_owned(),
    )
    .with_detail(detail)
}

/// Appends and commits one tenant event through the canonical SQL writer.
async fn relay_record(vala: &ValaPostgres, record: &AuditWalRecord) -> Result<(), ()> {
    let mut conn = vala.tenant_conn(record.tenant).await.map_err(|_| ())?;
    vala_sql::queries::audit_outbox::append_audit(&mut conn, &record.event)
        .await
        .map_err(|_| ())?;
    conn.commit().await.map_err(|_| ())
}

#[cfg(all(test, feature = "test-support"))]
mod pg_tests {
    use std::time::{Duration, Instant};

    use vala_bifrost_redux::oracle::AuthorizedQueryContext;
    use vala_bifrost_redux::oracle::BifrostQueryReadDecision;
    use wyrd_runtime::permission::{Permission, PermissionSet};
    use wyrd_runtime::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuditDetail, AuthMethod, QueryAuditDigest, QueryClass, QueryExecutionMode, VisibilityMode,
    };

    use super::*;

    /// Builds the valid tenant-bound context used by relay integration tests.
    fn context(tenant: wyrd_spec::DataTenantId) -> AuthorizedQueryContext {
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_query_read()]),
        );
        AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            Permission::bifrost_query_read().to_string(),
        )
        .expect("matching tenant context")
    }

    /// Builds one valid read decision accepted by the production publisher.
    fn decision() -> BifrostQueryReadDecision {
        let digest = |value: &str| QueryAuditDigest::new(value).expect("valid digest");
        BifrostQueryReadDecision::try_new(AuditDetail::BifrostQueryReadDecision {
            query_digest: digest("sha256:query"),
            query_class: QueryClass::Interactive,
            visibility: VisibilityMode::PublishedOnly,
            binding_digests: vec![digest("sha256:binding")],
            snapshot_digest: digest("sha256:snapshot"),
            manifest_digest: digest("sha256:manifest"),
            projection_digest: digest("sha256:projection"),
            permission_digest: digest("sha256:permission"),
            execution: QueryExecutionMode::Local,
            selected_node_count: 1,
            worker_count: 0,
            slot_units: 1,
            retry_ordinal: 0,
            deadline_ms: 1_000,
        })
        .expect("valid decision")
    }

    /// Runs one async test on the process-lifetime runtime used by PgFixture.
    fn run<F: std::future::Future<Output = ()>>(future: F) {
        wyrd_runtime::runtime().block_on(future);
    }

    #[test]
    /// Proves a locally fsynced tenant event reaches the canonical outbox.
    fn oracle_audit_relay_appends_tenant_scoped_event() {
        run(async {
            let vala = crate::test_support::test_vala_postgres().await;
            let tenant = crate::test_support::test_tenant().await;
            let root = tempfile::tempdir().expect("WAL root");
            let config = OracleAuditWalConfig {
                audit_wal_root: Some(root.path().to_owned()),
                ..OracleAuditWalConfig::default()
            };
            let publisher = OracleAuditPublisher::new(vala.clone(), config).expect("publisher");
            publisher
                .publish_read_decision(&context(tenant), decision())
                .await
                .expect("local acceptance");
            tokio::time::sleep(Duration::from_millis(250)).await;
            let mut conn = vala.tenant_conn(tenant).await.expect("tenant connection");
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE operation = 'bifrost.query.read_decision'").fetch_one(&mut **conn.transaction()).await.expect("audit count");
            assert_eq!(count, 1);
            publisher
                .shutdown(Instant::now() + Duration::from_secs(1))
                .await;
        });
    }

    #[test]
    /// Proves a commit-before-checkpoint crash window replays an accepted event.
    fn oracle_audit_relay_replays_uncheckpointed_record_pg() {
        run(async {
            let vala = crate::test_support::test_vala_postgres().await;
            let tenant = crate::test_support::test_tenant().await;
            let root = tempfile::tempdir().expect("WAL root");
            let config = OracleAuditWalConfig {
                audit_wal_root: Some(root.path().to_owned()),
                ..OracleAuditWalConfig::default()
            };
            let publisher =
                OracleAuditPublisher::new(vala.clone(), config.clone()).expect("publisher");
            let _pause = publisher.pause_relay_before_postgres();
            publisher
                .publish_read_decision(&context(tenant), decision())
                .await
                .expect("local acceptance");
            publisher.fail_after_next_postgres_commit_before_checkpoint();
            drop(_pause);
            tokio::time::sleep(Duration::from_millis(250)).await;
            publisher
                .shutdown(Instant::now() + Duration::from_secs(1))
                .await;
            drop(publisher);
            let restarted = OracleAuditPublisher::new(vala.clone(), config).expect("restart");
            tokio::time::sleep(Duration::from_millis(250)).await;
            let mut conn = vala.tenant_conn(tenant).await.expect("tenant connection");
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM vala.audit_outbox WHERE operation = 'bifrost.query.read_decision'").fetch_one(&mut **conn.transaction()).await.expect("audit count");
            assert!(
                count >= 2,
                "commit/checkpoint window replays a valid duplicate"
            );
            restarted
                .shutdown(Instant::now() + Duration::from_secs(1))
                .await;
        });
    }

    #[test]
    /// Proves relay delivery preserves the existing tenant hash-chain order.
    fn oracle_audit_relay_preserves_hash_chain_pg() {
        run(async {
            let vala = crate::test_support::test_vala_postgres().await;
            let tenant = crate::test_support::test_tenant().await;
            let root = tempfile::tempdir().expect("WAL root");
            let config = OracleAuditWalConfig {
                audit_wal_root: Some(root.path().to_owned()),
                ..OracleAuditWalConfig::default()
            };
            let publisher = OracleAuditPublisher::new(vala.clone(), config).expect("publisher");
            publisher
                .publish_read_decision(&context(tenant), decision())
                .await
                .expect("first acceptance");
            publisher
                .publish_read_decision(&context(tenant), decision())
                .await
                .expect("second acceptance");
            tokio::time::sleep(Duration::from_millis(300)).await;
            let mut conn = vala.tenant_conn(tenant).await.expect("tenant connection");
            let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = sqlx::query_as("SELECT seq, prev_hash, entry_hash FROM vala.audit_outbox WHERE operation = 'bifrost.query.read_decision' ORDER BY seq DESC LIMIT 2").fetch_all(&mut **conn.transaction()).await.expect("chain rows");
            assert!(rows.len() >= 2);
            assert_eq!(rows[0].1, rows[1].2, "newest prev hash links prior entry");
            publisher
                .shutdown(Instant::now() + Duration::from_secs(1))
                .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Proves a caller deadline later than configuration is capped by the owner.
    fn shutdown_deadline_caps_later_caller_deadline() {
        let started = Instant::now();
        let caller = started + Duration::from_secs(30);
        assert_eq!(
            effective_shutdown_deadline(started, caller, 10),
            started + Duration::from_millis(10)
        );
    }

    #[test]
    /// Proves an earlier caller deadline is preserved and never extended.
    fn shutdown_deadline_preserves_earlier_caller_deadline() {
        let started = Instant::now();
        let caller = started + Duration::from_millis(2);
        assert_eq!(effective_shutdown_deadline(started, caller, 10), caller);
    }
}
