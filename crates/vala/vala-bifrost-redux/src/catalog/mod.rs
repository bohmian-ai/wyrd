//! Redux-owned tenant-qualified catalog and physical table identity.

mod bifrost_catalog;
mod error;
mod iceberg_sql;
pub mod partition_spec;
mod storage;
pub mod table_ref;
pub mod tenant_table;
mod wire;

pub use bifrost_catalog::{BifrostCatalog, CreateTableRequest, PinnedSealedTable, TableUid};
pub use error::BifrostCatalogError;
pub use partition_spec::{PartitionTransform, build_partition_spec};
pub use table_ref::TableRef;
pub use tenant_table::{TenantTableBinding, TenantTableBindingError, TenantTableKey};
