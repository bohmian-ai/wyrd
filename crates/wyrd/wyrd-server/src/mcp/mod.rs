//! Wyrd's Model Context Protocol adapter.
//!
//! One [`rmcp`] Streamable HTTP endpoint is mounted at `/mcp` on the existing
//! public listener. `rmcp` owns framing, MCP headers, protocol negotiation, and
//! request cancellation; this module owns only the adapter edge — reading the
//! already-verified Wyrd request context out of request extensions, authorizing
//! and auditing each call, holding a shutdown-observed task token for the life
//! of the work, and translating public [`WyrdError`] values into
//! protocol-correct MCP errors.
//!
//! The production catalog exposes three read-only Bifrost tools for table
//! discovery, schema/layout description, and bounded terminal-safe queries,
//! followed by the seventeen tenant gateway administration tools: seven reads,
//! five idempotent replacements, and five destructive revocations or deletions.
//! A test-support context probe is available only through explicit fixture opt-in.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, ErrorCode, ErrorData, InitializeResult,
    ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{RoleServer, ServerHandler};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

use crate::components::auth::{AuthenticatedPrincipal, Caller};
use crate::state::AppState;

mod bifrost;
mod gateway;
mod principals;
#[cfg(feature = "test-support")]
pub mod probe;

/// The only MCP protocol revision Wyrd serves.
///
/// Narrowing the advertised set is what lets the transport require per-request
/// protocol metadata: an rmcp client negotiated below this revision does not
/// attach it, so accepting older revisions would mean accepting requests the
/// stateless validator then rejects.
///
/// Serving exactly one revision also fixes the lifecycle: `initialize`
/// negotiates only down to a pre-2026-07-28 revision, so a client reaches
/// `/mcp` through `server/discover` plus self-contained per-request protocol
/// metadata. That is the same session-free shape the stateless transport below
/// is configured for.
///
/// Wyrd does not serve the older lifecycle, and that is deliberate rather than
/// pending: a client that cannot speak 2026-07-28 cannot reach `/mcp` at all.
/// The cost is interoperability with MCP clients pinned to an earlier
/// revision; the return is one protocol shape with no session affinity, no
/// per-revision branch in the adapter, and per-request metadata the stateless
/// validator can require. Widening this set is a product decision about which
/// agents Wyrd serves, not a local change here — every value added has to
/// carry its own lifecycle through the same authorization, audit, tenancy, and
/// cancellation path.
const WYRD_MCP_PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion::V_2026_07_28;

/// The concrete Wyrd MCP request handler.
///
/// It owns the process's [`AppState`] and nothing else: identity, tenancy, and
/// authorization all come from the verified request context rather than from
/// handler-local state, so one instance per request is free to construct.
#[derive(Clone)]
pub struct WyrdMcpHandler {
    /// Shared server state used for authorization, audit, and task tracking.
    state: AppState,
}

