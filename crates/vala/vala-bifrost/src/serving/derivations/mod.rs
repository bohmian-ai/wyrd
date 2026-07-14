//! Cross-table derivation worker (slice 05b).
//!
//! Owns the long-running background worker that consumes committed source
//! deltas (currently `traces.spans`) and projects them into target tables
//! (currently `genai.{messages,embeddings,tool_calls,memory}`), watermarked
//! in `vala.olap_derivations`.
//!
//! The only exported symbol callers need is [`spawn_genai_derivation_worker`];
//! the runtime internals live in [`runtime`] and are not public.

mod runtime;

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgListener;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::catalog::WyrdCatalog;
use crate::error::BifrostError;
use crate::tables::genai::{DomainDerivation, GenAiFromSpans};
use crate::writer::CommitEvent;

/// Fallback poll cadence for the background genai derivation worker.
///
/// The derivation worker is now the ONLY derivation path. It is driven
/// primarily by `PostgreSQL` `NOTIFY` wakeups on the `vala_commits` channel,
/// emitted immediately after each committed source span batch. This periodic
/// tick is the fallback for missed notifications (listener disconnected, server
/// restarted before a `NOTIFY` was processed) and crash recovery (spans
/// committed but genai rows not yet derived because the server restarted).
/// 60 s gives bounded lag without hammering Iceberg scans in steady state.
pub const FALLBACK_CADENCE_SECS: u64 = 60;

/// Spawn the genai derivation worker.
///
/// Returns `None` when the operator pool (BYPASSRLS) is unavailable — cross-
/// tenant tenant enumeration requires it. Otherwise spawns a tokio task that
/// runs [`DerivationRuntime::run_tick`] on commit notifications and each
/// `FALLBACK_CADENCE` tick for crash recovery.
///
/// The worker logs per-tenant errors but never panics: a single-tenant failure
/// does not block other tenants or shut down the worker.
#[must_use]
pub fn spawn_genai_derivation_worker(
    catalog: Arc<WyrdCatalog>,
    pool: sqlx::PgPool,
    op: vala_sql::OperatorPool,
    shutdown: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Stable per-process identity for the maintenance-lease owner field.
        // Generated once at spawn so renewal-fencing (`(owner, fencing_token)`)
        // remains stable across ticks.
        let worker_owner = Uuid::now_v7();

        // Load table UIDs once at startup. If this fails (e.g. tables not yet
        // registered on a fresh node), log and retry on the next tick.
        let uids = loop {
            match runtime::TableUids::load(&catalog).await {
                Ok(u) => break u,
                Err(e) => {
                    tracing::warn!(error = %e, "genai derivation worker: table UID load failed, retrying on next tick");
                }
            }
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(Duration::from_secs(FALLBACK_CADENCE_SECS)) => {}
            }
        };

        let mut listener = match PgListener::connect_with(&pool).await {
            Ok(mut listener) => match listener.listen("vala_commits").await {
                Ok(()) => Some(listener),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "genai derivation worker: commit listener setup failed; using fallback only"
                    );
                    None
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "genai derivation worker: commit listener connection failed; using fallback only"
                );
                None
            }
        };

        let rt = runtime::DerivationRuntime {
            catalog,
            pool,
            op,
            uids,
            derivation: GenAiFromSpans,
            worker_owner,
        };

        loop {
            if let Some(active_listener) = listener.as_mut() {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_secs(FALLBACK_CADENCE_SECS)) => {
                        if let Err(e) = rt.run_tick().await {
                            tracing::error!(error = %e, "genai derivation worker: fallback tick failed");
                        }
                    }
                    notification = active_listener.recv() => {
                        match notification {
                            Ok(notification) => {
                                let Ok(CommitEvent::SpanCommitted { table_uid, .. }) =
                                    serde_json::from_str::<CommitEvent>(notification.payload())
                                else {
                                    continue;
                                };
                                if table_uid != rt.uids.source {
                                    continue;
                                }
                                if let Err(e) = rt.run_tick().await {
                                    tracing::error!(error = %e, "genai derivation worker: notification tick failed");
                                }
                            }
                            Err(error) => {
                                tracing::warn!(
                                    error = %error,
                                    "genai derivation worker: commit listener receive failed; using fallback only"
                                );
                                listener = None;
                            }
                        }
                    }
                }
            } else {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    () = tokio::time::sleep(Duration::from_secs(FALLBACK_CADENCE_SECS)) => {
                        if let Err(e) = rt.run_tick().await {
                            tracing::error!(error = %e, "genai derivation worker: fallback tick failed");
                        }
                    }
                }
            }
            if shutdown.is_cancelled() {
                break;
            }
        }
    })
}

/// Run one full derivation pass NOW.
///
/// Test-only seam: real fallback recovery relies on the 60-second `tokio::time`
/// sleep in [`spawn_genai_derivation_worker`], which cannot be virtualized
/// reliably in a test that also drives real Postgres/Iceberg I/O. Rather than
/// wait 60 real seconds, journey tests call this helper directly to prove
/// "worker tick materialises the missed source batch" against the same runtime
/// the background worker uses.
///
/// Production callers still go through [`spawn_genai_derivation_worker`]; this
/// entry point does no leasing, listens to no NOTIFY, and never runs on its
/// own cadence.
///
/// # Errors
/// Returns [`BifrostError`] when table UID loading, source enumeration, or a
/// target write fails.
pub async fn run_genai_derivation_tick(
    catalog: Arc<WyrdCatalog>,
    pool: sqlx::PgPool,
    op: vala_sql::OperatorPool,
    worker_owner: Uuid,
) -> Result<(), BifrostError> {
    let uids = runtime::TableUids::load(&catalog).await?;
    let rt = runtime::DerivationRuntime {
        catalog,
        pool,
        op,
        uids,
        derivation: GenAiFromSpans,
        worker_owner,
    };
    rt.run_tick().await
}

/// Initialise the [`GenAiFromSpans`] derivation in `vala.olap_derivations`
/// for a given tenant. Called by tests to pre-register a derivation row before
/// the worker tick fires.
///
/// This is a no-op if the row already exists (`ON CONFLICT DO NOTHING`).
///
/// # Errors
/// Returns [`BifrostError::Sql`] when the query fails.
pub async fn register_genai_derivation(
    catalog: &WyrdCatalog,
    pool: &sqlx::PgPool,
    tenant: wyrd_spec::ids::DataTenantId,
) -> Result<(), BifrostError> {
    let uids = runtime::TableUids::load(catalog).await?;
    let derivation_uid_bytes: [u8; 16] = *GenAiFromSpans::DERIVATION_UID.as_bytes();

    let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
        .await
        .map_err(BifrostError::Sql)?;
    vala_sql::queries::olap_derivations::insert_derivation(
        &mut conn,
        &derivation_uid_bytes,
        &uids.source,
        &uids.messages,
        uuid::Uuid::nil(),
        "genai_from_spans",
        None,
    )
    .await
    .map_err(BifrostError::Sql)?;
    conn.commit().await.map_err(BifrostError::Sql)?;
    Ok(())
}
