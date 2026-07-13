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

use tokio_util::sync::CancellationToken;

use crate::catalog::WyrdCatalog;
use crate::error::BifrostError;
use crate::tables::genai::{DomainDerivation, GenAiFromSpans};

/// Repair-worker poll cadence for the background genai derivation worker.
///
/// The inline path (OTLP collector → derive on arrival) is the primary write
/// path. This tick only fires for crash recovery: spans committed but genai
/// rows not yet written because the server restarted between the two writes.
/// 60 s gives bounded lag without hammering Iceberg scans in steady state.
pub const FALLBACK_CADENCE_SECS: u64 = 60;

/// Spawn the genai derivation worker.
///
/// Returns `None` when the operator pool (BYPASSRLS) is unavailable — cross-
/// tenant tenant enumeration requires it. Otherwise spawns a tokio task that
/// runs [`DerivationRuntime::run_tick`] on each `FALLBACK_CADENCE` tick
/// (replacing the NOTIFY-based wake path that lands in Task G).
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
                _ = shutdown.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_secs(FALLBACK_CADENCE_SECS)) => {}
            }
        };

        let rt = runtime::DerivationRuntime {
            catalog,
            pool,
            op,
            uids,
            derivation: GenAiFromSpans,
        };

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(FALLBACK_CADENCE_SECS)) => {}
            }
            if shutdown.is_cancelled() {
                break;
            }

            if let Err(e) = rt.run_tick().await {
                tracing::error!(error = %e, "genai derivation worker: tick failed");
            }
        }
    })
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
