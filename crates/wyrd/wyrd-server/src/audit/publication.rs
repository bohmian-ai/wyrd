//! Bounded publication of tenant audit events into `vala.system.audit_log`.
//!
//! `vala.audit_staging` is the transactional authority: an audited transition
//! and its canonical event commit together. Retained history lives in the
//! Bifrost table, so one server-owned publisher moves each tenant's oldest
//! contiguous run of events across that boundary and retires the rows only
//! once the shipment is durable in Scribe.
//!
//! Every step is idempotent. The shipment's batch id is derived from the
//! tenant and its inclusive sequence range, so an uncertain publication is
//! resolved by republishing the same range into Scribe's durable dedup fence
//! rather than by guessing. Retirement deletes exactly that range, so an
//! uncertain retirement costs one replayed shipment and never a lost or
//! duplicated audit event.

use std::time::Duration;

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use vala_bifrost_redux::tables::audit::projection::project_audit_rows;
use vala_sql::TenantConn;
use vala_sql::queries::audit_staging::{list_publication_batch, retire_published};
use vala_sql::row_types::audit_staging::AuditStagingRow;
use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
use wyrd_spec::request_id::RequestId;
use wyrd_sql::OperatorPool;

use crate::state::{AppState, ServerGate};

/// Fully-qualified physical name of the retained audit table.
const AUDIT_LOG_FQN: &str = "vala.system.audit_log";

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
    /// The inclusive range was published and retired.
    Published {
        /// First published sequence number.
        seq_lo: i64,
        /// Last published sequence number.
        seq_hi: i64,
        /// Rows actually removed by retirement.
        retired: u64,
    },
}

/// Server-owned publisher moving audit events into retained history.
///
/// It owns the pools and the Gate seam it publishes through, so a cycle is a
/// method on the owner rather than a function threading four dependencies.
pub struct AuditPublisher {
    /// RLS-enforced application pool used for tenant-scoped work.
    pool: PgPool,
    /// Cross-tenant pool the admin-owned tenant directory is read through.
    ///
    /// `platform.tenants` is not tenant data and grants no read to the
    /// application role, so a sweep that listed tenants on `pool` would be
    /// refused by Postgres and service nobody.
    directory: OperatorPool,
    /// Ingest seam that stamps and durably appends the shipment.
    gate: ServerGate,
    /// Maximum events moved by one tenant cycle.
    batch_records: i64,
    /// Delay between sweeps of the tenant directory.
    interval: Duration,
}

impl AuditPublisher {
    /// Build the publisher for a serving state that owns a Scribe.
    ///
    /// Returns `None` for a role with no ingest owner: such a process has no
    /// durable seam to publish through and must not claim the work. It also
    /// returns `None` without a cross-tenant operator pool, since the tenant
    /// directory is unreadable from the application role.
    #[must_use]
    pub fn from_state(state: &AppState) -> Option<Self> {
        state.bifrost_ingest()?;
        let directory = state.postgres.operator_pool().or_else(|| {
            tracing::warn!("audit publication skipped because operator pool is unavailable");
            None
        })?;
        Some(Self {
            pool: state.postgres.wyrd().app_pool().clone(),
            directory,
            gate: state.bifrost.gate().clone(),
            batch_records: PUBLICATION_BATCH_RECORDS,
            interval: PUBLICATION_INTERVAL,
        })
    }

    /// Run bounded publication sweeps until cancelled.
    ///
    /// A failing tenant is logged and retried on the next sweep: its rows stay
    /// durable in Postgres, so a transient Scribe or Postgres failure delays
    /// retained history rather than losing it.
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

    /// Move one tenant's oldest contiguous run of audit events, then retire it.
    ///
    /// The read transaction closes before the durable append, so a slow
    /// publication never holds a tenant transaction open, and the retirement
    /// runs in its own transaction once the append is durable.
    ///
    /// # Errors
    /// Returns the stable failure raised by the outbox read, the content
    /// projection, the durable append, or the retirement. A failure after a
    /// durable append leaves the range published but not retired; the next
    /// cycle republishes it into Scribe's dedup fence and retires it then.
    pub async fn publish_tenant(
        &self,
        tenant: DataTenantId,
    ) -> Result<PublishOutcome, AuditPublicationError> {
        let rows = self.claim(tenant).await?;
        if rows.is_empty() || is_publication_tail(&rows) {
            return Ok(PublishOutcome::Idle);
        }
        let projection = project_audit_rows(tenant, &rows)
            .map_err(|error| AuditPublicationError::Projection(error.to_string()))?;
        let (seq_lo, seq_hi) = (projection.seq_lo, projection.seq_hi);
        self.gate
            .publish_audit_projection(
                &publisher_principal(tenant),
                RequestId::now_v7(),
                projection,
            )
            .await
            .map_err(|error| AuditPublicationError::Publish(error.to_string()))?;
        let retired = self.retire(tenant, seq_lo, seq_hi).await?;
        Ok(PublishOutcome::Published {
            seq_lo,
            seq_hi,
            retired,
        })
    }

