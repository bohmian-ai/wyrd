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

use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

use axum::Router;
use axum::routing::get;

/// Build unprotected health routes.
pub fn health_router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
}

/// Basic liveness endpoint.
pub async fn healthz() -> &'static str {
    "ok"
}

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
    /// Scribe WAL recovery or downstream publication has not completed.
    ScribeRecovery,
    /// Oracle role registration, coordination, or worker startup has not completed.
    OracleStartup,
    /// The selected Forge coordinator has not completed a scheduling pass.
    ForgeCoordinatorUnavailable,
    /// The selected Forge worker has not finished durable recovery.
    ForgeWorkerUnavailable,
}

/// Snapshot published by the background readiness_loop task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadinessSnapshot {
    /// Postgres liveness result.
    pub postgres: ProbeOutcome,
    /// Object storage liveness result.
    pub storage: ProbeOutcome,
    /// Scribe recovery and write-path readiness result.
    pub scribe: ProbeOutcome,
    /// Oracle registration and query-path readiness result.
    pub oracle: ProbeOutcome,
    /// Forge coordinator readiness, present only when that role is selected.
    pub forge_coordinator: Option<ProbeOutcome>,
    /// Forge worker readiness, present only when that role is selected.
    pub forge_worker: Option<ProbeOutcome>,
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
            scribe: ProbeOutcome {
                ok: false,
                reason: ProbeReason::Warmup,
                elapsed_ms: 0,
            },
            oracle: ProbeOutcome {
                ok: false,
                reason: ProbeReason::Warmup,
                elapsed_ms: 0,
            },
            // Absent until a tick observes which Forge roles this target
            // selected, so a non-Forge target's report never grows a check it
            // can never satisfy.
            forge_coordinator: None,
            forge_worker: None,
        }
    }

    /// True when all probes passed in the most recent tick.
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.postgres.ok
            && self.storage.ok
            && self.scribe.ok
            && self.oracle.ok
            && self.forge_coordinator.as_ref().is_none_or(|probe| probe.ok)
            && self.forge_worker.as_ref().is_none_or(|probe| probe.ok)
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
        scribe: probe_scribe(state),
        oracle: probe_oracle(state),
        forge_coordinator: probe_forge_coordinator(state),
        forge_worker: probe_forge_worker(state),
    }
}

/// Reads the Forge coordinator bit the supervised planning loop publishes.
///
/// Returns `None` when this target did not select the role, so an unselected
/// role contributes no check rather than a vacuously passing one.
fn probe_forge_coordinator(state: &AppState) -> Option<ProbeOutcome> {
    let forge = state.bifrost.forge()?;
    forge.coordinator()?;
    let ready = forge.coordinator_readiness().is_ready();
    metrics::gauge!("bifrost_role_ready", "role" => "forge_coordinator").set(if ready {
        1.0
    } else {
        0.0
    });
    Some(role_outcome(
        ready,
        ProbeReason::ForgeCoordinatorUnavailable,
    ))
}

/// Reads the Forge worker bit its recovery-gated loop publishes.
///
/// Returns `None` when this target did not select the role.
fn probe_forge_worker(state: &AppState) -> Option<ProbeOutcome> {
    let forge = state.bifrost.forge()?;
    forge.worker()?;
    let ready = forge.worker_readiness().is_ready();
    metrics::gauge!("bifrost_role_ready", "role" => "forge_worker").set(if ready {
        1.0
    } else {
        0.0
    });
    Some(role_outcome(ready, ProbeReason::ForgeWorkerUnavailable))
}

/// Builds one probe outcome from a published role bit and its unready reason.
fn role_outcome(ready: bool, unavailable: ProbeReason) -> ProbeOutcome {
    ProbeOutcome {
        ok: ready,
        reason: if ready { ProbeReason::Ok } else { unavailable },
        elapsed_ms: 0,
    }
}

/// Reads retained Oracle readiness without executing a query or touching storage.
fn probe_oracle(state: &AppState) -> ProbeOutcome {
    let selected = state.bifrost.oracle().is_some();
    let outcome = match state.bifrost_query() {
        Some(runtime) if runtime.is_ready() => ProbeOutcome {
            ok: true,
            reason: ProbeReason::Ok,
            elapsed_ms: 0,
        },
        Some(_) if selected => ProbeOutcome {
            ok: false,
            reason: ProbeReason::OracleStartup,
            elapsed_ms: 0,
        },
        None if selected => ProbeOutcome {
            ok: false,
            reason: ProbeReason::OracleStartup,
            elapsed_ms: 0,
        },
        None | Some(_) => ProbeOutcome {
            ok: true,
            reason: ProbeReason::Ok,
            elapsed_ms: 0,
        },
    };
    metrics::gauge!("bifrost_role_ready", "role" => "oracle").set(if selected && outcome.ok {
        1.0
    } else {
        0.0
    });
    outcome
}

