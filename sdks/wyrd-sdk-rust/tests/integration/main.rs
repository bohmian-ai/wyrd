//! `wyrd-sdk-rust` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule is one story from
//! `fixtures/README.md`; nextest selects a story with
//! `-E 'test(/^<module>::/)'`.

mod drift_verification;
mod gateway_admin;
mod observe_a_run;
mod operator_connections;
mod query_bifrost;
mod register_and_hydrate;
mod saved_user_auth;
mod support;
mod workflow_loading;
