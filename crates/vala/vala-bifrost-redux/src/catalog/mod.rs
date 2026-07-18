//! Catalog module — table reference and partition spec builders.

pub mod partition_spec;
pub mod table_ref;
pub mod tenant_table;

pub use partition_spec::build_partition_spec;
pub use table_ref::TableRef;
pub use tenant_table::{TenantTableBinding, TenantTableBindingError, TenantTableKey};
