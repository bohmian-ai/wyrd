//! `wyrd-server` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod identity_e2e;
mod identity_ui_e2e;
mod pg_card_registration_route;
// Inspects recovered WAL state through `replay_wal_directory`, which exists only
// under `vala-bifrost-redux/test-support`; this crate's `test-support` feature
// forwards it.
#[cfg(feature = "test-support")]
mod pg_grpc_ingest_smoke;
mod pg_grpc_smoke;
mod pg_merge_http_protected;
mod pg_openapi_contract;
mod pg_operator_connection_routes;
#[cfg(feature = "test-support")]
mod pg_operator_delivery;
mod pg_router_smoke;
mod pg_verification_routes;
#[cfg(feature = "test-support")]
mod pg_verification_runtime;
mod pg_workflow_registration;
#[cfg(feature = "test-support")]
mod pg_workflow_runs;
mod platform_admin_e2e;
#[cfg(feature = "storage-emulator")]
mod storage_e2e;
