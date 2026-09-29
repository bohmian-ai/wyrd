//! Server-owned publication of staged audit events into retained history.
//!
//! Staging is transient write-ahead state: [`AuditPublisher`] is the one owner
//! that moves a tenant's frozen audit range into `vala.system.audit_log`
//! through the local Scribe and then drains what it published. The cycle is
//! engine-internal — it evaluates no permission and therefore appends no audit
//! event of its own — and only the complete freeze → publish → settle cycle is
//! callable, so staging can never be drained without a durable Scribe
//! acceptance for the same range.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::{
    CanonicalIngress, IngressPayload, Scribe as ScribeIngest, ScribeIngressFrame,
};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::ScribeImpl;
use vala_bifrost_redux::tables::AuditLogTable;
use vala_bifrost_redux::tables::DomainTable;
use vala_bifrost_redux::tables::audit::projection::project_audit_rows;
use vala_sql::ValaPostgres;
use vala_sql::queries::audit_staging::{
    AuditPublicationRange, freeze_publication_range, list_publication_range, settle_publication,
};
use vala_sql::row_types::audit_staging::AuditStagingRow;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::OperatorPool;

use crate::state::AppState;

/// Events moved by one tenant cycle.
///
/// The bound exists so one busy tenant cannot monopolize a sweep, and so a
/// shipment stays well inside the ingest frame limit. A tenant with a deeper
/// backlog drains it over consecutive sweeps.
const PUBLICATION_BATCH_RECORDS: i64 = 512;

/// Delay between sweeps of the tenant directory.
const PUBLICATION_INTERVAL: Duration = Duration::from_secs(5);

/// Tenant cycles the publisher runs at once, across overlapping sweeps.
///
/// Tenants publish concurrently so one tenant waiting on its chain head,
/// Scribe, or Postgres cannot stall every tenant behind it. The bound is fixed
/// rather than configurable: it exists to keep the publisher's demand on the
/// Vala pool and the local Scribe predictable, so a deeper tenant directory
/// waits for a free slot instead of opening unbounded work.
const PUBLICATION_TENANT_CONCURRENCY: usize = 8;

/// What one tenant's publication cycle did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    /// The tenant has no event owed to retained history.
    Idle,
    /// The inclusive range was published and drained.
    Published {
        /// First published sequence number.
        seq_lo: i64,
        /// Last published sequence number.
        seq_hi: i64,
        /// Rows actually removed by the watermark advance.
        retired: u64,
    },
}

/// Server-owned publisher moving audit events into retained history.
///
/// It owns the pools and the local Scribe it publishes through, so a cycle is a
/// method on the owner rather than a function threading four dependencies.
/// Publication is engine-internal: it evaluates no permission, never calls
/// Gate, and therefore appends no further audit event.
pub struct AuditPublisher {
    /// Vala warehouse owner every tenant-scoped staging transaction is opened
    /// through.
    postgres: ValaPostgres,
    /// Cross-tenant pool the admin-owned tenant directory is read through.
    ///
    /// `platform.tenants` is not tenant data and grants no read to the
    /// application role, so a sweep that listed tenants on `pool` would be
    /// refused by Postgres and service nobody.
    directory: OperatorPool,
    /// Local durable Scribe the shipment is appended through.
    scribe: Arc<ScribeImpl>,
    /// Maximum events moved by one tenant cycle.
    batch_records: i64,
    /// Delay between sweeps of the tenant directory.
    interval: Duration,
    /// Test-only report of each range this instance durably appended.
    ///
    /// Set only through [`AuditPublisher::observe_appends`], so a journey can
    /// name the cycle that reached Scribe before settlement even while the
    /// server's own sweep publishes the same tenant. Production never sets it.
    #[cfg(feature = "test-support")]
    appended: Option<tokio::sync::watch::Sender<Option<AuditPublicationRange>>>,
    /// Test-only pause between reading a frozen range and appending it.
    ///
    /// Set only through [`AuditPublisher::pause_before_append`]. The sender
    /// reports arrival at the pause; the cycle resumes once the semaphore
    /// grants it a permit. Production never sets it.
    #[cfg(feature = "test-support")]
    append_pause: Option<(
        tokio::sync::watch::Sender<bool>,
        Arc<tokio::sync::Semaphore>,
    )>,
}

