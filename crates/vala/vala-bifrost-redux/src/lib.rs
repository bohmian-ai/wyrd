//! Bifrost Redux — isolated rebuild of the Bifrost ingest and query engine.
//!
//! This crate implements the multi-pod Scribe (WAL + memtable + Parquet seal),
//! Forge (single-writer Iceberg committer + compaction), Oracle (fused scan + read
//! audit), and Gate (auth + dispatch) roles without depending on `vala-bifrost`.
//!
//! Per CONTRACTS §13, this crate MUST NOT depend on `vala-bifrost` in any form.
//! Shared logic is copied, not imported.

#[cfg(feature = "bench-support")]
pub mod bench_support;
pub mod catalog;
pub mod cluster;
pub mod contracts;
pub mod forge;
pub mod gate;
pub mod maintenance;
pub mod namespaces;
pub mod parquet;
pub mod provider;
pub mod schema;
pub mod scribe;
pub mod tables;

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::OnceLock;

    use wyrd_spec::DataTenantId;

    pub(crate) fn tenant() -> DataTenantId {
        static TENANT: OnceLock<DataTenantId> = OnceLock::new();
        *TENANT.get_or_init(DataTenantId::new_v7)
    }

    pub(crate) fn nil_tenant() -> DataTenantId {
        DataTenantId::SYSTEM_OWNER
    }
}
