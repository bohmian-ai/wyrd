//! Tenant GitHub App connection: the deployment's configured App, a tenant's
//! verified installation association, and installation tokens for background
//! work.
//!
//! [`app::GithubApp`] owns the App's credentials and every GitHub call;
//! [`service::GithubConnections`] owns connect, read, disconnect, the setup
//! and OAuth callback redirects, webhooks and installation tokens; HTTP and
//! MCP are thin projections of it, and [`pull_requests`] admits its
//! pull-request deliveries into the Verifier queue. [`review_cache`] keeps the code-review
//! pipeline's cache in Wyrd's object store and PostgreSQL.

/// The deployment GitHub App's provider protocol.
pub mod app;
/// Pull-request admission into the Verifier queue and failed-delivery
/// recovery.
pub mod pull_requests;
/// Wyrd custody of the code-review pipeline's object and metadata cache.
pub mod review_cache;
/// HTTP projection of the tenant GitHub connection.
pub mod routes;
/// The one owner of tenant GitHub connection state, shared by HTTP and MCP.
pub mod service;

pub use app::GithubApp;
pub use routes::{github_router, github_webhook_router};