impl WyrdMcpHandler {
    /// Build a handler bound to this process's server state.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Recover the verified Wyrd request context for the current MCP request.
    ///
    /// The Streamable HTTP transport stores the original [`axum::http::request::Parts`]
    /// in the request's extensions, so the values the `/mcp` edge already
    /// verified — [`AuthenticatedPrincipal`] from `require_authenticated` and
    /// the [`RequestId`] minted by `attach_request_id` — are read back here
    /// rather than reconstructed. Nothing in the tool arguments participates.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when the transport did not carry the
    /// HTTP parts, or when either extension is absent — both mean the route was
    /// mounted outside the layers that produce them, which is a wiring failure
    /// rather than a client error.
    fn request_context(
        context: &RequestContext<RoleServer>,
    ) -> Result<(AuthenticatedPrincipal, RequestId), WyrdError> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| missing_context("axum::http::request::Parts"))?;
        let principal = parts
            .extensions
            .get::<AuthenticatedPrincipal>()
            .cloned()
            .ok_or_else(|| missing_context("AuthenticatedPrincipal"))?;
        let request_id = parts
            .extensions
            .get::<RequestId>()
            .cloned()
            .ok_or_else(|| missing_context("RequestId"))?;
        Ok((principal, request_id))
    }

    /// Derive the tenant-scoped [`Caller`] every Wyrd service operation takes.
    ///
    /// The whole verified principal is handed to
    /// [`Caller::from_authenticated`] so the token's delegation chain reaches
    /// the operation's audit record alongside the effective principal that
    /// authorizes it.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::request_context`]'s wiring failures.
    fn caller(context: &RequestContext<RoleServer>) -> Result<Caller, WyrdError> {
        let (principal, request_id) = Self::request_context(context)?;
        Ok(Caller::from_authenticated(&principal, request_id))
    }

    /// The tools this handler advertises to every authenticated caller.
    ///
    /// Ordinary startup — production and an ordinary `WyrdTestServer` alike —
    /// advertises the three read-only Bifrost tools, the read-only principal
    /// credential listing, and the gateway tools, whose replacement and delete
    /// operations require explicit gateway write or delete scopes at dispatch.
    /// Principal write tools are per caller and the test-support probe is
    /// opt-in, so neither belongs here.
    fn catalog(&self) -> Vec<Tool> {
        let mut catalog = bifrost::descriptors();
        catalog.extend(principals::descriptors_unscoped());
        catalog.extend(gateway::descriptors());
        catalog
    }

    /// The test-support context probe, when this server opted into it.
    ///
    /// Kept out of [`Self::catalog`] so the probe trails the complete shipped
    /// catalog — read tools, then the caller's write tools — rather than
    /// splitting it. Compiling `test-support` is not enough to expose it; a
    /// test server must ask for it explicitly.
    fn probe_descriptors(&self) -> Vec<Tool> {
        #[cfg(feature = "test-support")]
        if self.state.mcp_context_probe {
            return vec![probe::descriptor()];
        }
        Vec::new()
    }
}

impl ServerHandler for WyrdMcpHandler {
    /// Advertise Wyrd's tools capability and sole supported protocol revision.
    fn get_info(&self) -> ServerInfo {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.protocol_version = WYRD_MCP_PROTOCOL_VERSION;
        info
    }