    /// Read the tenant's oldest bounded run of unpublished audit rows.
    async fn claim(
        &self,
        tenant: DataTenantId,
    ) -> Result<Vec<AuditStagingRow>, AuditPublicationError> {
        let mut conn = TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        let rows = list_publication_batch(&mut conn, self.batch_records)
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        conn.commit()
            .await
            .map_err(|error| AuditPublicationError::Outbox(error.to_string()))?;
        Ok(rows)
    }

    /// Retire the published inclusive range in its own tenant transaction.
    async fn retire(
        &self,
        tenant: DataTenantId,
        seq_lo: i64,
        seq_hi: i64,
    ) -> Result<u64, AuditPublicationError> {
        let mut conn = TenantConn::acquire(&self.pool, tenant)
            .await
            .map_err(|error| AuditPublicationError::Retire(error.to_string()))?;
        let retired = retire_published(&mut conn, seq_lo, seq_hi)
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
    /// The outbox read failed.
    #[error("audit outbox read failed: {0}")]
    Outbox(String),
    /// The claimed rows could not be projected into canonical content.
    #[error("audit projection failed: {0}")]
    Projection(String),
    /// The durable append was refused.
    #[error("audit publication failed: {0}")]
    Publish(String),
    /// Retirement of a published range failed.
    #[error("audit retirement failed: {0}")]
    Retire(String),
}

/// True when the batch contains only this publisher's own trailing markers.
///
/// Publishing into `vala.system.audit_log` is itself an audited transition, so
/// a shipment leaves one marker row behind. Publishing that marker alone would
/// mint the next one forever, so the tail waits until a real event arrives to
/// travel with it. No event is dropped — retained history simply lags by its
/// own marker until the tenant records something else.
fn is_publication_tail(rows: &[AuditStagingRow]) -> bool {
    rows.iter().all(|row| row.resource == AUDIT_LOG_FQN)
}

/// Build the internal principal that owns one tenant's publication.
fn publisher_principal(tenant: DataTenantId) -> Principal {
    Principal::new(
        PLATFORM_AUDIT_PRINCIPAL,
        PrincipalKind::User,
        tenant,
        Vec::new(),
        PermissionSet::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one outbox row for the given resource.
    fn row(seq: i64, resource: &str) -> AuditStagingRow {
        AuditStagingRow {
            data_tenant_id: uuid::Uuid::now_v7(),
            seq,
            entry_hash: vec![0; 32],
            prev_hash: vec![0; 32],
            request_id: "req".to_owned(),
            trace_id: None,
            operation: "bifrost.scribe.visibility.publish".to_owned(),
            resource: resource.to_owned(),
            card_ref: None,
            principal_id: uuid::Uuid::nil(),
            principal_kind: "user".to_owned(),
            auth_method: "internal".to_owned(),
            permission: "bifrost:record:write".to_owned(),
            decision: "allow".to_owned(),
            result: "success".to_owned(),
            payload_summary: "published".to_owned(),
            detail: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// A batch of nothing but publication markers does not ship.
    #[test]
    fn publication_markers_alone_are_a_tail() {
        let rows = vec![row(1, AUDIT_LOG_FQN), row(2, AUDIT_LOG_FQN)];
        assert!(is_publication_tail(&rows));
    }

    /// One real event makes the whole batch shippable, markers included.
    #[test]
    fn one_real_event_makes_the_batch_shippable() {
        let rows = vec![row(1, AUDIT_LOG_FQN), row(2, "wyrd.cards")];
        assert!(!is_publication_tail(&rows));
    }

    /// An empty batch is not treated as a tail; the caller handles it first.
    #[test]
    fn empty_batch_is_vacuously_a_tail() {
        assert!(is_publication_tail(&[]));
    }
}
