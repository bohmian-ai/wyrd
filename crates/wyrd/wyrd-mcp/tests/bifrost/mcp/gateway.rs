//! Tier-1 journey: an agent administers tenant gateway configuration over `/mcp`.
//!
//! An administrator configures a credential and deployment through the shared
//! Rust client over HTTP; an agent then reads the same redacted resources,
//! replaces and deletes configuration through the real MCP endpoint, and a
//! caller without gateway permission is denied with the canonical problem.

use crate::connectivity::{McpJourneyError, discover, problem, structured, transport};

mod pg_tests {
    use super::{McpJourneyError, discover, problem, structured, transport};

    use std::sync::Arc;

    use rmcp::ClientServiceExt as _;
    use rmcp::model::CallToolRequestParams;
    use secrecy::SecretString;
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::gateway_credential::CredentialWriter;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_client::transport::{HttpConfig, HttpTransport};
    use wyrd_client::{Gateway, WyrdClient};
    use wyrd_testing::WyrdTestServer;
    use wyrd_testing::server::TEST_GATEWAY_CREDENTIAL_BINDING;

    /// Builds the shared HTTP client for `server` authenticated by `jwt`.
    ///
    /// # Errors
    ///
    /// Returns an error when the server is unbound or the client cannot be built.
    fn http_client(server: &WyrdTestServer, jwt: &str) -> Result<WyrdClient, McpJourneyError> {
        let config = ClientConfig {
            http: HttpConfig {
                base_url: server.base_url().ok_or("server is bound")?.to_owned(),
                ..HttpConfig::default()
            },
            ..ClientConfig::default()
        };
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from(jwt.to_owned())),
        )?;
        let http = HttpTransport::new(&config.http, Arc::clone(&auth))?;
        Ok(WyrdClient::from_parts(auth, http, config.grpc))
    }

    /// Converts a JSON object literal into MCP tool arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is not a JSON object.
    fn object(
        value: serde_json::Value,
    ) -> Result<serde_json::Map<String, serde_json::Value>, McpJourneyError> {
        match value {
            serde_json::Value::Object(map) => Ok(map),
            other => Err(format!("tool arguments must be an object: {other}").into()),
        }
    }

    /// MCP gateway reads return the HTTP-configured redacted resources, MCP
    /// replacements and deletions are observed over HTTP with the same
    /// invalid-configuration and conflict problems, and a reader without
    /// gateway permission is denied reads and writes with a 403 problem.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed MCP journey lane"]
    async fn agent_administers_redacted_gateway_configuration() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;
        let admin = server
            .bootstrap_user("mcp-gateway-admin", &["admin"])
            .await?;
        let admin_jwt = admin
            .jwt()
            .ok_or("admin bootstrap issues a jwt")?
            .to_owned();
        let gateway = Gateway::new(http_client(&server, &admin_jwt)?);
        let credential_admin = CredentialWriter::new(http_client(&server, &admin_jwt)?);
        credential_admin
            .put_credential(&serde_json::from_value(serde_json::json!({
                "name": "primary",
                "provider": "openai",
                "source": {"environment": {"binding": TEST_GATEWAY_CREDENTIAL_BINDING}},
            }))?)
            .await?;
        gateway
            .put_deployment(&serde_json::from_value(serde_json::json!({
                "name": "primary",
                "model": {"provider": "openai", "model": "gpt-4o"},
                "adapter": "openai",
                "auth": {"bearer": {"credential": "primary"}},
                "capabilities": ["chat_completions"],
                "routing_weight": 1,
            }))?)
            .await?;

        let client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(admin_jwt.clone().into()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let credentials = structured(
            client
                .call_tool(CallToolRequestParams::new(
                    "gateway.list_provider_credentials",
                ))
                .await?,
        )?;
        assert_eq!(
            credentials["items"],
            serde_json::to_value(gateway.credentials().await?)?,
            "MCP projects the same redacted credential views as HTTP"
        );
        assert_eq!(credentials["items"][0]["state"], "active");
        let deployments = structured(
            client
                .call_tool(CallToolRequestParams::new(
                    "gateway.list_provider_deployments",
                ))
                .await?,
        )?;
        assert_eq!(deployments["items"][0]["name"], "primary");
        let capture = structured(
            client
                .call_tool(CallToolRequestParams::new("gateway.get_capture_policy"))
                .await?,
        )?;
        assert_eq!(
            capture,
            serde_json::to_value(gateway.capture_policy().await?)?
        );
        let credential = structured(
            client
                .call_tool(
                    CallToolRequestParams::new("gateway.get_provider_credential")
                        .with_arguments(object(serde_json::json!({"name": "primary"}))?),
                )
                .await?,
        )?;
        assert_eq!(credential, credentials["items"][0]);

        // The scoped write tool is the managed-secret submission path: the
        // same tool, a write-only source, and a unit view in every answer.
        const SUBMITTED: &str = "sk-live-mcp-submission";
        const ROTATED: &str = "sk-live-mcp-rotation";
        let submit = |secret: &'static str| {
            let client = &client;
            async move {
                let answer = client
                    .call_tool(
                        CallToolRequestParams::new("gateway.put_provider_credential")
                            .with_arguments(object(serde_json::json!({
                                "name": "managed",
                                "provider": "openai",
                                "source": {"managed_secret": {"secret": secret}},
                            }))?),
                    )
                    .await?;
                assert!(
                    !serde_json::to_string(&answer)?.contains(secret),
                    "no part of the tool answer repeats the submitted value"
                );
                structured(answer)
            }
        };
        let name = wyrd_spec::ids::ProviderCredentialName::new("managed")
            .map_err(|error| error.to_string())?;
        let managed = submit(SUBMITTED).await?;
        assert_eq!(managed["source"], "managed_secret");
        assert_eq!(managed["state"], "active");
        let rotated = submit(ROTATED).await?;
        assert_eq!(rotated["created_at"], managed["created_at"]);
        assert_ne!(rotated["rotated_at"], managed["rotated_at"]);
        assert_eq!(
            serde_json::to_value(gateway.credential(&name).await?)?,
            rotated,
            "HTTP reads the same redacted managed view"
        );
        credential_admin.delete_credential(&name).await?;

        let written = structured(
            client
                .call_tool(
                    CallToolRequestParams::new("gateway.put_capture_policy").with_arguments(
                        object(serde_json::json!({"mode": "metadata", "payload_fields": []}))?,
                    ),
                )
                .await?,
        )?;
        assert_eq!(written["mode"], "metadata");
        assert_eq!(
            written,
            serde_json::to_value(gateway.capture_policy().await?)?,
            "an MCP replacement is visible over HTTP"
        );
        let invalid = problem(
            client
                .call_tool(
                    CallToolRequestParams::new("gateway.put_capture_policy")
                        .with_arguments(object(serde_json::json!({"mode": "everything"}))?),
                )
                .await?,
        )?;
        assert_eq!(
            invalid["code"], "WYRD_GATEWAY_400_INVALID_CONFIGURATION",
            "{invalid}"
        );

        let conflict = problem(
            client
                .call_tool(
                    CallToolRequestParams::new("gateway.delete_provider_credential")
                        .with_arguments(object(serde_json::json!({"name": "primary"}))?),
                )
                .await?,
        )?;
        assert_eq!(conflict["status"], 409, "{conflict}");
        for tool in [
            "gateway.delete_provider_deployment",
            "gateway.delete_provider_credential",
        ] {
            let deleted = structured(
                client
                    .call_tool(
                        CallToolRequestParams::new(tool)
                            .with_arguments(object(serde_json::json!({"name": "primary"}))?),
                    )
                    .await?,
            )?;
            assert_eq!(deleted, serde_json::json!({}), "{tool}");
        }
        assert!(gateway.credentials().await?.is_empty());
        client.cancel().await?;

        let reader = server
            .bootstrap_user("mcp-gateway-reader", &["reader"])
            .await?;
        let reader_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::BearerToken(
                        reader
                            .jwt()
                            .ok_or("reader bootstrap issues a jwt")?
                            .to_owned()
                            .into(),
                    ),
                    None,
                )?,
                discover(),
            )
            .await?;
        let denied = problem(
            reader_client
                .call_tool(CallToolRequestParams::new(
                    "gateway.list_provider_credentials",
                ))
                .await?,
        )?;
        assert_eq!(denied["status"], 403, "{denied}");
        let denied_write = problem(
            reader_client
                .call_tool(CallToolRequestParams::new("gateway.delete_fallback_policy"))
                .await?,
        )?;
        assert_eq!(denied_write["status"], 403, "{denied_write}");
        reader_client.cancel().await?;
        server.shutdown().await?;
        Ok(())
    }
}
