//! Tenant GitHub connection over MCP.
//!
//! Typed projections of the same [`GithubConnections`] operations HTTP
//! serves. The read is advertised to every caller; connect and disconnect
//! appear only to a caller holding `identity_connections:write`, the
//! pull request review request only to one holding any `verifier:run`, and
//! every operation authorizes and audits its permissions again when named
//! directly.
//! Setup and the OAuth callback are browser redirects and have no tool.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde_json::{Map as JsonMap, Value as JsonValue};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::github_connection::{
    GithubConnectResponse, GithubConnectionView, GithubPullRequestReviewRequest,
    GithubPullRequestReviewResponse,
};
use wyrd_spec::ids::{IdempotencyKey, RepositoryId};

use super::principals::{parse_args, tool};
use super::{WyrdMcpHandler, structured};
use crate::components::auth::Caller;
use crate::components::github::service::GithubConnections;

/// Wire name of the connection read.
pub(super) const GET: &str = "github_connection.get";

/// Wire name of beginning a connection.
pub(super) const CONNECT: &str = "github_connection.connect";

/// Wire name of the disconnect.
pub(super) const DISCONNECT: &str = "github_connection.disconnect";

/// Wire name of the pull request review request.
pub(super) const REVIEW: &str = "github_connection.review_pull_request";

/// Arguments of [`REVIEW`]: the HTTP body and its `Idempotency-Key`.
#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReviewArgs {
    /// The configured GitHub web host, such as `github.com`.
    host: String,
    /// GitHub's numeric id of the pull request's base repository.
    repository_id: RepositoryId,
    /// Pull request number.
    number: u64,
    /// Names the request: a retry with the same key for the same pull
    /// request returns the same generation and members; a distinct key
    /// admits a new generation.
    idempotency_key: String,
}

/// Arguments of the connection tools: none.
#[derive(Debug, serde::Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

/// The read tool every authenticated caller may be shown.
#[must_use]
pub(super) fn descriptors_unscoped() -> Vec<Tool> {
    vec![tool::<NoArgs, GithubConnectionView>(
        GET,
        "Read the GitHub connection",
        "Read this tenant's GitHub App connection: configured host and App, state, verified \
         installation id and the selected repositories it can reach. No token or key is ever \
         returned. Requires identity_connections:write.",
        true,
    )]
}

/// The write tools shown only to a caller holding
/// `identity_connections:write`.
#[must_use]
pub(super) fn write_descriptors() -> Vec<Tool> {
    vec![
        tool::<NoArgs, GithubConnectResponse>(
            CONNECT,
            "Begin a GitHub connection",
            "Begin a one-use GitHub connection transaction and return GitHub's install URL and \
             its expiry. A human signed in to Wyrd as this same principal must open the URL in \
             a browser to install and authorize the App; nothing is connected until then. \
             Requires identity_connections:write.",
            false,
        ),
        tool::<NoArgs, GithubConnectionView>(
            DISCONNECT,
            "Disconnect GitHub",
            "Fence this tenant's GitHub work and release its installation, returning the \
             released view. Requires identity_connections:write.",
            false,
        ),
    ]
}

/// The review request tool, shown only to a caller holding any
/// `verifier:run` grant.
#[must_use]
pub(super) fn review_descriptors() -> Vec<Tool> {
    vec![tool::<ReviewArgs, GithubPullRequestReviewResponse>(
        REVIEW,
        "Request a pull request review",
        "Request a fresh review of one connected, open, ready pull request by its selected \
         Repository's whole pull_request binding set, even at an unchanged head, and return \
         the accepted generation and every member execution, refused members included. \
         host is the connection's host; repository_id is GitHub's numeric repository id. \
         Reuse idempotency_key to retry the same request; a new key admits a new generation \
         and supersedes the older one. Members run under the Repository declaration's \
         authority. Requires cards:read, and verifier:read and verifier:run on every selected \
         Verifier; WYRD_GITHUB_409_PULL_REQUEST_NOT_REVIEWABLE (details.reason not_found, \
         closed, draft or no_bindings) accepts nothing.",
        false,
    )]
}

/// Whether this caller's token carries `identity_connections:write`.
pub(super) fn may_manage(caller: &Caller) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission::identity_connections_write())
}

impl WyrdMcpHandler {
    /// Dispatch one GitHub tool by wire name.
    ///
    /// `name` is one of this module's four wire names; the caller routes
    /// only those here, so the final arm is [`DISCONNECT`].
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are invalid, the review key
    /// is invalid, or the owning [`GithubConnections`] operation refuses or
    /// fails.
    pub(super) async fn mcp_github(
        &self,
        name: &str,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let connections = GithubConnections::new(&self.state);
        if name == REVIEW {
            let args: ReviewArgs = parse_args(Some(arguments.unwrap_or_default()), name)?;
            let key = IdempotencyKey::new(args.idempotency_key).map_err(|error| {
                WyrdError::Validation {
                    message: format!("idempotency_key is invalid: {error}"),
                    details: serde_json::json!({ "tool": REVIEW }),
                }
            })?;
            let request = GithubPullRequestReviewRequest {
                host: args.host,
                repository_id: args.repository_id,
                number: args.number,
            };
            return structured(
                &connections
                    .review_pull_request(&caller, &request, &key)
                    .await?,
            );
        }
        let _: NoArgs = parse_args(Some(arguments.unwrap_or_default()), name)?;
        match name {
            GET => structured(&connections.get(&caller).await?),
            CONNECT => structured(&connections.connect(&caller).await?),
            _ => structured(&connections.disconnect(&caller).await?),
        }
    }
}
