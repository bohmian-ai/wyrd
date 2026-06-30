//! Cached readiness checking for /readyz and gRPC Health parity.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use wyrd_spec::error::WyrdError;

use crate::error::WyrdErrorResponse;
use crate::state::AppState;

/// Stable reason codes surfaced in the `/readyz` body.
///
/// Raw error strings from sqlx or the storage backend are never included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeReason {
    /// Probe passed.
    Ok,
    /// Probe was cancelled due to deadline.
    Timeout,
    /// Pool connection could not be acquired.
    PoolAcquire,
    /// Liveness query returned an error.
    QueryFailure,
    /// Backend returned an unexpected error.
    BackendError,
    /// Pre-boot warmup: the background task has not ticked yet.
    Warmup,
}

/// Snapshot published by the background readiness_loop task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadinessSnapshot {
    /// Postgres liveness result.
    pub postgres: ProbeOutcome,
    /// Object storage liveness result.
    pub storage: ProbeOutcome,
}

/// Per-dependency probe result.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProbeOutcome {
    /// Whether the probe passed.
    pub ok: bool,
    /// Stable reason code.
    pub reason: ProbeReason,
    /// Internal diagnostic, NOT serialized to the public body.
    #[serde(skip)]
    pub elapsed_ms: u128,
}

impl ReadinessSnapshot {
    /// Seed value: not ready until the background task has ticked once.
    #[must_use]
    pub fn initial() -> Self {
        let warmup = ProbeOutcome {
            ok: false,
            reason: ProbeReason::Warmup,
            elapsed_ms: 0,
        };
        Self {
            postgres: warmup.clone(),
            storage: warmup,
        }
    }

    /// True when all probes passed in the most recent tick.
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.postgres.ok && self.storage.ok
    }
}

/// Background readiness publisher. Runs every `tick` until `shutdown` fires.
#[tracing::instrument(skip(state, shutdown))]
pub async fn readiness_loop(
    state: AppState,
    tick: Duration,
    probe_timeout: Duration,
    shutdown: CancellationToken,
) {
    loop {
        let snapshot = compute_snapshot(&state, probe_timeout).await;
        state.readiness.store(Arc::new(snapshot));
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(tick) => {}
        }
    }
}

async fn compute_snapshot(state: &AppState, probe_timeout: Duration) -> ReadinessSnapshot {
    let (pg, storage) = tokio::join!(
        probe_postgres(state, probe_timeout),
        probe_storage(state, probe_timeout),
    );
    ReadinessSnapshot {
        postgres: pg,
        storage,
    }
}

async fn probe_postgres(state: &AppState, probe_timeout: Duration) -> ProbeOutcome {
    let started = std::time::Instant::now();
    let result = timeout(probe_timeout, async {
        let mut conn = state
            .pool
            .acquire()
            .await
            .map_err(|e| (ProbeReason::PoolAcquire, e))?;
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(conn.as_mut())
            .await
            .map_err(|e| (ProbeReason::QueryFailure, e))
    })
    .await;
    let elapsed_ms = started.elapsed().as_millis();
    match result {
        Ok(Ok(_)) => ProbeOutcome {
            ok: true,
            reason: ProbeReason::Ok,
            elapsed_ms,
        },
        Ok(Err((reason, error))) => {
            tracing::warn!(
                reason = ?reason,
                error_class = sqlx_error_class(&error),
                elapsed_ms,
                "readiness: postgres probe failed"
            );
            ProbeOutcome {
                ok: false,
                reason,
                elapsed_ms,
            }
        }
        Err(_) => {
            tracing::warn!(
                reason = ?ProbeReason::Timeout,
                elapsed_ms,
                "readiness: postgres probe timed out"
            );
            ProbeOutcome {
                ok: false,
                reason: ProbeReason::Timeout,
                elapsed_ms,
            }
        }
    }
}

fn sqlx_error_class(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::PoolClosed => "pool_closed",
        sqlx::Error::PoolTimedOut => "pool_timeout",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::Tls(_) => "tls",
        sqlx::Error::Protocol(_) => "protocol",
        sqlx::Error::Database(_) => "database",
        sqlx::Error::RowNotFound => "row_not_found",
        sqlx::Error::TypeNotFound { .. } => "type_not_found",
        sqlx::Error::ColumnIndexOutOfBounds { .. } => "column_index",
        sqlx::Error::ColumnNotFound(_) => "column_missing",
        sqlx::Error::ColumnDecode { .. } => "column_decode",
        sqlx::Error::Decode(_) => "decode",
        sqlx::Error::WorkerCrashed => "worker_crashed",
        _ => "other",
    }
}

