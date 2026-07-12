//! Audit log sealing: range signing, sealed-history verification, and export.
//!
//! The `worker` module seals a contiguous `[seq_lo, seq_hi]` range of shipped
//! audit rows by computing a SHA256 range hash over the ordered column bytes
//! and signing it with the dedicated `AuditSealKey` (Ed25519, domain separator
//! `wyrd.audit.seal.v1\0`). Seal checkpoints are persisted in
//! `vala.audit_seal_checkpoints`.
//!
//! The `verify` module recomputes each range hash from the Iceberg `audit_log`
//! table and re-checks the checkpoint signature; a tampered range degrades the
//! per-tenant audit health.
//!
//! The `export` module lists sealed and shipped rows for archival use.

pub mod export;
pub mod verify;
pub mod worker;
