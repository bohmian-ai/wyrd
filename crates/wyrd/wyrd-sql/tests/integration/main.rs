//! `wyrd-sql` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod pg_admin_principals;
mod pg_cards_declare;
/// Code-review cache metadata: pointer publication, collection claims and context expiry.
mod pg_code_review_cache;
mod pg_drift_baselines;
mod pg_gateway_usage;
/// Tenant GitHub installation slots, revision fences and login state.
mod pg_github_installations;
mod pg_login_state;
mod pg_migration;
mod pg_operator_connections;
mod pg_platform_identity;
mod pg_query_compile;
mod pg_tenant_slug;
mod pg_verification_bindings;
mod pg_verifier_executions;
