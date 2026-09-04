//! Tier-1 journey: a real MCP client reaching Wyrd's `/mcp` endpoint.
//!
//! This binary proves the connectivity contract end to end — the first-party
//! Rust client transport, the public listener's authentication and correlation
//! edge, the `rmcp` Streamable HTTP service mounted inside it, and the request
//! and process cancellation that shutdown depends on. No Bifrost tool exists
//! yet; the server's test-support context probe stands in for one so the
//! journey observes exactly what the edge bound.

use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use secrecy::SecretString;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::config::{ClientConfig, TokenCacheMode};
use wyrd_client::transport::credential::ResolvedCredential;
use wyrd_mcp::client::WyrdMcpHttpClient;
use wyrd_testing::WyrdTestServer;

/// Boxed error carried by every helper in this journey.
type McpJourneyError = Box<dyn std::error::Error + Send + Sync>;

/// Build the first-party MCP client transport for `server` using `api_key`.
///
/// The credential path is the production one: the middleware exchanges the key
/// at the server's real `/auth/token` route and the decorator attaches the
/// resulting bearer to every MCP request.
fn transport(
    server: &WyrdTestServer,
    api_key: &SecretString,
) -> Result<StreamableHttpClientTransport<WyrdMcpHttpClient>, McpJourneyError> {
    let base_url = server
        .base_url()
        .ok_or("journey requires a bound test server")?
        .to_owned();
    let mut config = ClientConfig::default();
    config.http.base_url = base_url.clone();
    config.token_cache = TokenCacheMode::InMemory;
    let auth = AuthMiddleware::new(&config, ResolvedCredential::ApiKey(api_key.clone()))?;
    Ok(StreamableHttpClientTransport::with_client(
        WyrdMcpHttpClient::new(reqwest::Client::new(), auth),
        StreamableHttpClientTransportConfig::with_uri(format!("{base_url}/mcp")),
    ))
}

mod pg_tests {
    use super::{McpJourneyError, transport};

    use std::time::Duration;

    use rmcp::model::{CallToolRequest, CallToolRequestParams, ClientRequest, ProtocolVersion};
    use rmcp::service::PeerRequestOptions;
    use rmcp::{ClientLifecycleMode, ClientServiceExt as _};
    use wyrd_server::mcp::probe;
    use wyrd_spec::DataTenantId;
    use wyrd_testing::WyrdTestServer;

    /// The modern, session-free MCP lifecycle: `server/discover` plus
    /// self-contained per-request protocol metadata.
    ///
    /// Wyrd serves exactly one protocol revision and mounts no session
    /// manager, so the legacy `initialize` handshake has nothing to negotiate
    /// down to; this is the only lifecycle a client can use against `/mcp`.
    fn discover() -> ClientLifecycleMode {
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        }
    }

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
        let caller = server
            .bootstrap_service("mcp-connectivity", &["admin"])
            .await?;
        let api_key = caller.api_key().ok_or("service bootstrap carries a key")?;
        let client = ().serve_with_lifecycle(transport(&server, api_key)?, discover()).await?;

        let tools = client.list_all_tools().await?;
        let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![probe::TOOL_NAME],
            "an opted-in fixture advertises exactly the context probe"
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

    /// A fixture that did not opt in advertises no MCP tool at all.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn default_server_advertises_no_mcp_tools() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let caller = server.bootstrap_service("mcp-default", &["admin"]).await?;
        let api_key = caller.api_key().ok_or("service bootstrap carries a key")?;
        let client = ().serve_with_lifecycle(transport(&server, api_key)?, discover()).await?;

        assert!(
            client.list_all_tools().await?.is_empty(),
            "the production catalog is empty until the Bifrost tools land"
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

    /// Count durable denial rows the probe's audited authorization wrote.
    async fn probe_denials(
        server: &WyrdTestServer,
        tenant: DataTenantId,
    ) -> Result<i64, McpJourneyError> {
        let mut conn = server.tenant_conn_for(tenant).await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM vala.audit_outbox WHERE operation = $1 AND decision = 'deny'",
        )
        .bind(probe::TOOL_NAME)
        .fetch_one(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(count)
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
        let denied_client =
            ().serve_with_lifecycle(transport(&server, denied_key)?, discover())
                .await?;
        let before = probe_denials(&server, tenant).await?;
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
        assert_eq!(
            probe_denials(&server, tenant).await?,
            before + 1,
            "the denial is durably audited before anything is disclosed"
        );
        denied_client.cancel().await?;

        // 3. An allowed call reports the identity the server verified, not the
        //    identity the client asked for.
        let allowed = server.bootstrap_service("mcp-allowed", &["admin"]).await?;
        let allowed_key = allowed.api_key().ok_or("service bootstrap carries a key")?;
        let client = ().serve_with_lifecycle(transport(&server, allowed_key)?, discover()).await?;
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
