//! Tenant GitHub App connection: the deployment's configured App, a tenant's
//! verified installation association, and installation tokens for background
//! work.
//!
//! [`app::GithubApp`] owns the App's credentials and every GitHub call;
//! [`service::GithubConnections`] owns connect, read, disconnect, the setup
//! and OAuth callback redirects, webhooks and installation tokens; HTTP and
//! MCP are thin projections of it.

/// The deployment GitHub App's provider protocol.
pub mod app;
/// HTTP projection of the tenant GitHub connection.
pub mod routes;
/// The one owner of tenant GitHub connection state, shared by HTTP and MCP.
pub mod service;

pub use app::GithubApp;
pub use routes::{github_router, github_webhook_router};
