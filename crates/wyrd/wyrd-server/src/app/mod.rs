//! Application composition and process lifecycle.

pub mod build;
pub mod serve;
pub mod shutdown;

pub use build::build_app;
pub use serve::serve;
