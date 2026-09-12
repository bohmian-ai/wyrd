use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
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
use vala_sql::TenantConn;
use vala_sql::queries::audit_staging::{
    AuditPublicationRange, freeze_publication_range, list_publication_range, settle_publication,
};
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
    /// RLS-enforced application pool used for tenant-scoped work.
    pool: PgPool,
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
            pool: state.postgres.wyrd().app_pool().clone(),
            directory,
            scribe,
            batch_records: PUBLICATION_BATCH_RECORDS,
            interval: PUBLICATION_INTERVAL,
        })
    }

    /// Run bounded publication sweeps until cancelled.
    ///
    /// A failing tenant is logged and retried on the next sweep: its rows stay
    /// durable in Postgres and its frozen bound stays set, so a transient
    /// Scribe or Postgres failure delays retained history rather than losing it
    /// or changing the in-flight batch identity.
    pub async fn run(self, shutdown: CancellationToken) {
        let mut ticks = tokio::time::interval(self.interval);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = ticks.tick() => self.sweep().await,
            }
        }
    }

    /// Publish one bounded batch for every live tenant.
    async fn sweep(&self) {
        let tenants = match wyrd_sql::queries::platform::tenants::list_active_tenant_ids(
            self.directory.pool(),
        )
        .await
        {
            Ok(tenants) => tenants,
            Err(error) => {
                tracing::warn!(error = %error, "audit publisher could not list tenants");
                return;
            }
        };
        for tenant in tenants {
            if let Err(error) = self.publish_tenant(tenant).await {
                tracing::warn!(
                    %tenant,
                    error = %error,
                    "audit publication cycle failed; retained history will retry"
                );
            }
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
        let Some(range) = self.publish_frozen_range(tenant).await? else {
            return Ok(PublishOutcome::Idle);
        };
        let retired = self.settle(tenant, range.seq_hi).await?;
        Ok(PublishOutcome::Published {
            seq_lo: range.seq_lo,
            seq_hi: range.seq_hi,
            retired,
        })
    }

    /// Freeze the tenant's owed range and durably append exactly it, without settling.
    ///
    /// This is the half of a cycle that must survive being repeated: the frozen
    /// bound is persisted before the append and reused verbatim afterwards, so a
    /// competing replica or a restart after an uncertain append re-derives the
    /// same tenant, bounds, and batch identity and lands on Scribe's durable
    /// batch fence instead of duplicating retained history. Rows appended above
    /// the frozen bound are not read and wait for the next batch.
    ///
    /// Returns `None` when the tenant owes nothing.
    ///
    /// # Errors
    /// Returns [`AuditPublicationError::Outbox`] when freezing or reading the
    /// range fails, or when a frozen bound has no staged rows — which cannot
    /// happen while staging is only deleted through the watermark;
    /// [`AuditPublicationError::Projection`] for non-contiguous or invalid
    /// content; and [`AuditPublicationError::Publish`] when Scribe refuses the
    /// append.
    pub async fn publish_frozen_range(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<AuditPublicationRange>, AuditPublicationError> {
        let Some(range) = self.freeze(tenant).await? else {
            return Ok(None);
        };
        let rows = self.read_range(tenant, range).await?;
        if rows.is_empty() {
            return Err(AuditPublicationError::Outbox(format!(
                "frozen audit range {}..={} has no staged row",
                range.seq_lo, range.seq_hi
            )));
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
        Ok(Some(range))
    }

    /// Freeze, or reuse, the one range this tenant owes retained history.
    async fn freeze(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<AuditPublicationRange>, AuditPublicationError> {
        let mut conn = TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        let range = freeze_publication_range(&mut conn, self.batch_records)
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        Ok(range)
    }

    /// Read exactly the staged rows of one frozen range.
    async fn read_range(
        &self,
        tenant: DataTenantId,
        range: AuditPublicationRange,
    ) -> Result<Vec<vala_sql::row_types::audit_staging::AuditStagingRow>, AuditPublicationError>
    {
        let mut conn = TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        let rows = list_publication_range(&mut conn, range)
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
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
    pub async fn settle(
        &self,
        tenant: DataTenantId,
        seq_hi: i64,
    ) -> Result<u64, AuditPublicationError> {
        let mut conn = TenantConn::acquire(&self.pool, tenant)
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

/// Stable failures raised by one publication cycle.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuditPublicationError {
    /// Freezing or reading the owed staging range failed.
    #[error("audit staging read failed: {0}")]
    Outbox(String),
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