    /// Restrict negotiation to the revision requiring stateless request metadata.
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(vec![WYRD_MCP_PROTOCOL_VERSION])
    }

    /// Resolve only tools in this process's configured Wyrd catalog.
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.catalog()
            .into_iter()
            .chain(principals::write_descriptors())
            .chain(self.probe_descriptors())
            .find(|tool| tool.name == name)
    }

    /// Return the complete bounded catalog after public-edge authentication.
    ///
    /// # Errors
    /// This hook is infallible; authentication failures are handled by the edge.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        // The advertised catalog is per caller, not per process: a write tool
        // appears only to a caller whose verified token already carries the
        // permission it needs. An agent therefore never discovers a capability
        // it cannot use, and naming one anyway is still refused at dispatch.
        let mut catalog = self.catalog();
        if let Ok(caller) = Self::caller(&context)
            && principals::may_administer(&caller)
        {
            catalog.extend(principals::write_descriptors());
        }
        catalog.extend(self.probe_descriptors());
        Ok(ListToolsResult::with_all_items(catalog))
    }

    /// Dispatch tools with verified request context and retain the shutdown
    /// tracker token through execution and cancellation settlement.
    ///
    /// # Errors
    /// Returns invalid parameters for unknown tools, mapped context errors when
    /// trusted extensions are missing, or the owning tool's protocol error.
    /// Domain failures remain structured tool results.
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        #[cfg(feature = "test-support")]
        if self.state.mcp_context_probe && request.name == probe::TOOL_NAME {
            let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
            let (principal, _) = Self::request_context(&context).map_err(wyrd_error_to_mcp)?;
            return probe::invoke(&self.state, caller, &principal, request, context)
                .await
                .map(CallToolResponse::Complete)
                .map_err(wyrd_error_to_mcp);
        }
        // Every Bifrost tool holds an MCP tracker token for its whole path, so
        // process shutdown waits for the work it cancelled rather than racing
        // the result back out.
        let _token = self.state.mcp_tasks.token();
        let outcome = match request.name.as_ref() {
            bifrost::LIST_TABLES => {
                let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
                self.list_tables(caller, request.arguments).await
            }
            bifrost::DESCRIBE_TABLE => {
                let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
                self.describe_table(caller, request.arguments).await
            }
            bifrost::QUERY => {
                let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
                self.query(caller, request.arguments, &context).await
            }
            principals::LIST_CREDENTIALS => {
                let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
                self.mcp_list_credentials(caller, request.arguments)
                    .await
                    .map_err(wyrd_error_to_mcp)
            }
            principals::REVOKE_CREDENTIAL => {
                let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
                // No scope check here. The operation this dispatches to
                // authorizes the same permission and records that decision in
                // the transaction that acts on it; a second check in front of
                // it would refuse identically while auditing nothing, so it
                // would only be a place for the two to drift apart.
                self.mcp_revoke_credential(caller, request.arguments)
                    .await
                    .map_err(wyrd_error_to_mcp)
            }
            name if gateway::TOOLS.contains(&name) => {
                let caller = Self::caller(&context).map_err(wyrd_error_to_mcp)?;
                self.gateway_tool(name, caller, request.arguments).await
            }
            unknown => {
                return Err(ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("unknown Wyrd MCP tool: {unknown}"),
                    None,
                ));
            }
        };
        outcome.map(CallToolResponse::Complete)
    }
}

/// Compose the single `/mcp` Streamable HTTP service for this process.
///
/// Sessions are disabled (`NeverSessionManager`, legacy session mode off) so
/// every request is served statelessly and no replica affinity is implied.
/// `Host` validation is disabled because Wyrd's public listener and deployed
/// gateway own remote host routing; `Origin` validation is disabled explicitly
/// because Wyrd has no authoritative browser-origin allowlist — the gateway,
/// TLS, and the mandatory JWT edge are the current trust boundary, and a
/// browser-origin policy needs its own approved configuration source.
///
/// The transport's cancellation token is the process
/// [`AppState::shutdown_token`], so process shutdown both stops admission and
/// cancels every in-flight MCP request context.
pub(crate) fn mcp_service(
    state: &AppState,
) -> StreamableHttpService<WyrdMcpHandler, NeverSessionManager> {
    let config = StreamableHttpServerConfig::default()
        .with_cancellation_token(state.shutdown_token.clone())
        .with_legacy_session_mode(false)
        .with_stateless_protocol_metadata_required(true)
        .disable_allowed_hosts()
        .disable_allowed_origins();
    let handler_state = state.clone();
    StreamableHttpService::new(
        move || Ok(WyrdMcpHandler::new(handler_state.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    )
}

/// Report a request-context extension the `/mcp` layer stack should have set.
fn missing_context(extension: &'static str) -> WyrdError {
    WyrdError::Internal {
        message: format!("MCP request is missing the {extension} extension"),
        details: serde_json::json!({ "extension": extension }),
    }
}

/// Project a public Wyrd error onto the MCP protocol error shape.
///
/// The complete RFC 9457 problem body — stable code, title, status, details,
/// and remediation — is preserved in `data` so an agent client sees the same
/// contract it would over HTTP; the JSON-RPC code carries only the coarse
/// client/server distinction the protocol defines.
pub(crate) fn wyrd_error_to_mcp(error: WyrdError) -> ErrorData {
    let code = if error.status() >= 500 {
        ErrorCode::INTERNAL_ERROR
    } else {
        ErrorCode::INVALID_REQUEST
    };
    let message = error.to_string();
    ErrorData::new(code, message, Some(error.as_problem_json()))
}
