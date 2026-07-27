//! gRPC scaffold re-exports plus the application-level ingest mount.
//!
//! Boot wiring imports via `wyrd_server::grpc::*`. Most symbols here are a
//! `pub use` that forwards to the owning `wyrd_tonic` crate. The one body this
//! module owns is [`build_app_grpc`]: the single code path that mounts the C1
//! ingest service, shared by `main.rs` and the `wyrd-testing` harness.
pub use wyrd_tonic::error;
pub use wyrd_tonic::health::WyrdHealthSentinel;
pub use wyrd_tonic::server::*;

use wyrd_tonic::tonic::transport::server::Router as TonicRouter;
use wyrd_tonic::tonic_health::pb::health_server::{Health, HealthServer};

use crate::AppState;

/// Build the application gRPC router: health (unauthenticated) plus the C1
/// ingest service with auth completed in the handler.
///
/// Fallible: ingest is never mounted unauthenticated, so a `None`
/// `AppState.token_verifier` is a hard [`GrpcError::MissingTokenVerifier`]. The
/// verifier is the same `TokenVerifier` the HTTP `AuthenticatedPrincipal`
/// extractor uses; `build_grpc_router` stays unchanged (health only,
/// `NoopInterceptor`, no `.layer`) and ingest is attached via `add_service`.
///
/// # Errors
/// Returns [`GrpcError::MissingTokenVerifier`] when no token verifier is
/// configured, or a router-assembly error from [`build_grpc_router`].
pub fn build_app_grpc<H>(
    state: &AppState,
    health_service: HealthServer<H>,
    cfg: GrpcRouterConfig,
) -> Result<TonicRouter, GrpcError>
where
    H: Health,
{
    if state.auth.token_verifier.is_none() {
        return Err(GrpcError::MissingTokenVerifier);
    }
    let ingest = state
        .bifrost_ingest
        .as_ref()
        .map(|runtime| runtime.gate())
        .ok_or(GrpcError::MissingScribe)?;
    let traces = wyrd_tonic::otlp::trace_service::trace_service_server::TraceServiceServer::new(
        (*ingest).clone(),
    );
    let metrics =
        wyrd_tonic::otlp::metrics_service::metrics_service_server::MetricsServiceServer::new(
            (*ingest).clone(),
        );
    let logs = wyrd_tonic::otlp::logs_service::logs_service_server::LogsServiceServer::new(
        (*ingest).clone(),
    );
    let query = crate::vala_query::grpc::ValaQueryGrpc::new(state.clone());
    let router = build_grpc_router(health_service, NoopInterceptor, cfg)?;
    Ok(router
        .add_service((*ingest).clone().into_server())
        .add_service(traces)
        .add_service(metrics)
        .add_service(logs)
        .add_service(query.into_server()))
}
