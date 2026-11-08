//! `wyrd-sdk-rust` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule is one story from
//! `fixtures/README.md`; nextest selects a story with
//! `-E 'test(/^<module>::/)'`.

/// One Service action read back as one connected trace timeline.
mod action_timeline;
mod declare_and_pull;
/// Queued work, acknowledged only once durably admitted.
mod durable_verifier_work;
mod gateway_admin;
mod gateway_inference;
/// TypeSafe Jev through the Gateway, natively and through Chat Completions.
mod gateway_jev;
/// Agents write private scratch and publish it for the steps that read it.
mod hosted_agent_tools;
/// The admin-key local workflow, start to finish, without a publication flush.
mod local_development;
mod observe_a_run;
mod operator_connections;
mod otel_export;
/// Principal discovery and Role assignment through `Principals`.
mod principal_roles;
mod query_bifrost;
/// Common analytical SQL over part-published, part-live tables.
mod query_coverage;
mod saved_user_auth;
mod scheduled_drift_alerts_operator;
/// Every handle's `connect` forms against a real server.
mod sdk_client_instantiation;
/// The same workflow from a saved login, past access-token expiry.
mod signed_in_development;
mod support;
/// The canonical support desk, from the checked-in example.
mod support_desk;
/// Every Verifier action and Run check kind, and every extractable format.
mod verifier_building_blocks;
/// Verifier steps fan out, branch, repeat, and speculate over real Agents.
mod verifier_orchestration;
mod verify_in_real_time;
