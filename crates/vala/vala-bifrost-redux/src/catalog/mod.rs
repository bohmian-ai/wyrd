//! Redux-owned tenant-qualified catalog and physical table identity.

mod bifrost_catalog;
mod error;
mod iceberg_sql;
mod logical_table_identity;
pub mod partition_spec;
mod storage;
pub mod table_ref;
pub mod tenant_table;
mod wire;

pub(crate) use bifrost_catalog::schema_shape_matches;
pub use bifrost_catalog::{
    BifrostCatalog, CreateTableRequest, PinnedIcebergFile, PinnedSealedTable, TableUid,
    project_persisted_wal_ranges,
};
#[cfg(any(test, feature = "test-support"))]
pub use bifrost_catalog::{reset_sealed_pin_count_for_test, sealed_pin_count_for_test};
pub use error::BifrostCatalogError;
pub(crate) use logical_table_identity::{
    LogicalTableIdentity, PhysicalProjectionRole, PhysicalTableProjection,
};
pub use partition_spec::{PartitionTransform, build_partition_spec};
pub use table_ref::TableRef;
pub(crate) use tenant_table::PhysicalBindingFacts;
pub use tenant_table::{TenantTableBinding, TenantTableBindingError, TenantTableKey};
