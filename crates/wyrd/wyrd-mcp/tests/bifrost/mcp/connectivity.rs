//! Tier-1 journey: a real MCP client reaching Wyrd's `/mcp` endpoint.
//!
//! This binary proves the connectivity contract end to end — the first-party
//! Rust client transport, the public listener's authentication and correlation
//! edge, the `rmcp` Streamable HTTP service mounted inside it, and the request
//! and process cancellation that shutdown depends on. No Bifrost tool exists
//! yet; the server's test-support context probe stands in for one so the
//! journey observes exactly what the edge bound.

use http::{HeaderName, HeaderValue};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::{ClientConfig, TokenCacheMode};
use wyrd_client::principals::Principals;
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_mcp::client::WyrdMcpHttpClient;
use wyrd_spec::request_id::RequestId;
use wyrd_testing::WyrdTestServer;

/// Boxed error carried by every helper in this journey.
pub(crate) type McpJourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Read one tool call's structured content, failing on a tool-level error.
///
/// # Errors
///
/// Returns the structured Wyrd problem when the call reported `is_error`, and
/// a description when it carried no structured content at all.
pub(crate) fn structured(
    result: rmcp::model::CallToolResult,
) -> Result<serde_json::Value, McpJourneyError> {
    let content = result
        .structured_content
        .ok_or("a Wyrd MCP tool returns structured content")?;
    if result.is_error == Some(true) {
        return Err(format!("tool call failed: {content}").into());
    }
    Ok(content)
}

/// Read the canonical Wyrd problem from a call that must have failed.
///
/// # Errors
///
/// Returns a description when the call succeeded or carried no problem.
pub(crate) fn problem(
    result: rmcp::model::CallToolResult,
) -> Result<serde_json::Value, McpJourneyError> {
    let content = result
        .structured_content
        .ok_or("a failed Wyrd MCP tool returns its structured problem")?;
    if result.is_error != Some(true) {
        return Err(format!("tool call unexpectedly succeeded: {content}").into());
    }
    Ok(content)
}

/// Build the first-party Wyrd client for `server` using `credential`.
///
/// The one configured capability every journey surface is derived from: the
/// MCP transport takes its HTTP pool and credential from it, and an HTTP-only
/// operation speaks as the same principal over the same token cache. The
/// credential path is the production one — for an API key the middleware
/// exchanges it at the server's real `/auth/token` route, for a bearer it
/// carries the token as issued.
///
/// # Errors
///
/// Returns an error when the server is not bound to a listener or when the
/// auth or HTTP layers cannot be constructed for `credential`.
pub(crate) fn client(
    server: &WyrdTestServer,
    credential: ResolvedCredential,
) -> Result<WyrdClient, McpJourneyError> {
    let base_url = server
        .base_url()
        .ok_or("journey requires a bound test server")?
        .to_owned();
    let mut config = ClientConfig::default();
    config.http.base_url = base_url;
    config.token_cache = TokenCacheMode::InMemory;
    let auth = AuthMiddleware::new(&config, credential)?;
    let http = wyrd_client::transport::http::HttpTransport::new(
        &config.http,
        std::sync::Arc::clone(&auth),
    )?;
    Ok(wyrd_client::WyrdClient::from_parts(auth, http, config.grpc))
}

/// Build the first-party MCP client transport for `server` using `credential`.
///
/// The decorator is derived from [`client`], so the journey constructs no HTTP
/// client and writes no Wyrd header of its own: pool, TLS, token cache, and
/// the bounded re-exchange after a `401` are all the shipped client's.
///
/// When `request_id` is supplied it is seeded into the transport's custom
/// headers as `wyrd-request-id`, which the decorator preserves rather than
/// minting its own — the only way a test can join a durable audit row to the
/// exact request that produced it.
///
/// # Errors
///
/// Returns an error when the server is not bound to a listener or when the
/// client cannot be constructed for `credential`.
pub(crate) fn transport(
    server: &WyrdTestServer,
    credential: ResolvedCredential,
    request_id: Option<&RequestId>,
) -> Result<StreamableHttpClientTransport<WyrdMcpHttpClient>, McpJourneyError> {
    let base_url = server
        .base_url()
        .ok_or("journey requires a bound test server")?
        .to_owned();
    let mut transport_config =
        StreamableHttpClientTransportConfig::with_uri(format!("{base_url}/mcp"));
    if let Some(request_id) = request_id {
        transport_config.custom_headers.insert(
            HeaderName::from_static("wyrd-request-id"),
            HeaderValue::from_str(request_id.as_str())?,
        );
    }
    Ok(StreamableHttpClientTransport::with_client(
        WyrdMcpHttpClient::new(&client(server, credential)?),
        transport_config,
    ))
}

