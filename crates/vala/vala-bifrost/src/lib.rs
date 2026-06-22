//! Vala OLAP write/read engine built on Apache Iceberg and DataFusion.
//!
//! `vala-bifrost` provides the `WyrdCatalog` entry point that manages Iceberg
//! tables for TenantOwned and SystemShared OLAP workloads, a 2PC commit
//! pipeline, and DataFusion `TableProvider`s with built-in tenant isolation.

pub mod batch_builder;
pub mod catalog;
pub mod error;
pub mod provider;
pub mod registry;
pub mod schema;
pub mod session;
pub mod types;
pub mod writer;

#[cfg(feature = "python")]
mod python;

pub use catalog::WyrdCatalog;
pub use error::BifrostError;
pub use types::{OlapTableKind, PartitionTransform, SchemaFingerprint, TableScope, TableUid};
