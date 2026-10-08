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
//! never dropped, but only once Postgres confirms the earlier attempt did not
//! commit. [`AuditSink`] records each write's transaction id and, when its
//! commit returns an error, asks Postgres for that transaction's outcome
//! before reporting anything, so a committed batch is never written twice.

use std::sync::Arc;

use wyrd_runtime::outbox::{INITIAL_BACKOFF, MAX_BACKOFF, Outbox, OutboxSink};
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

    /// Resolves the outcome of transaction `xact`, whose commit returned
    /// `error`, from Postgres transaction status.
    ///
    /// Asks `pg_xact_status` on a fresh pool connection, never the one whose
    /// commit failed. A committed transaction is the write's success. An
    /// aborted one returns `error`, so the outbox retries the batch. While the
    /// transaction is still in progress, or Postgres cannot be reached, this
    /// waits with the outbox's backoff and asks again; nothing is re-sent
    /// until the outcome is known. A status Postgres no longer holds cannot be
    /// resolved, so the `events` decisions are counted in
    /// `outbox_events_lost_total{outbox="audit"}` and not re-sent.
    ///
    /// Waiting holds the tenant's write slot. Cancelling it, as an expired
    /// shutdown deadline does, leaves the batch counted lost by the outbox.
    ///
    /// # Errors
    ///
    /// Returns `error` when Postgres reports the transaction aborted.
    async fn resolve_commit(
        &self,
        tenant: DataTenantId,
        xact: &str,
        events: usize,
        error: SqlError,
    ) -> Result<(), SqlError> {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            let status: Result<Option<String>, sqlx::Error> =
                sqlx::query_scalar("SELECT pg_xact_status($1::xid8)")
                    .bind(xact)
                    .fetch_one(self.vala.pool())
                    .await;
            match status.as_ref().map(Option::as_deref) {
                Ok(Some("committed")) => return Ok(()),
                Ok(Some("aborted")) => return Err(error),
                Ok(None) => {
                    metrics::counter!("outbox_events_lost_total", "outbox" => Self::NAME)
                        .increment(events as u64);
                    tracing::error!(
                        %tenant,
                        xact,
                        %error,
                        lost = events,
                        "audit commit outcome is no longer known; decisions counted lost"
                    );
                    return Ok(());
                }
                Ok(Some(_)) | Err(_) => {
                    tracing::warn!(
                        %tenant,
                        xact,
                        %error,
                        retry_in_ms = backoff.as_millis(),
                        "audit commit outcome unresolved; asking again"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }
}

impl OutboxSink for AuditSink {
    type Item = AuditEvent;
    type Error = SqlError;
    const NAME: &'static str = "audit";

    /// Commits one tenant's decisions under its tenant binding with one
    /// chain-head lock.
    ///
    /// Reads the transaction id first, so a commit that returns an error is
    /// resolved by [`AuditSink::resolve_commit`] rather than assumed failed.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when acquiring the tenant connection, reading the
    /// transaction id, or the append fails, after which the transaction rolls
    /// back on drop, or when a failed commit is confirmed aborted.
    async fn write(&self, tenant: DataTenantId, events: &[AuditEvent]) -> Result<(), SqlError> {
        let mut conn = self.vala.tenant_conn(tenant).await?;
        let xact: String = sqlx::query_scalar("SELECT pg_current_xact_id()::text")
            .fetch_one(&mut **conn.transaction())
            .await
            .map_err(SqlError::from)?;
        append_audit_events(&mut conn, events).await?;
        match conn.commit().await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.resolve_commit(tenant, &xact, events.len(), error)
                    .await
            }
        }
    }
}
