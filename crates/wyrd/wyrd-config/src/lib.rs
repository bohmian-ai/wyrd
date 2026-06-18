//! Wyrd workspace configuration.
//!
//! Parses `wyrd.toml` and merges its defaults into card metadata
//! before the CLI or SDK sends a card to the server. Loader-side
//! only — the server never reads this file.
//!
//! See `architecture/wyrd-design.md` §"Workspace config (`wyrd.toml`)".

#![deny(missing_docs)]

mod config;
mod discovery;
mod error;
mod merge;

#[cfg(feature = "python")]
mod py;

pub use config::{Defaults, KindOverride, WyrdConfig};
pub use error::WyrdConfigError;
pub use merge::apply_defaults;

#[cfg(feature = "python")]
pub use py::register;
