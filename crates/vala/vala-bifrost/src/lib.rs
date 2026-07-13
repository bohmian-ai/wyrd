//! Vala OLAP write/read engine built on Apache Iceberg and `DataFusion`.
//!
//! `vala-bifrost` provides the `WyrdCatalog` entry point that manages Iceberg
//! tables for `TenantOwned` and `SystemShared` OLAP workloads, a 2PC commit
//! pipeline, and `DataFusion` `TableProvider`s with built-in tenant isolation.

pub mod batch_builder;
pub mod catalog;
pub mod error;
pub mod provider;
pub mod reconcile;
pub mod registry;
pub mod relay;
pub mod schema;
pub mod serving;
pub mod session;
pub mod tables;
pub mod types;
pub mod writer;

pub use catalog::WyrdCatalog;
pub use catalog::namespaces::BifrostNamespace;
pub use error::BifrostError;
pub use types::{OlapTableKind, PartitionTransform, SchemaFingerprint, TableScope, TableUid};
