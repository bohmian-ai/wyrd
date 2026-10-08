//! `wyrd-sdk-rust` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule is one story from
//! `fixtures/README.md`; nextest selects a story with
//! `-E 'test(/^<module>::/)'`.

mod gateway_admin;
mod gateway_inference;
mod grant_role;
mod observe_a_run;
mod operator_connections;
mod otel_export;
mod query_bifrost;
mod register_and_hydrate;
mod saved_user_auth;
mod scheduled_drift_alerts_operator;
mod support;
mod verify_in_real_time;
mod workflow_loading;