impl AuditPublisher {
    /// Build the publisher for a serving state that owns a local Scribe.
    ///
    /// Returns `None` for a role with no local Scribe — an Oracle-only or
    /// Forge-worker-only process has no durable seam to publish through and
    /// must not claim the work. It also returns `None` without a cross-tenant
    /// operator pool, since the tenant directory is unreadable from the
    /// application role.
    #[must_use]
    pub fn from_state(state: &AppState) -> Option<Self> {
        let scribe = Arc::clone(state.bifrost_ingest()?.scribe());
        let directory = state.postgres.operator_pool().or_else(|| {
            tracing::warn!("audit publication skipped because operator pool is unavailable");
            None
        })?;
        Some(Self {
            postgres: state.postgres.vala().clone(),
            directory,
            scribe,
            batch_records: PUBLICATION_BATCH_RECORDS,
            interval: PUBLICATION_INTERVAL,
            #[cfg(feature = "test-support")]
            appended: None,
            #[cfg(feature = "test-support")]
            append_pause: None,
        })
    }

    /// Hold every cycle of this instance after it reads its frozen range and
    /// before its Scribe append.
    ///
    /// The receiver turns `true` once a cycle has read its range and is
    /// waiting; each permit added to the returned semaphore releases one
    /// waiting cycle. A test uses it to order this instance's append after
    /// another actor's crash while the range read already happened ahead of
    /// any settlement. Only this instance pauses; the server's own sweep is
    /// built separately and never waits. Calling it again replaces the pause.
    #[cfg(feature = "test-support")]
    pub fn pause_before_append(
        &mut self,
    ) -> (
        tokio::sync::watch::Receiver<bool>,
        Arc<tokio::sync::Semaphore>,
    ) {
        let (reached, receiver) = tokio::sync::watch::channel(false);
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        self.append_pause = Some((reached, Arc::clone(&release)));
        (receiver, release)
    }

    /// Report every range this instance durably appends, before it settles.
    ///
    /// The receiver observes the last range whose Scribe append returned, so a
    /// test can abort that exact cycle in the window between its durable append
    /// and its settlement. Only this instance reports; the server's own sweep
    /// is built separately and stays silent. Calling it again replaces the
    /// previous observer.
    #[cfg(feature = "test-support")]
    pub fn observe_appends(
        &mut self,
    ) -> tokio::sync::watch::Receiver<Option<AuditPublicationRange>> {
        let (sender, receiver) = tokio::sync::watch::channel(None);
        self.appended = Some(sender);
        receiver
    }

