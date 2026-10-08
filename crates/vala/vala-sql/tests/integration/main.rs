//! `vala-sql` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod pg_file_list_cluster_nodes;
mod pg_forge_file_list;
mod pg_forge_leader;
mod pg_forge_operations;
mod pg_forge_tasks;
mod pg_maintenance_leases;
mod pg_migration;
mod pg_olap_catalog;
mod pg_oracle_membership;
mod pg_schema_usage;
mod pg_stream_identity;
