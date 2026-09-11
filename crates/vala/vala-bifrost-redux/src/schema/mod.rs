//! Schema utilities — fingerprinting and system columns.

pub mod fingerprint;
pub mod managed_columns;

pub use fingerprint::SchemaFingerprint;
pub use managed_columns::with_managed_columns;
