//! Bifrost client-queue lanes the L2 Bifrost experiment runs beside its
//! competing ingest and queries.
//!
//! One tenant registers a Service whose Model is bound to a Drift Window
//! Verifier; each lane loads the Service bundle into a fresh `WyrdState` and
//! emits Drift observations open loop through the public client, so the
//! lane measures the client queue's headroom, its behavior under late
//! acknowledgements, and a single in-flight send, and reads every admitted
//! row back exactly once.

mod fixture;
mod proxy;
mod step;

pub use fixture::Tenant;
pub use step::{Emitted, LANES};

/// Error type of the client and server steps a lane runs.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