/// Build the first-party tenant principal handle for `server` using `credential`.
///
/// Shares the exact client [`transport`] gives the MCP surface, so a journey
/// that reaches for an HTTP-only operation — credential issuance, which MCP
/// deliberately does not expose — still speaks as the same principal over the
/// same credential path.
///
/// # Errors
///
/// Returns an error when the server is not bound to a listener or when the
/// client cannot be constructed for `credential`.
pub(crate) fn principals(
    server: &WyrdTestServer,
    credential: ResolvedCredential,
) -> Result<Principals, McpJourneyError> {
    Ok(wyrd_client::principals::Principals::with_client(client(
        server, credential,
    )?))
}

/// The modern, session-free MCP lifecycle: `server/discover` plus
/// self-contained per-request protocol metadata.
///
/// Wyrd serves exactly one protocol revision and mounts no session manager, so
/// the legacy `initialize` handshake has nothing to negotiate down to; this is
/// the only lifecycle a client can use against `/mcp`.
pub(crate) fn discover() -> rmcp::ClientLifecycleMode {
    rmcp::ClientLifecycleMode::Discover {
        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
    }
}

mod pg_tests {
    use super::{McpJourneyError, RequestId, ResolvedCredential, discover, transport};

    use std::time::Duration;

    use rmcp::ClientServiceExt as _;
    use rmcp::model::{CallToolRequest, CallToolRequestParams, ClientRequest};
    use rmcp::service::PeerRequestOptions;
    use wyrd_runtime::Permission;
    use wyrd_server::mcp::probe;
    use wyrd_spec::DataTenantId;
    use wyrd_testing::WyrdTestServer;

