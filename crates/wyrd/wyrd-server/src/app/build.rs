use std::sync::Arc;

use axum::Router;
use tokio_util::sync::CancellationToken;
use wyrd_telemetry::TelemetryGuard;
use wyrd_tonic::tonic_health::server::HealthReporter;

use crate::boot::{ServerBootError, build_app_state_from_config};
use crate::config::WyrdServerConfig;
use crate::state::AppState;

/// Build the router and shared state in one call so callers can couple HTTP
/// with gRPC/Prometheus transports.
pub async fn build_app(
    config: &WyrdServerConfig,
    shutdown: CancellationToken,
    telemetry: Arc<TelemetryGuard>,
    grpc_health: HealthReporter,
) -> Result<(Router, AppState), ServerBootError> {
    let state = build_app_state_from_config(config, shutdown, telemetry, grpc_health).await?;
    state
        .production_validate()
        .map_err(ServerBootError::ProductionValidation)?;
    let router = crate::http::build_router(state.clone());
    Ok((router, state))
}
