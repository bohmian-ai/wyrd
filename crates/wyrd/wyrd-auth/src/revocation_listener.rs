//! Cross-pod revocation via Postgres LISTEN/NOTIFY.
//!
//! When a principal is revoked on any server replica, that replica sends
//! `NOTIFY wyrd_principal_revoked, '<payload>'`. All replicas (including the
//! sender) receive the notification and immediately invalidate the in-process
//! `SqlRevocationCheck` cache for that principal, bounding effective revocation
//! lag to NOTIFY delivery latency rather than the 5-second cache TTL.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wyrd_auth_verify::PrincipalKindWire;
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;

use super::revocation_resolver::SqlRevocationCheck;

const CHANNEL: &str = "wyrd_principal_revoked";

/// Notify all replicas that a principal has been revoked.
///
/// Sends `NOTIFY wyrd_principal_revoked, '<tenant>/<kind>/<id>'` on the given
/// pool connection. Callers are responsible for committing the surrounding
/// transaction before or after this call — the NOTIFY is delivered only when
/// the transaction commits.
///
/// # Errors
/// Returns a `SQLx` error on database failure.
pub async fn notify_principal_revoked(
    pool: &PgPool,
    tenant: DataTenantId,
    kind: PrincipalKindWire,
    id: PrincipalId,
) -> Result<(), sqlx::Error> {
    let kind_str = match kind {
        PrincipalKindWire::User => "user",
        PrincipalKindWire::Service => "service",
        PrincipalKindWire::Agent => "agent",
    };
    let payload = format!("{tenant}/{kind_str}/{id}");
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CHANNEL)
        .bind(&payload)
        .execute(pool)
        .await?;
    Ok(())
}

/// Background task that listens on `wyrd_principal_revoked` and invalidates the
/// in-process `SqlRevocationCheck` cache on each notification.
///
/// Spawned once at server boot via `RevocationListener::spawn`. The task
/// restarts the listener on transient errors and exits cleanly when
/// `shutdown_token` is cancelled.
pub struct RevocationListener {
    pool: PgPool,
    check: Arc<SqlRevocationCheck>,
    shutdown: CancellationToken,
}

impl RevocationListener {
    /// Create and immediately spawn the listener background task.
    pub fn spawn(
        pool: PgPool,
        check: Arc<SqlRevocationCheck>,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let listener = Self {
            pool,
            check,
            shutdown,
        };
        tokio::spawn(
            listener
                .run()
                .instrument(tracing::info_span!("revocation_listener")),
        )
    }

    async fn run(self) {
        loop {
            tokio::select! {
                () = self.shutdown.cancelled() => {
                    tracing::info!("revocation listener shutting down");
                    return;
                }
                result = self.listen_once() => {
                    match result {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "revocation listener disconnected, retrying");
                            tokio::time::sleep(Duration::from_secs(2)).await;
                        }
                    }
                }
            }
        }
    }

    async fn listen_once(&self) -> Result<(), sqlx::Error> {
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(CHANNEL).await?;
        loop {
            tokio::select! {
                () = self.shutdown.cancelled() => return Ok(()),
                notification = listener.recv() => {
                    let n = notification?;
                    self.handle_notification(n.payload());
                }
            }
        }
    }

    fn handle_notification(&self, payload: &str) {
        let mut parts = payload.splitn(3, '/');
        let (Some(tenant_str), Some(kind_str), Some(id_str)) =
            (parts.next(), parts.next(), parts.next())
        else {
            tracing::warn!(%payload, "malformed revocation notification");
            return;
        };

        let Ok(tenant) = tenant_str.parse::<DataTenantId>() else {
            tracing::warn!(%payload, "unparseable tenant in revocation notification");
            return;
        };
        let kind = match kind_str {
            "user" => PrincipalKindWire::User,
            "service" => PrincipalKindWire::Service,
            "agent" => PrincipalKindWire::Agent,
            _ => {
                tracing::warn!(%payload, "unknown kind in revocation notification");
                return;
            }
        };
        let Ok(id) = id_str.parse::<PrincipalId>() else {
            tracing::warn!(%payload, "unparseable principal id in revocation notification");
            return;
        };

        let check = Arc::clone(&self.check);
        tokio::spawn(async move {
            check.invalidate(tenant, id, kind).await;
        });
    }
}
