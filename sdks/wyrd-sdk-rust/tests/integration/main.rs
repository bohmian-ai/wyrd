//! `wyrd-sdk-rust` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod cards_state;
mod drift_verification;
mod gateway_admin;
mod observe_run;
mod operator_connections;
mod verification_run;
mod workflow_loading;
