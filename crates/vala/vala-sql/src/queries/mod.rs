//! SQL query modules for Vala-owned observability storage.
//!
//! Future Vala query concerns are tenant-scoped by default and take
//! [`crate::TenantConn`]. Runtime queries must schema-qualify table references
//! and retain explicit `data_tenant_id = $...` predicates alongside RLS. They
//! must not open nested transactions, use savepoints, or call Wyrd query write
//! functions to coordinate one cross-crate transaction; Wyrd-to-Vala effects
//! are propagated after commit through the future outbox path.

pub mod alerts;
pub mod anchors;
pub mod audit_outbox;
pub mod cluster_nodes;
pub mod drift_alerts;
pub mod file_list;
pub mod forge_catalog_operator;
pub mod forge_operations;
pub mod forge_tasks;
pub mod maintenance_leases;
pub mod monitor;
pub mod olap_catalog;
pub mod oracle_admission;
pub mod profiles;
pub mod queues;
pub mod scribe_batch_commits;
