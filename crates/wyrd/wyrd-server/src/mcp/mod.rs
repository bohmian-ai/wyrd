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
//! discovery, schema/layout description, and bounded terminal-safe queries.
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
    /// # Errors
    ///
    /// Propagates [`Self::request_context`]'s wiring failures.
    fn caller(context: &RequestContext<RoleServer>) -> Result<Caller, WyrdError> {
        let (principal, request_id) = Self::request_context(context)?;
        let principal = wyrd_runtime::Principal::from(principal);
        Ok(Caller {
            data_tenant_id: principal.tenant_id,
            principal,
            request_id,
        })
    }

    /// The tools this handler advertises and accepts.
    ///
    /// Ordinary startup — production and an ordinary `WyrdTestServer` alike —
    /// advertises exactly the three read-only Bifrost tools. The test-support
    /// context probe joins that catalog only when a test server explicitly
    /// opted into it, so compiling `test-support` is not enough to expose it.
    fn catalog(&self) -> Vec<Tool> {
        let mut catalog = bifrost::descriptors();
        #[cfg(feature = "test-support")]
        if self.state.mcp_context_probe {
            catalog.push(probe::descriptor());
        }
        catalog
    }
}

impl ServerHandler for WyrdMcpHandler {
    fn get_info(&self) -> ServerInfo {
        let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
        info.protocol_version = WYRD_MCP_PROTOCOL_VERSION;
        info
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(vec![WYRD_MCP_PROTOCOL_VERSION])
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.catalog().into_iter().find(|tool| tool.name == name)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.catalog()))
    }

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
