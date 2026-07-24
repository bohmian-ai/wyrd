//! Vala OLAP read engine built on Apache Iceberg and `DataFusion`.
//!
//! `vala-bifrost` provides the `WyrdCatalog` entry point that manages Iceberg
//! table registration and `DataFusion` `TableProvider`s with built-in tenant
//! isolation. Current writes and maintenance are owned by Redux Scribe and Forge.

pub mod catalog;
pub mod error;
pub mod provider;
pub mod registry;
pub mod schema;
pub mod serving;
pub mod session;
pub mod tables;
pub mod types;

pub use catalog::WyrdCatalog;
pub use catalog::namespaces::BifrostNamespace;
pub use error::BifrostError;
pub use types::{OlapTableKind, PartitionTransform, SchemaFingerprint, TableScope, TableUid};
