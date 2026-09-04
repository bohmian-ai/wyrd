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

    use rmcp::model::{CallToolRequestParams, ProtocolVersion};
    use rmcp::{ClientLifecycleMode, ClientServiceExt as _};
    use wyrd_server::mcp::probe;
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
}
