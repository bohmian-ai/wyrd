//! Scribe cluster-node heartbeat + live-scribe discovery.
//!
//! The `vala.cluster_nodes` row for this pod is created by
//! [`stream_identity::acquire_on_boot`](super::stream_identity::acquire_on_boot);
//! this module owns the recurring `UPDATE heartbeat_at = now()` lifecycle plus
//! the `live_scribes()` reader Oracle discovery consumes.
//!
//! Heartbeats never touch `fencing_token` (epochs only advance on boot) and
//! do not emit `audit_staging` rows — ordinary uptime signal, not a
//! security/lifecycle transition.

use std::time::Duration;

use sqlx::types::Uuid;
use tokio::task::JoinHandle;
use vala_sql::ValaPostgres;
use wyrd_spec::DataTenantId;

use crate::contracts::ScribeError;
use crate::scribe::stream_identity::{NodeId, WriterEpoch};

/// Live scribe row projected from `vala.cluster_nodes`.
///
/// Matches the discovery query shape in CONTRACTS §8:
/// `(node_id, fencing_token AS writer_epoch, advertise_addr)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveScribe {
    /// Stable pod identifier.
    pub node_id: NodeId,
    /// Writer epoch (aliased from `fencing_token`).
    pub writer_epoch: WriterEpoch,
    /// Advertise address (host:port) the Oracle uses to reach this Scribe.
    pub advertise_addr: String,
}

/// Recurring heartbeat task handle.
///
/// Spawned by [`ScribeHeartbeat::start`]. Aborted on `shutdown()` or drop.
#[derive(Debug)]
pub struct ScribeHeartbeat {
    handle: JoinHandle<()>,
}

impl ScribeHeartbeat {
    /// Spawn the heartbeat loop.
    ///
    /// Ticks every `interval` (5 s in production; tests override to speed up
    /// the wall clock). The row this heartbeat updates must already exist —
    /// [`super::stream_identity::acquire_on_boot`] creates it during pod boot.
    ///
    /// Transient PG errors are logged at `warn` and the loop continues; a
    /// stale `heartbeat_at` is caught by the 15 s staleness filter in
    /// [`live_scribes`].
    #[must_use]
    pub fn start(postgres: ValaPostgres, node_id: NodeId, interval: Duration) -> Self {
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            // First `tick()` returns immediately; the row is already fresh from
            // acquire_on_boot's `heartbeat_at = now()`, so consume it without
            // hitting PG.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(err) = heartbeat_tick(&postgres, node_id).await {
                    tracing::warn!(
                        %node_id,
                        error = %err,
                        "scribe heartbeat tick failed; will retry next interval",
                    );
                }
            }
        });
        Self { handle }
    }

    /// Stop the heartbeat loop.
    pub fn shutdown(self) {
        self.handle.abort();
    }
}

impl Drop for ScribeHeartbeat {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Run one heartbeat update against this pod's `vala.cluster_nodes` row.
///
/// Only touches `heartbeat_at`; `fencing_token` and `started_at` remain
/// pinned to the values [`super::stream_identity::acquire_on_boot`] set on
/// boot.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if the UPDATE fails.
pub async fn heartbeat_tick(postgres: &ValaPostgres, node_id: NodeId) -> Result<(), ScribeError> {
    let mut conn = postgres
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .map_err(|error| ScribeError::Internal {
            detail: format!("failed to open heartbeat transaction: {error}"),
        })?;
    let result = sqlx::query(
        "UPDATE vala.cluster_nodes SET heartbeat_at = now() \
         WHERE data_tenant_id = $1 AND node_id = $2",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .bind(node_id.as_uuid())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|e| ScribeError::Internal {
        detail: format!("failed to update heartbeat_at: {e}"),
    })?;
    if result.rows_affected() == 0 {
        return Err(ScribeError::Internal {
            detail: format!("cluster_nodes row missing for node_id={node_id}"),
        });
    }
    conn.commit().await.map_err(|error| ScribeError::Internal {
        detail: format!("failed to commit heartbeat: {error}"),
    })?;
    Ok(())
}

/// Return every scribe row whose `heartbeat_at` is within the 15 s liveness
/// window.
///
/// Projection matches the CONTRACTS §8 discovery query:
/// `(node_id, fencing_token AS writer_epoch, advertise_addr)`. Oracle
/// discovery in Phase B consumes this reader; the Scribe pod itself does not.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if the SELECT fails.
pub async fn live_scribes(postgres: &ValaPostgres) -> Result<Vec<LiveScribe>, ScribeError> {
    let mut conn = postgres
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .map_err(|error| ScribeError::Internal {
            detail: format!("failed to open discovery transaction: {error}"),
        })?;
    let rows: Vec<(Uuid, i64, String)> = sqlx::query_as(
        "SELECT node_id, fencing_token AS writer_epoch, advertise_addr
         FROM vala.cluster_nodes
         WHERE data_tenant_id = $1
           AND role = 'scribe'
           AND heartbeat_at > now() - interval '15 seconds'
         ORDER BY node_id",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(|e| ScribeError::Internal {
        detail: format!("failed to query live_scribes: {e}"),
    })?;

    conn.commit().await.map_err(|error| ScribeError::Internal {
        detail: format!("failed to commit discovery read: {error}"),
    })?;
    Ok(rows
        .into_iter()
        .map(|(node_uuid, epoch, addr)| LiveScribe {
            node_id: NodeId::new(node_uuid),
            writer_epoch: WriterEpoch::new(epoch),
            advertise_addr: addr,
        })
        .collect())
}
