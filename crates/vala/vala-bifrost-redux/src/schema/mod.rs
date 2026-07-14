//! Schema utilities — fingerprinting and system columns.

pub mod fingerprint;
pub mod system_columns;

pub use fingerprint::SchemaFingerprint;
pub use system_columns::with_system_columns;
