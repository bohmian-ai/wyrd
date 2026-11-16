//! Tenant GitHub connection over MCP.
//!
//! Typed projections of the same [`GithubConnections`] operations HTTP
//! serves. The read is advertised to every caller; connect and disconnect
//! appear only to a caller holding `identity_connections:write`, and every
//! operation authorizes and audits that permission again when named directly.
//! Setup and the OAuth callback are browser redirects and have no tool.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde_json::{Map as JsonMap, Value as JsonValue};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::github_connection::{GithubConnectResponse, GithubConnectionView};

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

/// Arguments of every GitHub tool: none.
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
    /// `name` is one of this module's three wire names; the caller routes
    /// only those here, so the final arm is [`DISCONNECT`].
    ///
    /// # Errors
    /// Returns a Wyrd error when the arguments are not empty or the owning
    /// [`GithubConnections`] operation refuses or fails.
    pub(super) async fn mcp_github(
        &self,
        name: &str,
        caller: Caller,
        arguments: Option<JsonMap<String, JsonValue>>,
    ) -> Result<CallToolResult, WyrdError> {
        let _: NoArgs = parse_args(Some(arguments.unwrap_or_default()), name)?;
        let connections = GithubConnections::new(&self.state);
        match name {
            GET => structured(&connections.get(&caller).await?),
            CONNECT => structured(&connections.connect(&caller).await?),
            _ => structured(&connections.disconnect(&caller).await?),
        }
    }
}
