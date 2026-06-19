//! Wyrd-owned tonic facade.
//!
//! All tonic-family version pins live here. Consumers (`wyrd-server`, future
//! `rust-client`, `py-wyrd` gRPC stubs) take `wyrd-tonic` as a workspace
//! dependency rather than declaring their own tonic pins.
//!
//! F-T11 closeout (intentional re-export): the re-exports below are the point
//! of this crate. A major bump in `tonic` or `tonic_health` is a `wyrd-tonic`
//! major bump — the trade Wyrd accepts for single-pin discipline.
pub use tonic;
pub use tonic_health;
pub use prost;

pub mod error;
pub mod health;

#[cfg(feature = "server")] pub mod server;