    /// Run bounded publication sweeps until cancelled.
    ///
    /// A failing tenant is logged and retried on a later sweep: its rows stay
    /// durable in Postgres and its frozen bound stays set, so a transient
    /// Scribe or Postgres failure delays retained history rather than losing it
    /// or changing the in-flight batch identity.
    ///
    /// Each tenant cycle runs as its own task in [`TenantCycles`], which
    /// outlives the sweep that started it. A sweep never waits for a cycle to
    /// finish, only for a free slot, so a cycle blocked mid-settlement on a
    /// held chain head or staging row delays its own tenant and nobody else:
    /// later sweeps keep re-listing the directory and publishing every other
    /// tenant.
    ///
    /// Cancellation is observed between sweeps. Returning drops the in-flight
    /// cycles, which aborts them; partial progress is safe by construction:
    /// every effect is either committed or absent, a frozen bound survives to
    /// be reused verbatim, and a range appended but not settled is republished
    /// into Scribe's batch fence on the next run. Nothing is retried inside one
    /// cycle; a later sweep is the retry.
    pub async fn run(self, shutdown: CancellationToken) {
        let publisher = Arc::new(self);
        let mut cycles = TenantCycles::new(PUBLICATION_TENANT_CONCURRENCY);
        let mut ticks = tokio::time::interval(publisher.interval);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticks.tick() => publisher.sweep(&mut cycles).await,
            }
        }
    }

    /// Start one bounded publication cycle for every live tenant without one.
    ///
    /// Cycles run unordered, at most [`PUBLICATION_TENANT_CONCURRENCY`] at a
    /// time across sweeps, so total demand on the Vala pool and the local
    /// Scribe stays inside one fixed bound. A tenant whose previous cycle is
    /// still running is skipped rather than doubled: it owes the same frozen
    /// range, which that cycle is already publishing. The sweep returns once
    /// every listed tenant has a cycle, without waiting for any to finish. A
    /// failing cycle is logged and left to a later sweep.
    async fn sweep(self: &Arc<Self>, cycles: &mut TenantCycles) {
        let tenants =
            match wyrd_sql::queries::platform::tenants::list_active_tenant_ids(&self.directory)
                .await
            {
                Ok(tenants) => tenants,
                Err(error) => {
                    tracing::warn!(error = %error, "audit publisher could not list tenants");
                    return;
                }
            };
        cycles.reap();
        for tenant in tenants {
            if cycles.running(tenant) {
                continue;
            }
            cycles.await_slot().await;
            let publisher = Arc::clone(self);
            cycles.start(
                tenant,
                async move { publisher.publish_logged(tenant).await },
            );
        }
    }

    /// Run one tenant cycle, reporting a failure instead of propagating it.
    ///
    /// A sweep services every tenant it listed, so one tenant's transient
    /// Scribe or Postgres failure must not cancel the rest of the sweep.
    async fn publish_logged(&self, tenant: DataTenantId) {
        if let Err(error) = self.publish_tenant(tenant).await {
            tracing::warn!(
                %tenant,
                error = %error,
                "audit publication cycle failed; retained history will retry"
            );
        }
    }

    /// Move one tenant's frozen audit range into retained history, then settle it.
    ///
    /// The freeze transaction closes before the durable append, so a slow
    /// publication never holds a tenant transaction or the audit-chain append
    /// lock open, and the settlement runs in its own transaction once the
    /// append is durable.
    ///
    /// # Errors
    /// Returns the stable failure raised by freezing the range, reading it, the
    /// content projection, the durable append, or the settlement. A failure
    /// after a durable append leaves the bound frozen, so the next cycle
    /// republishes the identical range into Scribe's dedup fence and settles it
    /// then.
    pub async fn publish_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<PublishOutcome, AuditPublicationError> {
        let Some(range) = self.freeze(tenant).await? else {
            return Ok(PublishOutcome::Idle);
        };
        self.publish_range(tenant, range).await?;
        let retired = self.settle(tenant, range.seq_hi).await?;
        Ok(PublishOutcome::Published {
            seq_lo: range.seq_lo,
            seq_hi: range.seq_hi,
            retired,
        })
    }

    /// Durably append exactly one frozen range, without freezing or settling.
    ///
    /// This is the half of a cycle that must survive being repeated. Because
    /// `range` comes from the persisted in-flight bound, a competing replica or
    /// a restart after an uncertain append re-derives the same tenant and
    /// bounds, projects the same rows, and therefore presents the same batch
    /// identity — landing on Scribe's durable batch fence instead of
    /// duplicating retained history. Rows appended above `seq_hi` are not read
    /// and wait for the next batch.
    ///
    /// A range whose rows are already gone is treated as published rather than
    /// as a failure. Staging is only ever deleted through the watermark, so an
    /// empty frozen range means a competing replica published and settled this
    /// exact range between this cycle's freeze and its read. Settling it again
    /// is inert under the `GREATEST` and matching-bound guards, while refusing
    /// would leave the tenant erroring on a range nobody still owes.
    ///
    /// # Errors
    /// Returns [`AuditPublicationError::Staging`] when reading the range fails,
    /// [`AuditPublicationError::Projection`] for non-contiguous or invalid
    /// content, and [`AuditPublicationError::Publish`] when Scribe refuses the
    /// append.
    async fn publish_range(
        &self,
        tenant: DataTenantId,
        range: AuditPublicationRange,
    ) -> Result<(), AuditPublicationError> {
        let rows = self.read_range(tenant, range).await?;
        #[cfg(feature = "test-support")]
        if let Some((reached, release)) = &self.append_pause {
            reached.send_replace(true);
            if let Ok(permit) = release.acquire().await {
                permit.forget();
            }
        }
        if rows.is_empty() {
            tracing::debug!(
                %tenant,
                seq_lo = range.seq_lo,
                seq_hi = range.seq_hi,
                "frozen audit range was already published and drained by another replica"
            );
            return Ok(());
        }
        let projection = project_audit_rows(tenant, &rows)
            .map_err(|error| AuditPublicationError::Projection(error.to_string()))?;
        self.scribe
            .ingest_frame(ScribeIngressFrame {
                principal: publisher_principal(tenant),
                authenticated_tenant: tenant,
                table: TableRef::new(BifrostNamespace::Audit, AuditLogTable::NAME),
                expected_schema_fingerprint: None,
                request_id: RequestId::now_v7(),
                batch_id: projection.batch_id,
                measured_wire_bytes: projection.rows.get_array_memory_size(),
                payload: IngressPayload::Canonical(CanonicalIngress::unreserved(vec![
                    projection.rows,
                ])),
            })
            .await
            .map_err(|error| AuditPublicationError::Publish(format!("{error:?}")))?;
        #[cfg(feature = "test-support")]
        if let Some(appended) = &self.appended {
            appended.send_replace(Some(range));
        }
        Ok(())
    }

    /// Freeze, or reuse, the one range this tenant owes retained history.
    ///
    /// The short transaction commits before any Scribe IO, so the durable bound
    /// is progress state rather than a lease. `None` means the tenant owes
    /// nothing and the cycle is idle.
    ///
    /// # Errors
    /// Returns [`AuditPublicationError::Staging`] when the tenant transaction
    /// cannot be acquired, the locked chain-head read times out or fails, the
    /// bound update fails, or the commit fails. Nothing is frozen unless the
    /// commit succeeds; a failed transaction rolls back on drop and the next
    /// sweep retries the unchanged range.
    async fn freeze(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<AuditPublicationRange>, AuditPublicationError> {
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| AuditPublicationError::Staging(error.to_string()))?;
        let range = freeze_publication_range(&mut conn, self.batch_records)
            .await
            .map_err(|error| AuditPublicationError::Staging(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| AuditPublicationError::Staging(error.to_string()))?;
        Ok(range)
    }

    /// Read exactly the staged rows of one frozen range.
    ///
    /// Rows appended above `range.seq_hi` are invisible here, which is what
    /// keeps a growing staging tail out of an in-flight batch.
    ///
    /// # Errors
    /// Returns [`AuditPublicationError::Staging`] when the tenant transaction
    /// cannot be acquired, the range read fails or RLS rejects it, or the
    /// read-only commit fails.
    async fn read_range(
        &self,
        tenant: DataTenantId,
        range: AuditPublicationRange,
    ) -> Result<Vec<AuditStagingRow>, AuditPublicationError> {
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| AuditPublicationError::Staging(error.to_string()))?;
        let rows = list_publication_range(&mut conn, range)
            .await
            .map_err(|error| AuditPublicationError::Staging(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| AuditPublicationError::Staging(error.to_string()))?;
        Ok(rows)
    }

    /// Advance the tenant watermark past the published range, release the
    /// matching in-flight bound, and drain through the watermark in one
    /// transaction.
    ///
    /// # Errors
    /// Returns [`AuditPublicationError::Retire`] when the settlement
    /// transaction cannot be acquired, applied, or committed. Nothing is
    /// removed unless all three effects commit together.
    async fn settle(
        &self,
        tenant: DataTenantId,
        seq_hi: i64,
    ) -> Result<u64, AuditPublicationError> {
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| AuditPublicationError::Retire(error.to_string()))?;
        let retired = settle_publication(&mut conn, seq_hi)
            .await
            .map_err(|error| AuditPublicationError::Retire(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| AuditPublicationError::Retire(error.to_string()))?;
        Ok(retired)
    }
}

