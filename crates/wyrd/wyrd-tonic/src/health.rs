//! Health-sentinel type and the snapshot trait the server-feature consumer reads.
//!
//! Both pieces are dependency-free of `wyrd-server` so the future rust-client
//! and py-wyrd paths can pull them without transitively dragging in axum or sqlx.

/// The tonic `NamedService` whose serving status drives `grpc.health.v1.Health/Check`.
///
/// Until business services land, this sentinel is the single registered service.
/// When a future commit adds a real service `S`, replace it everywhere with `S`
/// and call `reporter.set_serving::<S>().await` on it instead.
#[derive(Clone, Copy, Debug, Default)]
pub struct WyrdHealthSentinel;

impl tonic::server::NamedService for WyrdHealthSentinel {
    const NAME: &'static str = "wyrd.v1.HealthSentinel";
}

/// Lock-free snapshot the gRPC health consumer reads.
///
/// Implemented by `wyrd_server::health::ReadinessSnapshot`; kept abstract here
/// so `wyrd-tonic` does NOT depend on `wyrd-server` (which would be circular).
pub trait HealthSnapshot: Send + Sync + 'static {
    /// `true` when every cached probe passes.
    fn all_ok(&self) -> bool;
}
