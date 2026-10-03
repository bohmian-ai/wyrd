//! The one non-blocking audit outbox every audited surface stages through.
//!
//! Permissions block; audits do not. A surface evaluates its permission,
//! enforces the verdict, and hands the decision to [`AuditOutbox`]'s `stage`,
//! which only enqueues it: no request waits for, or is refused by, an audit
//! commit. The generic [`Outbox`] owns the queue, per-tenant batching, bounded
//! concurrency, retry, and shutdown; [`AuditSink`] is its audit destination,
//! committing each tenant's batch into `vala.audit_staging` through the
//! canonical append, which is private to this crate. The server's
//! `AuditPublisher` later moves staged rows into retained history.
//!
//! A batch that fails to commit is retried at the front of its tenant's queue,
//! never dropped. Each decision carries the event id assigned when it was
//! staged, and the append skips ids already staged, so a retry after an unknown
//! commit outcome cannot stage a decision twice.

use std::sync::Arc;

use sqlx::types::Uuid;
use wyrd_runtime::outbox::{Outbox, OutboxSink};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::queries::audit_staging::append_audit_events;
use crate::{SqlError, ValaPostgres};

/// Tenants the audit writer commits at once.
///
/// Fixed rather than configurable: it is the audit writer's share of the Vala
/// pool, leaving the rest to reader-epoch renewal, readiness, query pins, and
/// publication.
pub const AUDIT_WRITER_CONNECTIONS: usize = 4;

/// The process audit outbox: the generic outbox over [`AuditSink`].
pub type AuditOutbox = Outbox<AuditSink>;

/// One audit decision as the outbox holds it.
///
/// The event id is assigned once, when the decision is staged, and travels
/// with it through every retry; staged audit is unique per tenant and event id.
#[derive(Debug, Clone)]
pub struct StagedAuditEvent {
    /// Id assigned when the decision was staged.
    pub event_id: Uuid,
    /// The decision itself.
    pub event: AuditEvent,
}

impl From<AuditEvent> for StagedAuditEvent {
    /// Stages `event` under a fresh time-ordered event id.
    fn from(event: AuditEvent) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            event,
        }
    }
}

/// Audit destination of the outbox: one tenant's batch, one transaction.
#[derive(Clone)]
pub struct AuditSink {
    /// Vala Postgres owner the batches commit through.
    vala: ValaPostgres,
}

impl AuditSink {
    /// Builds the audit destination over `vala`.
    #[must_use]
    pub const fn new(vala: ValaPostgres) -> Self {
        Self { vala }
    }

    /// Starts the process audit outbox over `vala`.
    ///
    /// The server calls this once per process and shares the result with
    /// every audited surface.
    ///
    /// # Panics
    ///
    /// Panics when called outside a Tokio runtime, because the writer task is
    /// spawned immediately.
    #[must_use]
    pub fn outbox(vala: ValaPostgres) -> Arc<AuditOutbox> {
        Outbox::new(Self::new(vala), AUDIT_WRITER_CONNECTIONS)
    }
}

impl OutboxSink for AuditSink {
    type Item = StagedAuditEvent;
    type Error = SqlError;
    const NAME: &'static str = "audit";

    /// Commits one tenant's decisions under its tenant binding with one
    /// chain-head lock, skipping decisions already staged.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when acquiring the tenant connection, the append,
    /// or the commit fails; the transaction then rolls back on drop.
    async fn write(
        &self,
        tenant: DataTenantId,
        events: &[StagedAuditEvent],
    ) -> Result<(), SqlError> {
        let mut conn = self.vala.tenant_conn(tenant).await?;
        append_audit_events(&mut conn, events).await?;
        conn.commit().await
    }
}
