//! Bifrost Redux — isolated rebuild of the Bifrost ingest and query engine.
//!
//! This crate implements the multi-pod Scribe (WAL + memtable + Parquet seal),
//! Forge (single-writer Iceberg committer + compaction), Oracle (fused scan + read
//! audit), and Gate (auth + dispatch) roles without depending on `vala-bifrost`.
//!
//! Per CONTRACTS §13, this crate MUST NOT depend on `vala-bifrost` in any form.
//! Shared logic is copied, not imported.

pub mod catalog;
pub mod contracts;
#[cfg(feature = "scribe-inspect")]
pub mod inspect;
pub mod namespaces;
pub mod parquet;
pub mod provider;
pub mod schema;
pub mod scribe;
