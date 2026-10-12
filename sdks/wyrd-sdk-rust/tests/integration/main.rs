//! `wyrd-sdk-rust` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule is one story from
//! `fixtures/README.md`; nextest selects a story with
//! `-E 'test(/^<module>::/)'`.

mod gateway_admin;
mod gateway_inference;
/// The admin-key local workflow, start to finish, without a publication flush.
mod local_development;
mod observe_a_run;
mod operator_connections;
mod otel_export;
/// Principal discovery and Role assignment through `Principals`.
mod principal_roles;
mod query_bifrost;
mod register_and_hydrate;
mod saved_user_auth;
mod scheduled_drift_alerts_operator;
/// The same workflow from a saved login, past access-token expiry.
mod signed_in_development;
mod support;
/// The canonical support desk, from the checked-in example.
mod support_desk;
mod verify_in_real_time;
mod workflow_loading;
