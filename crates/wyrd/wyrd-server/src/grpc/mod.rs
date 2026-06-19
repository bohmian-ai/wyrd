//! gRPC scaffold re-exports.
//!
//! Boot wiring imports via `wyrd_server::grpc::*`. Every symbol here is a
//! `pub use` that forwards to the owning `wyrd_tonic` crate — this shim has
//! no body of its own.
pub use wyrd_tonic::error;
pub use wyrd_tonic::health::WyrdHealthSentinel;
pub use wyrd_tonic::server::*;
