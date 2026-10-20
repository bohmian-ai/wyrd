//! `wyrd-server` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod identity_e2e;
mod identity_ui_e2e;
/// Rust-owned opaque browser sessions through `/auth/browser`.
mod pg_browser_sessions;
mod pg_card_registration_route;
// Inspects recovered WAL state through `replay_wal_directory`, which exists only
// under `vala-bifrost-redux/test-support`; this crate's `test-support` feature
// forwards it.
#[cfg(feature = "test-support")]
mod pg_grpc_ingest_smoke;
mod pg_grpc_smoke;
/// Registered Artifact evidence and code through the hosted runtime.
mod pg_hosted_artifacts;
mod pg_merge_http_protected;
mod pg_openapi_contract;
mod pg_operator_connection_routes;
#[cfg(feature = "test-support")]
mod pg_operator_delivery;
/// Principal discovery and Role assignment over the authenticated HTTP routes.
mod pg_principal_roles;
mod pg_router_smoke;
mod pg_verification_routes;
// Arms one-shot query faults, which exist only under `test-support`.
#[cfg(feature = "test-support")]
mod pg_verification_groups;
#[cfg(feature = "test-support")]
mod pg_verification_runtime;
#[cfg(feature = "test-support")]
mod platform_admin_e2e;
/// The Python sandbox against the real hardened worker.
mod sandbox_worker;
#[cfg(feature = "storage-emulator")]
mod storage_e2e;
