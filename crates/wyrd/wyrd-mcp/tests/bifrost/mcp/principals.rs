//! Tier-1 journey: an agent observes and retires credentials over `/mcp`.
//!
//! Administrative capability over MCP is where a scope has to be more than a
//! label. The journey drives a real `rmcp` client twice — once with a read-only
//! credential and once with an administrative one — so what it observes about
//! the catalog and the refusal is exactly what an agent observes.

use crate::connectivity::{McpJourneyError, discover, problem, structured, transport};

mod pg_tests {
    use super::{McpJourneyError, discover, problem, structured, transport};

    use rmcp::ClientServiceExt as _;
    use rmcp::model::CallToolRequestParams;
    use secrecy::ExposeSecret as _;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::WyrdTestServer;

    /// The read tool every authenticated agent may use.
    const LIST_CREDENTIALS: &str = "principals.list_credentials";

    /// The write tool only an administrative agent may use.
    const REVOKE_CREDENTIAL: &str = "principals.revoke_credential";

    /// An administrative agent sees the write tool; a reader does not, and is
    /// refused when it names the tool anyway.
    ///
    /// The second half is the one that matters, and it is deliberately not a
    /// property of the catalog. Hiding a tool only shapes what an agent plans
    /// to do. What stops an under-scoped agent is the operation behind the
    /// tool, which authorizes the permission and records the denial — the same
    /// decision the HTTP surface makes, from the same code.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the Postgres-backed Bifrost journey lane"]
    async fn a_write_tool_is_scoped_at_dispatch_not_merely_hidden() -> Result<(), McpJourneyError> {
        let server = WyrdTestServer::start_bound().await?;

        let admin = server
            .bootstrap_service("mcp-principal-admin", &["admin"])
            .await?;
        let admin_key = admin
            .api_key()
            .ok_or("an admin service bootstraps with a machine key")?
            .expose_secret()
            .to_owned();
        let reader = server
            .bootstrap_service("mcp-principal-reader", &["reader"])
            .await?;
        let reader_key = reader
            .api_key()
            .ok_or("a reader service bootstraps with a machine key")?
            .expose_secret()
            .to_owned();

        // The administrative agent is offered both tools.
        let admin_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(admin_key.clone().into()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let admin_tools = admin_client.list_all_tools().await?;
        let admin_names: Vec<&str> = admin_tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert!(
            admin_names.contains(&LIST_CREDENTIALS),
            "an administrative agent is offered the read tool: {admin_names:?}"
        );
        assert!(
            admin_names.contains(&REVOKE_CREDENTIAL),
            "an administrative agent is offered the write tool: {admin_names:?}"
        );

        // The reader is offered only the read tool.
        let reader_client = ()
            .serve_with_lifecycle(
                transport(
                    &server,
                    ResolvedCredential::ApiKey(reader_key.clone().into()),
                    None,
                )?,
                discover(),
            )
            .await?;
        let reader_tools = reader_client.list_all_tools().await?;
        let reader_names: Vec<&str> = reader_tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert!(
            reader_names.contains(&LIST_CREDENTIALS),
            "read tools stay available to every agent: {reader_names:?}"
        );
        assert!(
            !reader_names.contains(&REVOKE_CREDENTIAL),
            "an under-scoped agent is not offered the write tool: {reader_names:?}"
        );

        // ...and naming it anyway is refused, which is the actual boundary.
        let refused = reader_client
            .call_tool(
                CallToolRequestParams::new(REVOKE_CREDENTIAL).with_arguments(
                    serde_json::json!({
                        "principal_id": uuid::Uuid::now_v7().to_string(),
                        "credential_id": uuid::Uuid::now_v7().to_string(),
                    })
                    .as_object()
                    .ok_or("arguments are an object")?
                    .clone(),
                ),
            )
            .await;

        match refused {
            Err(error) => {
                let rendered = error.to_string();
                assert!(
                    rendered.contains("service_accounts:write") || rendered.contains("permission"),
                    "the refusal names the missing permission: {rendered}"
                );
            }
            Ok(result) => {
                let problem = problem(result)?;
                assert!(
                    problem.to_string().contains("PERMISSION"),
                    "the refusal is the stable permission error: {problem}"
                );
            }
        }

        // The read tool genuinely works for the administrative agent.
        let listing = admin_client
            .call_tool(
                CallToolRequestParams::new(LIST_CREDENTIALS).with_arguments(
                    serde_json::json!({ "principal_id": admin.id().to_string() })
                        .as_object()
                        .ok_or("arguments are an object")?
                        .clone(),
                ),
            )
            .await?;
        let content = structured(listing)?;
        assert!(
            content.get("credentials").is_some(),
            "the listing projects credential metadata: {content}"
        );
        assert!(
            !content.to_string().contains(&admin_key),
            "a listing never carries secret material"
        );

        admin_client.cancel().await?;
        reader_client.cancel().await?;
        Ok(())
    }
}
