//! Redux-owned tenant-qualified catalog and physical table identity.

/// Exact wire name of the single Iceberg catalog every Bifrost table lives in.
///
/// Forge task identities, reader protection headers, and the loaded
/// `SqlCatalog` all persist this string, and a mismatch between any two of them
/// is a table-identity error rather than a naming preference. Keeping one
/// source constant is what lets a durable row assert catalog equality instead
/// of trusting a literal repeated at each call site.
pub const BIFROST_CATALOG_NAME: &str = "wyrd-redux";

mod bifrost_catalog;
mod error;
pub mod event_time;
mod iceberg_sql;
mod iceberg_storage;
pub mod layout;
mod logical_table_identity;
mod storage;
pub mod table_ref;
pub mod tenant_table;
mod wire;

pub use bifrost_catalog::{
    BifrostCatalog, CreateTableRequest, PinnedIcebergFile, PinnedSealedTable,
    PreparedReaderIdentity, TableUid,
};
#[cfg(any(test, feature = "test-support"))]
pub use bifrost_catalog::{
    inject_revalidation_faults_for_test, pending_revalidation_faults_for_test,
    prepared_identity_count_for_test, reset_prepared_identity_count_for_test,
    reset_sealed_pin_count_for_test, sealed_pin_count_for_test,
};
pub use error::BifrostCatalogError;
pub use layout::{
    LayoutSortKey, NullOrder, PhysicalLayout, SortDirection, TimeGranularity, TimePartition,
    TimePartitionError, TimePartitionSpec,
};
pub(crate) use logical_table_identity::{
    LogicalTableIdentity, PhysicalProjectionRole, PhysicalTableProjection,
};
pub use table_ref::TableRef;
pub(crate) use tenant_table::PhysicalBindingFacts;
pub use tenant_table::{TenantTableBinding, TenantTableBindingError, TenantTableKey};