async fn probe_storage(state: &AppState, probe_timeout: Duration) -> ProbeOutcome {
    let started = std::time::Instant::now();
    let result = timeout(probe_timeout, state.storage.health_probe()).await;
    let elapsed_ms = started.elapsed().as_millis();
    match result {
        Ok(Ok(())) => ProbeOutcome {
            ok: true,
            reason: ProbeReason::Ok,
            elapsed_ms,
        },
        Ok(Err(error)) => {
            tracing::warn!(
                reason = ?ProbeReason::BackendError,
                error_class = storage_health_error_class(&error),
                elapsed_ms,
                "readiness: storage probe failed"
            );
            ProbeOutcome {
                ok: false,
                reason: ProbeReason::BackendError,
                elapsed_ms,
            }
        }
        Err(_) => {
            tracing::warn!(
                reason = ?ProbeReason::Timeout,
                elapsed_ms,
                "readiness: storage probe timed out"
            );
            ProbeOutcome {
                ok: false,
                reason: ProbeReason::Timeout,
                elapsed_ms,
            }
        }
    }
}

fn storage_health_error_class(error: &wyrd_storage::StorageHealthError) -> &'static str {
    use wyrd_storage::StorageHealthError;
    match error {
        StorageHealthError::Timeout { .. } => "timeout",
        StorageHealthError::Backend(_) => "backend",
        StorageHealthError::LocalRoot(_) => "local_root",
    }
}

#[derive(Debug, serde::Serialize)]
struct PublicReadinessReport {
    status: &'static str,
    checks: PublicChecks,
}

#[derive(Debug, serde::Serialize)]
struct PublicChecks {
    postgres: PublicProbeOutcome,
    storage: PublicProbeOutcome,
}

#[derive(Debug, serde::Serialize)]
struct PublicProbeOutcome {
    reason: ProbeReason,
}

impl PublicReadinessReport {
    fn from_snapshot(snapshot: &ReadinessSnapshot, status: &'static str) -> Self {
        Self {
            status,
            checks: PublicChecks {
                postgres: PublicProbeOutcome {
                    reason: snapshot.postgres.reason,
                },
                storage: PublicProbeOutcome {
                    reason: snapshot.storage.reason,
                },
            },
        }
    }
}

/// Readiness handler — reads the cached snapshot only. Never touches live dependencies.
#[tracing::instrument(skip(state))]
pub async fn readyz(State(state): State<AppState>) -> Response {
    let snapshot = state.readiness.load_full();
    if snapshot.all_ok() {
        let report = PublicReadinessReport::from_snapshot(&snapshot, "ok");
        return (StatusCode::OK, Json(report)).into_response();
    }
    let report = PublicReadinessReport::from_snapshot(&snapshot, "not_ready");
    WyrdErrorResponse::from(WyrdError::ServerNotReady {
        message: "one or more readiness probes failed".to_owned(),
        details: serde_json::json!({ "checks": report.checks }),
    })
    .into_response()
}

impl wyrd_tonic::health::HealthSnapshot for ReadinessSnapshot {
    fn all_ok(&self) -> bool {
        self.postgres.ok && self.storage.ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_ok_snapshot() -> ReadinessSnapshot {
        ReadinessSnapshot {
            postgres: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
            storage: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
        }
    }

    fn failing_snapshot() -> ReadinessSnapshot {
        ReadinessSnapshot {
            postgres: ProbeOutcome {
                ok: false,
                reason: ProbeReason::PoolAcquire,
                elapsed_ms: 100,
            },
            storage: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
        }
    }

    #[test]
    fn initial_snapshot_is_not_ready() {
        let snap = ReadinessSnapshot::initial();
        assert!(!snap.all_ok());
        assert_eq!(snap.postgres.reason, ProbeReason::Warmup);
        assert_eq!(snap.storage.reason, ProbeReason::Warmup);
    }

    #[test]
    fn all_ok_snapshot_is_ready() {
        let snap = all_ok_snapshot();
        assert!(snap.all_ok());
    }

    #[test]
    fn partial_failure_is_not_ready() {
        let snap = failing_snapshot();
        assert!(!snap.all_ok());
    }

    #[tokio::test]
    async fn readiness_loop_exits_on_cancel() {
        use std::sync::Arc;

        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
        use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let state = crate::state::AppState::new(app_pool, None, storage);

        let shutdown = CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        let handle = tokio::spawn(readiness_loop(
            state,
            Duration::from_millis(10),
            Duration::from_millis(100),
            shutdown,
        ));

        tokio::time::sleep(Duration::from_millis(25)).await;
        shutdown_clone.cancel();

        let result = tokio::time::timeout(Duration::from_millis(200), handle).await;
        assert!(result.is_ok(), "readiness_loop did not exit after cancel");
    }
}
