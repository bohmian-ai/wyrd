//! Tier-1 journey: an agent observes and retires credentials over `/mcp`.
//!
//! Administrative capability over MCP is where a scope has to be more than a
//! label. The journey drives a real `rmcp` client twice — once with a read-only
//! credential and once with an administrative one — so what it observes about
//! the catalog and the refusal is exactly what an agent observes.

use crate::connectivity::{McpJourneyError, discover, principals, problem, structured, transport};

/// The Postgres-backed half of the credential journey.
mod pg_tests {
    use super::{McpJourneyError, discover, principals, problem, structured, transport};

    use jsonschema::JSONSchema;
    use rmcp::ClientServiceExt as _;
    use rmcp::model::CallToolRequestParams;
    use secrecy::ExposeSecret as _;
    use serde_json::Map as JsonMap;
    use serde_json::Value as JsonValue;
    use wyrd_client::transport::credential::ResolvedCredential;
    use wyrd_testing::WyrdTestServer;

    /// The read tool every authenticated agent may use.
    const LIST_CREDENTIALS: &str = "principals.list_credentials";

    /// The write tool only an administrative agent may use.
    const REVOKE_CREDENTIAL: &str = "principals.revoke_credential";

    /// The property names one advertised schema declares required.
    ///
    /// Reading the catalog's own schema rather than restating it is the point:
    /// the assertion fails if the published contract stops matching the shared
    /// DTO the server parses and returns.
    fn required(schema: &JsonMap<String, JsonValue>) -> Vec<&str> {
        schema
            .get("required")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Compile one advertised schema the way a strict agent-side validator
    /// would, honoring the `uuid` format the identifier fields declare.
    ///
    /// # Errors
    ///
    /// Returns an error when the advertised schema does not compile.
    fn validator(schema: &JsonMap<String, JsonValue>) -> Result<JSONSchema, McpJourneyError> {
        JSONSchema::options()
            .should_validate_formats(true)
            .with_format("uuid", |value| uuid::Uuid::parse_str(value).is_ok())
            .compile(&JsonValue::Object(schema.clone()))
            .map_err(|error| format!("an advertised schema compiles: {error}").into())
    }

    /// An administrative agent sees the write tool; a reader does not, and is
    /// refused when it names the tool anyway.
    ///
    /// The second half is the one that matters, and it is deliberately not a
    /// property of the catalog. Hiding a tool only shapes what an agent plans
    /// to do. What stops an under-scoped agent is the operation behind the
    /// tool, which authorizes the permission and records the denial — the same
    /// decision the HTTP surface makes, from the same code.
    ///
    /// # Errors
    ///
    /// Returns server startup, bootstrap, credential-issuance, MCP transport,
    /// or tool-call failures.
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

        // Both tools publish the shape they speak, in and out, so an agent can
        // plan a call and read a result without guessing at either end.
        let descriptor = |name: &str| {
            admin_tools
                .iter()
                .find(|tool| tool.name.as_ref() == name)
                .cloned()
                .ok_or("an advertised tool carries a descriptor")
        };
        let list_tool = descriptor(LIST_CREDENTIALS)?;
        assert_eq!(
            required(&list_tool.input_schema),
            vec!["principal_id"],
            "the read tool advertises its input shape: {:?}",
            list_tool.input_schema
        );
        let list_output = list_tool
            .output_schema
            .clone()
            .ok_or("the read tool publishes an output schema")?;
        assert_eq!(
            required(&list_output),
            vec!["credentials"],
            "the read tool advertises its result shape: {list_output:?}"
        );
        let revoke_tool = descriptor(REVOKE_CREDENTIAL)?;
        // An agent planning a revocation reads the same auth contract the
        // server enforces: the next issuance is refused, and a token already
        // minted is a snapshot that lapses within five minutes.
        let revoke_description = revoke_tool.description.as_deref().unwrap_or_default();
        assert!(
            revoke_description.contains("lapses within five minutes")
                && !revoke_description.contains("stop authorizing"),
            "the write tool describes the five-minute snapshot: {revoke_description}"
        );
        assert_eq!(
            required(&revoke_tool.input_schema),
            vec!["credential_id", "principal_id"],
            "the write tool advertises its input shape: {:?}",
            revoke_tool.input_schema
        );
        let revoke_output = revoke_tool
            .output_schema
            .clone()
            .ok_or("the write tool publishes an output schema")?;
        assert_eq!(
            required(&revoke_output),
            vec!["credential_id", "revoked"],
            "the write tool advertises its result shape: {revoke_output:?}"
        );

        // The identifiers are typed in the catalog, not only at dispatch: an
        // argument the advertised schema accepts is one the server can act on,
        // and a malformed one is rejected before any call is made.
        let list_input = validator(&list_tool.input_schema)?;
        let revoke_input = validator(&revoke_tool.input_schema)?;
        let principal = uuid::Uuid::now_v7().to_string();
        let credential = uuid::Uuid::now_v7().to_string();
        assert!(list_input.is_valid(&serde_json::json!({ "principal_id": principal })));
        assert!(!list_input.is_valid(&serde_json::json!({ "principal_id": "not-a-uuid" })));
        assert!(revoke_input.is_valid(&serde_json::json!({
            "principal_id": principal,
            "credential_id": credential,
        })));
        for malformed in [
            serde_json::json!({ "principal_id": "not-a-uuid", "credential_id": credential }),
            serde_json::json!({ "principal_id": principal, "credential_id": "not-a-uuid" }),
        ] {
            assert!(
                !revoke_input.is_valid(&malformed),
                "the advertised revocation schema rejects {malformed}"
            );
        }
        let list_result = validator(&list_output)?;
        let revoke_result = validator(&revoke_output)?;
        // Listed credential ids carry the same UUID contract revocation takes.
        let listed = |id: &str| {
            serde_json::json!({ "credentials": [{
                "id": id,
                "prefix": "wyrd_sk_",
                "created_at": "2026-01-01T00:00:00Z",
                "expires_at": null,
                "revoked_at": null,
                "last_used_at": null,
            }] })
        };
        assert!(list_result.is_valid(&listed(&credential)));
        assert!(
            !list_result.is_valid(&listed("not-a-uuid")),
            "the advertised listing schema types credential ids as UUIDs"
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
        for property in required(&list_output) {
            assert!(
                content.get(property).is_some(),
                "a real listing carries every advertised property: {content}"
            );
        }
        assert!(
            list_result.is_valid(&content),
            "a real listing satisfies the advertised output schema: {content}"
        );
        assert!(
            !content.to_string().contains(&admin_key),
            "a listing never carries secret material"
        );

        // Listing an unknown principal is the stable, non-enumerating refusal,
        // and the permission it evaluated is still recorded.
        let superuser = server.pg_fixture().superuser_pool().await?;
        let listed_decisions = || async {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM vala.audit_staging
                  WHERE operation = 'auth.credential.list' AND outcome = 'allowed'",
            )
            .fetch_one(&superuser)
            .await
        };
        let before = listed_decisions().await?;
        let unknown = admin_client
            .call_tool(
                CallToolRequestParams::new(LIST_CREDENTIALS).with_arguments(
                    serde_json::json!({ "principal_id": uuid::Uuid::now_v7().to_string() })
                        .as_object()
                        .ok_or("arguments are an object")?
                        .clone(),
                ),
            )
            .await;
        let rendered = match unknown {
            Err(error) => error.to_string(),
            Ok(result) => problem(result)?.to_string(),
        };
        assert!(
            rendered.contains("NOT_FOUND"),
            "an unknown principal is refused as not found: {rendered}"
        );
        assert_eq!(
            listed_decisions().await?,
            before + 1,
            "the refused listing records exactly one allowed decision"
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
        let acknowledgement = structured(revoked)?;
        assert!(
            revoke_result.is_valid(&acknowledgement),
            "a real acknowledgement satisfies the advertised output schema: {acknowledgement}"
        );
        for property in required(&revoke_output) {
            assert!(
                acknowledgement.get(property).is_some(),
                "a real acknowledgement carries every advertised property: {acknowledgement}"
            );
        }
        assert_eq!(
            acknowledgement
                .get("credential_id")
                .and_then(serde_json::Value::as_str),
            Some(doomed.as_str()),
            "the acknowledgement names the credential that was retired: {acknowledgement}"
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
