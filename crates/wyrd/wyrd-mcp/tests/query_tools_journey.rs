//! Real-server MCP Card lifecycle journeys.

mod pg_tests {
    use reqwest::Method;
    use serde_json::{Value, json};
    use skald_tool::ToolRegistry;
    use url::Url;
    use wyrd_client::{WyrdClient, config::ClientConfig};
    use wyrd_mcp::cards::register_card_tools;
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::storage::{SinglePutComplete, UploadCompleteRequest};
    use wyrd_testing::WyrdTestServer;

    fn client_from_bootstrap(base_url: &str, api_key: secrecy::SecretString) -> WyrdClient {
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.to_owned();
        config.api_key = Some(api_key);
        WyrdClient::with_config(config).expect("test client assembles")
    }

    fn registry(client: WyrdClient) -> ToolRegistry {
        let registry = ToolRegistry::new();
        register_card_tools(&registry, client).expect("Card tools register");
        registry
    }

    async fn invoke(registry: &ToolRegistry, name: &str, args: Value) -> Value {
        registry
            .resolve(name)
            .expect("tool resolves")
            .invoke(args)
            .await
            .unwrap_or_else(|error| panic!("{name} failed: {error}"))
    }

    fn registration_args(name: &str, idempotency_key: &str, artifact: bool) -> Value {
        let mut submission = json!({
            "apiVersion": "wyrd/v1",
            "kind": "Mcp",
            "metadata": {
                "name": name,
                "version": "1.0.0",
                "space": "default"
            },
            "spec": {
                "server_name": "mcp-journey",
                "transport": "http",
                "scopes": []
            }
        });
        if artifact {
            submission["artifacts"] = json!([{
                "relative_path": "prompt.txt",
                "sha256": "ypeBEsobvcr6wjGzmiPcTaeG7/gUfE5yuYB3ha/uSLs=",
                "size_bytes": 1,
                "content_type": "text/plain"
            }]);
        }
        json!({
            "request": {"submissions": [submission]},
            "idempotency_key": idempotency_key
        })
    }

