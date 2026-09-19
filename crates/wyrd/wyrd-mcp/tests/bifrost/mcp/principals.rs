//! Tier-1 journey: an agent observes and retires credentials over `/mcp`.
//!
//! Administrative capability over MCP is where a scope has to be more than a
//! label. The journey drives a real `rmcp` client twice — once with a read-only
//! credential and once with an administrative one — so what it observes about
//! the catalog and the refusal is exactly what an agent observes.

use crate::connectivity::{McpJourneyError, discover, principals, problem, structured, transport};

mod pg_tests {
    use super::{McpJourneyError, discover, principals, problem, structured, transport};

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

        reader_client.cancel().await?;

        // Discover, act, observe: the authorized agent retires a credential and
        // sees the retirement, which is the only proof that the write tool is a
        // capability rather than a catalog entry.
        //
        // The reader is given a replacement first, so the credential being
        // retired is a real non-current one and the assertion afterwards is
        // about that credential rather than about the principal losing its last
        // way in.
        principals(
            &server,
            ResolvedCredential::ApiKey(admin_key.clone().into()),
        )?
        .issue_credential(&reader.id().to_string().parse()?)
        .await?;

        let before = structured(
            admin_client
                .call_tool(
                    CallToolRequestParams::new(LIST_CREDENTIALS).with_arguments(
                        serde_json::json!({ "principal_id": reader.id().to_string() })
                            .as_object()
                            .ok_or("arguments are an object")?
                            .clone(),
                    ),
                )
                .await?,
        )?;
        let listed = before
            .get("credentials")
            .and_then(serde_json::Value::as_array)
            .ok_or("the listing carries credential metadata")?
            .clone();
        assert_eq!(listed.len(), 2, "both credentials are live: {before}");
        assert!(
            listed.iter().all(|entry| entry
                .get("revoked_at")
                .is_none_or(serde_json::Value::is_null)),
            "nothing is retired yet: {before}"
        );
        // The oldest is the superseded one, which is what a rotation retires.
        let doomed = listed
            .last()
            .and_then(|entry| entry.get("id"))
            .and_then(serde_json::Value::as_str)
            .ok_or("a listed credential carries an id")?
            .to_owned();

        let revoked = admin_client
            .call_tool(
                CallToolRequestParams::new(REVOKE_CREDENTIAL).with_arguments(
                    serde_json::json!({
                        "principal_id": reader.id().to_string(),
                        "credential_id": doomed,
                    })
                    .as_object()
                    .ok_or("arguments are an object")?
                    .clone(),
                ),
            )
            .await?;
        assert!(
            revoked.is_error != Some(true),
            "the authorized revocation succeeds: {revoked:?}"
        );

        // Observed, not assumed: the agent reads the retirement back through
        // the same read tool it used before acting.
        let after = structured(
            admin_client
                .call_tool(
                    CallToolRequestParams::new(LIST_CREDENTIALS).with_arguments(
                        serde_json::json!({ "principal_id": reader.id().to_string() })
                            .as_object()
                            .ok_or("arguments are an object")?
                            .clone(),
                    ),
                )
                .await?,
        )?;
        let observed = after
            .get("credentials")
            .and_then(serde_json::Value::as_array)
            .ok_or("the listing carries credential metadata")?;
        for entry in observed {
            let retired = entry
                .get("revoked_at")
                .is_some_and(|value| !value.is_null());
            let is_doomed = entry.get("id").and_then(serde_json::Value::as_str) == Some(&doomed);
            assert_eq!(
                retired, is_doomed,
                "exactly the revoked credential is retired: {after}"
            );
        }

        // Replaying it finds nothing left to retire, which is how an agent
        // learns the first call was the one that took effect.
        let replay = admin_client
            .call_tool(
                CallToolRequestParams::new(REVOKE_CREDENTIAL).with_arguments(
                    serde_json::json!({
                        "principal_id": reader.id().to_string(),
                        "credential_id": doomed,
                    })
                    .as_object()
                    .ok_or("arguments are an object")?
                    .clone(),
                ),
            )
            .await;
        // A refusal reaches an agent either as a protocol error or as an error
        // result, depending on how the transport frames it; both carry the same
        // stable Wyrd code, which is what the agent actually reads.
        let rendered = match replay {
            Err(error) => error.to_string(),
            Ok(result) => problem(result)?.to_string(),
        };
        assert!(
            rendered.contains("NOT_FOUND"),
            "a replayed revocation reports the credential is already gone: {rendered}"
        );

        admin_client.cancel().await?;
        Ok(())
    }
}