    /// A real `rmcp` client reaches the `/mcp` endpoint and sees the exact
    /// tenant and principal Wyrd's authentication edge verified for it.
    ///
    /// The probe reports only server-verified context, so a matching tenant and
    /// principal prove the whole chain — client transport headers, the public
    /// listener's `require_authenticated` layer, the request-id layer, and the
    /// handler's recovery of both from the transport's request extensions.
    /// The catalog assertion pins the other half of the contract: the probe is
    /// visible only to a fixture that explicitly opted in.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn streamable_http_client_reaches_authenticated_server_context()
    -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::builder()
            .with_mcp_context_probe_for_test()
            .start_bound()
            .await?;
        let initiator = server
            .bootstrap_service("mcp-initiator", &["runtime_admin"])
            .await?;
        let caller = server
            .bootstrap_service("mcp-connectivity", &["admin"])
            .await?;
        let initiator_jwt = server
            .exchange_api_key(
                initiator
                    .api_key()
                    .ok_or("service bootstrap carries a key")?,
            )
            .await?;
        let delegated_jwt = server
            .delegate(
                &initiator_jwt,
                caller.card_ref().ok_or("a service carries a card ref")?,
            )
            .await?;
        let client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(delegated_jwt.into()),
                    None,
                )?,
                discover(),
            )
            .await?;

        let tools = client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                "bifrost.list_tables",
                "bifrost.describe_table",
                "bifrost.query",
                "principals.list_credentials",
                "principals.revoke_credential",
                probe::TOOL_NAME,
            ],
            "an opted-in fixture advertises the context probe after the ordinary catalog"
        );

        let result = client
            .call_tool(CallToolRequestParams::new(probe::TOOL_NAME))
            .await?;
        assert_ne!(result.is_error, Some(true), "probe call succeeds");
        let context = result
            .structured_content
            .ok_or("probe returns structured content")?;
        assert_eq!(
            context["tenant_id"],
            serde_json::Value::String(server.data_tenant_id().to_string()),
            "the server binds its own verified tenant"
        );
        assert_eq!(
            context["principal_id"],
            serde_json::Value::String(caller.id().to_string()),
            "the server binds the verified caller"
        );
        assert_eq!(
            context["roles"],
            serde_json::json!(["admin"]),
            "the delegated bearer carries exactly the target's own roles"
        );
        assert_eq!(
            context["delegation_chain"],
            serde_json::json!([initiator.id().to_string()]),
            "the chain is initiator-first and stops at the single hop taken"
        );
        assert_eq!(context["cancelled"], serde_json::Value::Bool(false));
        assert!(
            context["request_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "the correlation id from the public edge reaches the handler"
        );

        client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// A fixture that did not opt in never advertises the test probe.
    ///
    /// What the ordinary catalog *does* contain is
    /// `discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout`'s
    /// claim; this one owns only the opt-in gate, which is the half that has to
    /// hold no matter what the Bifrost catalog grows to.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn default_server_advertises_no_test_probe() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let caller = server.bootstrap_service("mcp-default", &["admin"]).await?;
        let api_key = caller.api_key().ok_or("service bootstrap carries a key")?;
        let client = ()
            .serve_with_lifecycle(
                transport(&server, ResolvedCredential::ApiKey(api_key.clone()), None)?,
                discover(),
            )
            .await?;

        assert!(
            client
                .list_all_tools()
                .await?
                .iter()
                .all(|tool| tool.name != probe::TOOL_NAME),
            "the probe is reachable only through an explicit fixture opt-in"
        );

        client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }

    /// POST one MCP request straight through `reqwest`, bypassing `rmcp`.
    ///
    /// A credential is rejected at the public edge, before any MCP framing, so
    /// the typed status and the Wyrd problem body are only observable outside
    /// the protocol client — `rmcp`'s reqwest transport erases both.
    ///
    /// # Errors
    /// Returns request/header transport failures or a non-JSON response-body error.
    async fn raw_mcp_post(
        base_url: &str,
        credential: Option<&str>,
    ) -> Result<(reqwest::StatusCode, serde_json::Value), McpJourneyError> {
        let mut request = reqwest::Client::new()
            .post(format!("{base_url}/mcp"))
            .header("accept", "text/event-stream, application/json")
            .header("content-type", "application/json")
            .body(serde_json::json!({"jsonrpc": "2.0", "method": "server/discover"}).to_string());
        if let Some(credential) = credential {
            request = request.header("x-wyrd-access-token", credential);
        }
        let response = request.send().await?;
        let status = response.status();
        Ok((status, response.json().await?))
    }

    /// Build one cancellable `tools/call` for the probe, parked on `hold_id`.
    fn parked_call(hold_id: &str) -> ClientRequest {
        let mut arguments = serde_json::Map::new();
        arguments.insert(
            "hold_id".to_owned(),
            serde_json::Value::String(hold_id.to_owned()),
        );
        ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(probe::TOOL_NAME).with_arguments(arguments),
        ))
    }

    /// Credentials, authorization, and both cancellation paths on one server.
    ///
    /// These belong together because they are one lifecycle, not four
    /// behaviors: a request is refused at the edge before MCP framing exists,
    /// refused again by audited RBAC once it has an identity, bound to the
    /// identity the server verified rather than anything the client sent, and
    /// finally cancellable from either end — the client's own `notifications/
    /// cancelled` and the process shutdown that has to wait for the work it
    /// cancelled. Splitting them would stand up four servers to observe one
    /// chain.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn mcp_rejects_credentials_and_joins_request_and_process_cancellation()
    -> Result<(), McpJourneyError> {
        let mut server = WyrdTestServer::builder()
            .with_mcp_context_probe_for_test()
            .start_bound()
            .await?;
        let tenant = server.data_tenant_id();
        let base_url = server
            .base_url()
            .ok_or("journey requires a bound server")?
            .to_owned();

        // 1. The public edge refuses a request that carries no usable credential
        //    before `/mcp` ever sees it.
        let (status, problem) = raw_mcp_post(&base_url, None).await?;
        assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
        assert_eq!(problem["code"], "WYRD_AUTH_401_UNAUTHENTICATED");
        assert_eq!(problem["status"], 401);
        let (status, problem) = raw_mcp_post(&base_url, Some("Bearer not-a-jwt")).await?;
        assert_eq!(status, reqwest::StatusCode::BAD_REQUEST);
        assert_eq!(problem["code"], "WYRD_AUTH_400_BAD_TOKEN_FORMAT");
        assert_eq!(problem["status"], 400);

        // 2. An authenticated but under-scoped principal is denied by the same
        //    audited RBAC path every Bifrost read uses, and the denial is durable.
        let denied = server.bootstrap_service("mcp-denied", &["reader"]).await?;
        let denied_key = denied.api_key().ok_or("service bootstrap carries a key")?;
        let denied_request_id = RequestId::now_v7();
        let denied_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(denied_key.clone()),
                    Some(&denied_request_id),
                )?,
                discover(),
            )
            .await?;
        let refusal = denied_client
            .call_tool(CallToolRequestParams::new(probe::TOOL_NAME))
            .await
            .expect_err("an under-scoped principal cannot call the probe");
        assert!(
            refusal
                .to_string()
                .contains("WYRD_PERMISSION_403_DENIED_RBAC"),
            "the RBAC denial reaches the client verbatim: {refusal}"
        );
        let mut conn = server.tenant_conn_for(tenant).await?;
        let denial_rows: Vec<(String, uuid::Uuid, String, String)> = sqlx::query_as(
            "SELECT permission, principal_id, request_id, outcome \
             FROM vala.audit_staging \
             WHERE operation = $1 \
               AND request_id = $2",
        )
        .bind(probe::TOOL_NAME)
        .bind(denied_request_id.as_str())
        .fetch_all(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        assert_eq!(
            denial_rows,
            vec![(
                Permission::bifrost_query_read().to_string(),
                denied.id().as_uuid(),
                denied_request_id.to_string(),
                "denied".to_owned(),
            )],
            "the denial is durably audited exactly once, under the caller's own \
             request id, before anything is disclosed"
        );
        denied_client.cancel().await?;

        // 3. An allowed call reports the identity the server verified, not the
        //    identity the client asked for.
        let allowed = server.bootstrap_service("mcp-allowed", &["admin"]).await?;
        let allowed_key = allowed.api_key().ok_or("service bootstrap carries a key")?;
        let client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(allowed_key.clone()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let mut spoofed = serde_json::Map::new();
        spoofed.insert(
            "tenant_id".to_owned(),
            serde_json::Value::String(DataTenantId::new_v7().to_string()),
        );
        spoofed.insert(
            "principal_id".to_owned(),
            serde_json::Value::String(uuid::Uuid::now_v7().to_string()),
        );
        let result = client
            .call_tool(CallToolRequestParams::new(probe::TOOL_NAME).with_arguments(spoofed))
            .await?;
        let context = result
            .structured_content
            .ok_or("probe returns structured content")?;
        assert_eq!(
            context["tenant_id"],
            serde_json::Value::String(tenant.to_string()),
            "client-supplied identity arguments never displace the verified tenant"
        );
        assert_eq!(
            context["principal_id"],
            serde_json::Value::String(allowed.id().to_string())
        );

        // 4. A client-cancelled request cancels the handler's own context.
        let hold = format!("request-cancel-{}", uuid::Uuid::now_v7());
        let latches = probe::latches(&hold);
        let handle = client
            .send_cancellable_request(parked_call(&hold), PeerRequestOptions::no_options())
            .await?;
        latches.wait_entered().await;
        handle.cancel(None).await?;
        latches.wait_cancelled().await;
        latches.release_cleanup();
        latches.wait_cleanup_done().await;

        // 5. Process shutdown cancels in-flight MCP work and then waits for it,
        //    inside the one existing drain deadline.
        let hold = format!("process-cancel-{}", uuid::Uuid::now_v7());
        let latches = probe::latches(&hold);
        let _parked = client
            .send_cancellable_request(parked_call(&hold), PeerRequestOptions::no_options())
            .await?;
        latches.wait_entered().await;
        let mut drain = std::pin::pin!(server.cancel_and_join_for_test());
        assert!(
            tokio::time::timeout(Duration::from_millis(500), &mut drain)
                .await
                .is_err(),
            "the drain cannot report settled while a tracker token is still held"
        );
        latches.wait_cancelled().await;
        latches.release_cleanup();
        latches.wait_cleanup_done().await;
        drain.await?;
        Ok(())
    }
}