/// Tenant publication cycles that are still running, bounded and keyed by tenant.
///
/// The publisher's run loop owns it so cycles outlive the sweep that started
/// them: a sweep waits for a free slot, never for a particular cycle, and so a
/// cycle blocked on one tenant's locks cannot hold every later sweep. The
/// tenant map is what lets a sweep skip a tenant that already has a cycle.
/// Dropping it aborts every cycle still running.
struct TenantCycles {
    /// Running cycles; each task yields nothing and logs its own failure.
    tasks: tokio::task::JoinSet<()>,
    /// Tenant each running task publishes, keyed by its task id so a panicked
    /// or aborted task still frees its tenant.
    tenants: std::collections::HashMap<tokio::task::Id, DataTenantId>,
    /// Maximum cycles running at once.
    limit: usize,
}

impl TenantCycles {
    /// Build an empty set admitting at most `limit` concurrent cycles.
    fn new(limit: usize) -> Self {
        Self {
            tasks: tokio::task::JoinSet::new(),
            tenants: std::collections::HashMap::new(),
            limit,
        }
    }

    /// Forget every cycle that has already finished, without waiting.
    fn reap(&mut self) {
        while let Some(done) = self.tasks.try_join_next_with_id() {
            self.forget(done);
        }
    }

    /// Whether `tenant` still has a cycle running.
    fn running(&self, tenant: DataTenantId) -> bool {
        self.tenants.values().any(|running| *running == tenant)
    }

