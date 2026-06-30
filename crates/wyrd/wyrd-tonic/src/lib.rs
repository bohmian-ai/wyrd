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

pub mod error;

// `health` impls `tonic::server::NamedService`, which only exists under tonic's
// `server` feature — keep the module (and its re-export) behind `server` so the
// client path stays axum-free.
#[cfg(feature = "server")]
pub mod health;

#[cfg(feature = "server")]
pub mod server;