    fn card_ref(value: &Value) -> CardRef {
        serde_json::from_value(value["root"].clone()).expect("registration returns CardRef")
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "real Postgres and socket MCP journey"]
    async fn mcp_card_lifecycle_discover_register_finalize_read_load_delete() {
        let server = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = server.base_url().expect("server has base URL").to_owned();
        let bootstrap = server
            .bootstrap_agent("mcp-lifecycle", &["writer"])
            .await
            .expect("writer bootstraps");
        let client = client_from_bootstrap(
            &base_url,
            bootstrap.api_key().expect("machine API key").clone(),
        );
        let upload_client = client.clone();
        let registry = registry(client);

        assert_eq!(
            registry.names(),
            vec![
                "cards.delete",
                "cards.finalize",
                "cards.get",
                "cards.latest",
                "cards.list",
                "cards.load",
                "cards.register"
            ]
        );

        let registered = invoke(
            &registry,
            "cards.register",
            registration_args("mcp-lifecycle-card", "mcp-lifecycle-001", true),
        )
        .await;
        let root = card_ref(&registered);
        let card_uid = root.uid.clone().expect("registration returns Card UID");
        let upload_url = registered["upload_plans"][0]["entries"][0]["plan"]["data"]["put_url"]
            .as_str()
            .expect("local registration returns upload URL");
        let upload_id = registered["upload_plans"][0]["entries"][0]["upload_id"]
            .as_str()
            .expect("registration returns upload ID");
        let upload_path = Url::parse(upload_url)
            .map_or_else(|_| upload_url.to_owned(), |url| url.path().to_owned());
        let upload = upload_client
            .request_stream(Method::PUT, &upload_path, reqwest::Body::from("a"))
            .await
            .expect("artifact upload succeeds");
        assert!(upload.status().is_success());
        upload_client
            .request_json::<_, Value>(
                Method::POST,
                &format!("/v1/cards/upload/{upload_id}/complete"),
                Some(&UploadCompleteRequest::SinglePut(SinglePutComplete {})),
            )
            .await
            .expect("internal upload completion succeeds");

        let finalized = invoke(
            &registry,
            "cards.finalize",
            json!({
                "card_uid": card_uid,
                "idempotency_key": "mcp-lifecycle-001"
            }),
        )
        .await;
        assert_eq!(finalized["outcomes"][0]["status"], "active");

        let by_ref = invoke(&registry, "cards.get", json!({"ref": root})).await;
        assert_eq!(by_ref["card"]["metadata"]["name"], "mcp-lifecycle-card");

        let listed = invoke(&registry, "cards.list", json!({})).await;
        assert!(
            listed["items"]
                .as_array()
                .expect("list items")
                .iter()
                .any(|item| item["name"] == "mcp-lifecycle-card")
        );

        let latest = invoke(
            &registry,
            "cards.latest",
            json!({"kind": "Mcp", "space": "default", "name": "mcp-lifecycle-card"}),
        )
        .await;
        assert_eq!(latest["card"]["metadata"]["name"], "mcp-lifecycle-card");

        let load = invoke(
            &registry,
            "cards.load",
            json!({"card_uid": root.uid, "relative_path": "prompt.txt"}),
        )
        .await;
        let download_url = load["plan"]["get_url"]
            .as_str()
            .expect("load returns download URL");
        let download_path = Url::parse(download_url)
            .map_or_else(|_| download_url.to_owned(), |url| url.path().to_owned());
        let downloaded = upload_client
            .request_raw(Method::GET, &download_path)
            .await
            .expect("download plan is readable");
        assert_eq!(downloaded.bytes().await.expect("download body reads"), "a");

        let deleted = invoke(
            &registry,
            "cards.delete",
            json!({"uid": {"kind": "Mcp", "uid": root.uid}}),
        )
        .await;
        assert_eq!(deleted["deleted"], true);

        server.shutdown().await.expect("test server shuts down");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "real Postgres and socket MCP journey"]
    async fn mcp_card_negative_permissions_tenancy_and_exact_refs() {
        let server = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = server.base_url().expect("server has base URL").to_owned();

        let denied = server
            .bootstrap_agent("mcp-denied", &[])
            .await
            .expect("unprivileged agent bootstraps");
        let denied_registry = registry(client_from_bootstrap(
            &base_url,
            denied.api_key().expect("machine API key").clone(),
        ));
        let error = denied_registry
            .resolve("cards.register")
            .expect("register resolves")
            .invoke(registration_args(
                "mcp-denied-card",
                "mcp-denied-001",
                false,
            ))
            .await
            .expect_err("under-privileged registration is rejected");
        assert!(
            error
                .to_string()
                .contains("WYRD_PERMISSION_403_DENIED_RBAC")
        );

        let owner = server
            .bootstrap_agent("mcp-owner", &["writer"])
            .await
            .expect("owner bootstraps");
        let owner_registry = registry(client_from_bootstrap(
            &base_url,
            owner.api_key().expect("machine API key").clone(),
        ));
        let registered = invoke(
            &owner_registry,
            "cards.register",
            registration_args("mcp-tenant-card", "mcp-tenant-001", false),
        )
        .await;
        let root = card_ref(&registered);

        let tenant_b = server
            .seed_tenant("mcp-tenant-b")
            .await
            .expect("tenant seeds");
        let other = server
            .bootstrap_service_in_tenant(tenant_b, "mcp-other-tenant", &["reader"])
            .await
            .expect("second tenant reader bootstraps");
        let other_registry = registry(client_from_bootstrap(
            &base_url,
            other.api_key().expect("machine API key").clone(),
        ));
        let cross_tenant = other_registry
            .resolve("cards.get")
            .expect("get resolves")
            .invoke(json!({"ref": root}))
            .await
            .expect_err("cross-tenant lookup is hidden");
        assert!(
            cross_tenant
                .to_string()
                .contains("WYRD_REGISTRY_404_CARD_NOT_FOUND")
        );

        let invalid = other_registry
            .resolve("cards.get")
            .expect("get resolves")
            .invoke(json!({
                "ref": {"kind": "Mcp", "name": "bad-ref", "version": "not-semver"}
            }))
            .await
            .expect_err("invalid exact reference is rejected");
        assert_eq!(invalid.code(), "SKALD_TOOL_422_INPUT");

        server.shutdown().await.expect("test server shuts down");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "real Postgres and socket MCP journey"]
    async fn mcp_card_finalize_reports_storage_failure() {
        let server = WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = server.base_url().expect("server has base URL").to_owned();
        let bootstrap = server
            .bootstrap_agent("mcp-storage-failure", &["writer"])
            .await
            .expect("writer bootstraps");
        let registry = registry(client_from_bootstrap(
            &base_url,
            bootstrap.api_key().expect("machine API key").clone(),
        ));
        let registered = invoke(
            &registry,
            "cards.register",
            registration_args("mcp-storage-card", "mcp-storage-001", true),
        )
        .await;
        let card_uid = card_ref(&registered)
            .uid
            .expect("registration returns Card UID");
        let error = registry
            .resolve("cards.finalize")
            .expect("finalize resolves")
            .invoke(json!({
                "card_uid": card_uid,
                "idempotency_key": "mcp-storage-001"
            }))
            .await
            .expect_err("missing storage object prevents finalization");
        assert!(
            error
                .to_string()
                .contains("WYRD_STORAGE_404_OBJECT_NOT_FOUND")
        );

        server.shutdown().await.expect("test server shuts down");
    }
}