    /// Wait until fewer than `limit` cycles are running.
    ///
    /// This is the only wait a sweep performs on cycles, and it ends as soon
    /// as any one cycle finishes; it can stall only when every slot holds a
    /// blocked tenant, which is the fixed demand bound working as intended.
    async fn await_slot(&mut self) {
        while self.tasks.len() >= self.limit {
            let Some(done) = self.tasks.join_next_with_id().await else {
                return;
            };
            self.forget(done);
        }
    }

    /// Spawn `cycle` as the running cycle for `tenant`.
    fn start(
        &mut self,
        tenant: DataTenantId,
        cycle: impl std::future::Future<Output = ()> + Send + 'static,
    ) {
        let id = self.tasks.spawn(cycle).id();
        self.tenants.insert(id, tenant);
    }

    /// Drop the tenant entry of one joined task, logging a panicked cycle.
    fn forget(&mut self, done: Result<(tokio::task::Id, ()), tokio::task::JoinError>) {
        let id = match done {
            Ok((id, ())) => id,
            Err(error) => {
                tracing::warn!(error = %error, "audit publication cycle ended abnormally");
                error.id()
            }
        };
        self.tenants.remove(&id);
    }
}

/// Stable failures raised by one publication cycle.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuditPublicationError {
    /// Freezing or reading the owed staging range failed.
    #[error("audit staging read failed: {0}")]
    Staging(String),
    /// The claimed rows could not be projected into canonical content.
    #[error("audit projection failed: {0}")]
    Projection(String),
    /// The durable append was refused.
    #[error("audit publication failed: {0}")]
    Publish(String),
    /// Settling a published range failed.
    #[error("audit drain failed: {0}")]
    Retire(String),
}

/// Build the internal principal that owns one tenant's publication.
///
/// The principal carries no permission: publication is engine-internal and
/// evaluates none, so an empty set is the honest description of its authority.
fn publisher_principal(tenant: DataTenantId) -> Principal {
    Principal::new(
        PLATFORM_AUDIT_PRINCIPAL,
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

/// Unit coverage for the run loop's cycle bookkeeping.
#[cfg(test)]
mod tests {
    use super::TenantCycles;
    use wyrd_spec::DataTenantId;

    /// A cycle that never finishes keeps only its own tenant out of a sweep.
    ///
    /// The blocked cycle stands in for a settlement parked on a held chain
    /// head: it must not stop a free slot from being granted, a second
    /// tenant's cycle from finishing and being reaped, or the blocked tenant
    /// from being reported as still running.
    #[tokio::test]
    async fn a_blocked_cycle_holds_only_its_own_tenant() {
        let mut cycles = TenantCycles::new(2);
        let blocked = DataTenantId::new_v7();
        let healthy = DataTenantId::new_v7();
        cycles.start(blocked, std::future::pending());
        cycles.await_slot().await;
        let (finished, done) = tokio::sync::oneshot::channel();
        cycles.start(healthy, async move {
            let _ = finished.send(());
        });
        done.await.expect("the healthy cycle runs to completion");
        tokio::task::yield_now().await;
        cycles.reap();
        cycles.await_slot().await;
        assert!(
            cycles.running(blocked),
            "the blocked tenant stays in flight"
        );
        assert!(!cycles.running(healthy), "the finished tenant is reaped");
    }
}