/// Read the Scribe recovery bit without touching its queues or storage.
fn probe_scribe(state: &AppState) -> ProbeOutcome {
    let selected = state.bifrost.scribe().is_some();
    let outcome = match state.bifrost_ingest() {
        Some(runtime) if runtime.is_ready() => ProbeOutcome {
            ok: true,
            reason: ProbeReason::Ok,
            elapsed_ms: 0,
        },
        Some(_) => ProbeOutcome {
            ok: false,
            reason: ProbeReason::ScribeRecovery,
            elapsed_ms: 0,
        },
        None => ProbeOutcome {
            ok: true,
            reason: ProbeReason::Ok,
            elapsed_ms: 0,
        },
    };
    metrics::gauge!("bifrost_role_ready", "role" => "scribe").set(if selected && outcome.ok {
        1.0
    } else {
        0.0
    });
    outcome
}

async fn probe_postgres(state: &AppState, probe_timeout: Duration) -> ProbeOutcome {
    let started = std::time::Instant::now();
    let result = timeout(probe_timeout, async {
        let mut conn = state
            .postgres
            .app_pool()
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
    scribe: PublicProbeOutcome,
    oracle: PublicProbeOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    forge_coordinator: Option<PublicProbeOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forge_worker: Option<PublicProbeOutcome>,
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
                scribe: PublicProbeOutcome {
                    reason: snapshot.scribe.reason,
                },
                oracle: PublicProbeOutcome {
                    reason: snapshot.oracle.reason,
                },
                forge_coordinator: snapshot.forge_coordinator.as_ref().map(|probe| {
                    PublicProbeOutcome {
                        reason: probe.reason,
                    }
                }),
                forge_worker: snapshot
                    .forge_worker
                    .as_ref()
                    .map(|probe| PublicProbeOutcome {
                        reason: probe.reason,
                    }),
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
        Self::all_ok(self)
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
            scribe: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
            oracle: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
            forge_coordinator: None,
            forge_worker: None,
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
            scribe: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
            oracle: ProbeOutcome {
                ok: true,
                reason: ProbeReason::Ok,
                elapsed_ms: 1,
            },
            forge_coordinator: None,
            forge_worker: None,
        }
    }

    #[test]
    fn initial_snapshot_is_not_ready() {
        let snap = ReadinessSnapshot::initial();
        assert!(!snap.all_ok());
        assert_eq!(snap.postgres.reason, ProbeReason::Warmup);
        assert_eq!(snap.storage.reason, ProbeReason::Warmup);
        assert_eq!(snap.scribe.reason, ProbeReason::Warmup);
        assert_eq!(snap.oracle.reason, ProbeReason::Warmup);
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

    #[test]
    fn failed_scribe_recovery_is_not_ready() {
        let mut snapshot = all_ok_snapshot();
        snapshot.scribe = ProbeOutcome {
            ok: false,
            reason: ProbeReason::ScribeRecovery,
            elapsed_ms: 0,
        };
        assert!(!snapshot.all_ok());
    }

    /// Oracle startup failure independently blocks public readiness.
    #[test]
    fn failed_oracle_startup_is_not_ready() {
        let mut snapshot = all_ok_snapshot();
        snapshot.oracle = ProbeOutcome {
            ok: false,
            reason: ProbeReason::OracleStartup,
            elapsed_ms: 0,
        };
        assert!(!snapshot.all_ok());
    }
}

#[cfg(test)]
mod pg_tests {
    use super::*;

    #[tokio::test]
    async fn readiness_loop_exits_on_cancel() {
        use std::sync::Arc;

        use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
        use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        let root = tempfile::tempdir().expect("temp dir");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let storage = Arc::new(StorageHandle::new(BackendSigner::Local(signer)));
        let state = crate::test_support::test_app_state(
            postgres,
            storage,
            crate::test_support::test_catalog().await,
        );

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
