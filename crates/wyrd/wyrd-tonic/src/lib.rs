//! Wyrd-owned tonic facade.
//!
//! All tonic-family version pins live here. Consumers (`wyrd-server`, future
//! `rust-client`, `py-wyrd` gRPC stubs) take `wyrd-tonic` as a workspace
//! dependency rather than declaring their own tonic pins.
//!
//! The re-exports below are the point of this crate. A major bump in `tonic`
//! or `tonic_health` is a `wyrd-tonic` major bump — the trade Wyrd accepts for
//! single-pin discipline.
pub use prost;
pub use tonic;
pub use tonic_types;

#[cfg(feature = "server")]
pub use tonic_health;

/// Generated `wyrd.v1` protobuf surface (compiled by `build.rs`).
///
/// The message structs are always present; the `bifrost_ingest_service_server`
/// / `bifrost_ingest_service_client` submodules appear only when this crate's
/// `server` / `client` feature is enabled.
pub mod wyrd {
    /// Version 1 of the Wyrd gRPC surface.
    pub mod v1 {
        tonic::include_proto!("wyrd.v1");
    }
}

pub mod error;
pub mod frame_codec;
pub mod private_conversion;
pub mod query_conversion;
pub mod transport;

// `health` impls `tonic::server::NamedService`, which only exists under tonic's
// `server` feature — keep the module (and its re-export) behind `server` so the
// client path stays axum-free.
#[cfg(feature = "server")]
pub mod health;

#[cfg(feature = "server")]
pub mod server;

/// OpenTelemetry OTLP proto surface (collector services + signal messages).
///
/// Gated behind `server` because the collector `*Service` traits are tonic
/// server-side codegen (`gen-tonic` pulls `tonic/transport`); keeping the
/// re-export here preserves the axum-free client tier.
///
/// Downstream consumers reach the collector service traits and signal message
/// types entirely through this facade — e.g.
/// `wyrd_tonic::otlp::trace_service::trace_service_server::TraceService` and
/// `wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest` — without ever
/// naming `opentelemetry_proto` or `tonic` directly.
#[cfg(feature = "server")]
pub mod otlp {
    /// Logs collector service (`LogsService` trait + `Export*` messages).
    pub use opentelemetry_proto::tonic::collector::logs::v1 as logs_service;
    /// Metrics collector service (`MetricsService` trait + `Export*` messages).
    pub use opentelemetry_proto::tonic::collector::metrics::v1 as metrics_service;
    /// Trace collector service (`TraceService` trait + `Export*` messages).
    pub use opentelemetry_proto::tonic::collector::trace::v1 as trace_service;

    // Signal + common message modules. Each exposes a `v1` submodule holding the
    // generated OTLP message types (e.g. `trace::v1::ResourceSpans`).
    pub use opentelemetry_proto::tonic::{common, logs, metrics, resource, trace};
}

#[cfg(all(test, feature = "server"))]
mod otlp_reexport_tests {
    // Proves the OTLP re-export compiles through the `wyrd_tonic::otlp` facade.
    // Each reference fails to compile if the corresponding re-export is removed.
    use super::otlp;

    #[allow(dead_code)]
    fn _assert_service_traits_reexported<T, M, L>()
    where
        T: otlp::trace_service::trace_service_server::TraceService,
        M: otlp::metrics_service::metrics_service_server::MetricsService,
        L: otlp::logs_service::logs_service_server::LogsService,
    {
    }

    #[test]
    fn otlp_proto_traits_reexported() {
        // One request message per signal, reached through the facade.
        let _trace: Option<otlp::trace_service::ExportTraceServiceRequest> = None;
        let _metrics: Option<otlp::metrics_service::ExportMetricsServiceRequest> = None;
        let _logs: Option<otlp::logs_service::ExportLogsServiceRequest> = None;
        // One signal message type, proving the message modules re-export too.
        let _spans: Option<otlp::trace::v1::ResourceSpans> = None;
    }
}
