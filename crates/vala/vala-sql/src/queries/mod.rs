//! SQL query modules for Vala-owned observability storage.
//!
//! Future Vala query concerns are tenant-scoped by default and take
//! [`crate::TenantConn`]. Runtime queries must schema-qualify table references
//! and retain explicit `data_tenant_id = $...` predicates alongside RLS.

pub mod alerts;
pub mod anchors;
pub mod monitor;
pub mod olap_catalog;
pub mod profiles;
pub mod queues;
