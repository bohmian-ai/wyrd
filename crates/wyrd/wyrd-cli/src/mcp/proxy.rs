//! `wyrd mcp proxy`: a host's stdio MCP session bridged onto Wyrd's `/mcp`.
//!
//! Hosts launch local MCP servers over stdio and speak whichever protocol
//! revision they ship; Wyrd serves one session-free revision over Streamable
//! HTTP behind its authenticated edge. The proxy is the protocol bridge
//! between the two and nothing more: it forwards tool discovery and tool
//! calls verbatim to the server, which owns the catalog, permissions, tenancy,
//! errors, and audit. Credentials never pass through here — the upstream
//! transport is [`WyrdMcpHttpClient`], which takes its bearer, renewal, and
//! refusal replay from the shared [`wyrd_client::WyrdClient`].

use std::process::ExitCode;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ErrorData, InitializeResult, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, ServiceError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::io::stdio;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{ClientLifecycleMode, ClientServiceExt as _, Peer, RoleClient, RoleServer};
use rmcp::{ServerHandler, ServiceExt as _};
use wyrd_mcp::client::WyrdMcpHttpClient;

use crate::client::from_global;
use crate::error::{CliBoundaryError, WyrdCliError};

/// The host-facing MCP server: every request is answered by the upstream
/// Wyrd session.
struct McpProxy {
    /// The established `/mcp` client session tool requests are forwarded on.
    upstream: Peer<RoleClient>,
}

impl ServerHandler for McpProxy {
    /// Advertise tools, the only capability Wyrd's `/mcp` serves.
    ///
    /// The protocol revision is negotiated with the host independently of the
    /// upstream session, so a host pinned to an earlier revision still reaches
    /// Wyrd's session-free one.
    fn get_info(&self) -> ServerInfo {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = self
            .upstream
            .peer_info()
            .and_then(|info| info.instructions.clone());
        info
    }

    /// Return the server's own catalog for the authenticated caller.
    ///
    /// # Errors
    /// Returns the server's protocol error, or an internal error describing
    /// an upstream transport failure.
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.upstream
            .list_tools(request)
            .await
            .map_err(upstream_error)
    }

    /// Forward one tool call and return the server's response unchanged.
    ///
    /// Domain failures, including permission refusals, are the server's
    /// structured tool results and pass through as such.
    ///
    /// # Errors
    /// Returns the server's protocol error, or an internal error describing
    /// an upstream transport failure.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.upstream
            .call_tool_once(request)
            .await
            .map_err(upstream_error)
    }
}

/// Carry an upstream failure back to the host.
///
/// A server protocol error is forwarded as-is; a transport failure becomes an
/// internal error whose message is the transport's own, which names the
/// failing request but never a credential.
fn upstream_error(error: ServiceError) -> ErrorData {
    match error {
        ServiceError::McpError(error) => error,
        other => ErrorData::internal_error(format!("Wyrd server request failed: {other}"), None),
    }
}

/// Run the proxy until the host closes its stdio session.
///
/// The client is built from the ambient configuration with `server` as an
/// explicit endpoint override, so a host connection retains the URL it was
/// installed with and never consults or rewrites the global endpoint. The
/// credential is exercised once before any protocol traffic, so a missing,
/// unusable, or unreachable credential exits with a stable error on stderr
/// rather than a host-side protocol timeout. The upstream session then uses
/// Wyrd's session-free `server/discover` lifecycle, and the host's stdio
/// session is served until the host disconnects.
///
/// # Errors
/// Returns the client-assembly error of [`crate::client::from_global`], the
/// server's stable error when it refuses the credential or cannot be reached,
/// and [`WyrdCliError::McpProxy`] when either MCP session cannot start.
pub(super) async fn run(server: Option<&str>) -> Result<ExitCode, CliBoundaryError> {
    let client = from_global(server)?;
    client.access_token().await?;
    let transport = StreamableHttpClientTransport::with_client(
        WyrdMcpHttpClient::new(&client),
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", client.server_url())),
    );
    let upstream = ()
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .map_err(|error| WyrdCliError::McpProxy {
            detail: format!("Wyrd MCP session failed: {error}"),
        })?;
    let host = McpProxy {
        upstream: upstream.peer().clone(),
    }
    .serve(stdio())
    .await
    .map_err(|error| WyrdCliError::McpProxy {
        detail: format!("host MCP session failed: {error}"),
    })?;
    let _ = host.waiting().await;
    let _ = upstream.cancel().await;
    Ok(ExitCode::SUCCESS)
}
