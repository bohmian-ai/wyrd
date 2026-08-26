//! Proves the MCP describe surface projects the canonical physical layout
//! byte-identically to the HTTP describe route.
//!
//! MCP is a first-class agent surface, not a convenience projection: an agent
//! that plans a scan from `bifrost.describe_table` must see exactly the
//! partition granularity, sort order, and Bloom set the HTTP contract states.
//! The journey registers one caller-declared layout over the real HTTP route on
//! a real server, then reads the same table back through both surfaces.

// Wrapped in `mod pg_tests`: the journey boots a `start_bound` server backed by
// the embedded Postgres fixture, so the fast family lane skips it by module
// name while the gated real-server lane runs it.
mod pg_tests {
    use std::sync::Arc;

    use reqwest::Method;
    use serde_json::json;
    use skald_tool::ToolRegistry;
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::HttpConfig;
    use wyrd_mcp::bifrost::register_bifrost_tools;
    use wyrd_spec::vala::api::{
        BifrostTableDescription, DataTypeSpec, FieldSpec, NullOrderWire, PhysicalLayoutWire,
        RegisterTableRequest, RegisterTableResponse, SortDirectionWire, SortKeyWire,
        TimeGranularityWire,
    };
    use wyrd_testing::WyrdTestServer;

    /// The caller-owned namespace the HTTP register route admits.
    const NAMESPACE: &str = "vala.datasets";

    /// Build one non-null user field descriptor for the fixture schema.
    fn field(name: &str, data_type: DataTypeSpec) -> FieldSpec {
        FieldSpec {
            name: name.to_owned(),
            data_type,
            nullable: false,
            metadata: std::collections::BTreeMap::new(),
        }
    }

    /// One deliberately non-default declaration: a managed event-time key the
    /// old contract reserved, a user key behind it, and one user Bloom column.
    fn declared_layout() -> PhysicalLayoutWire {
        PhysicalLayoutWire {
            partition_granularity: TimeGranularityWire::Hour,
            sort_keys: vec![
                SortKeyWire {
                    column: wyrd_spec::vala::WYRD_EVENT_TIME.to_owned(),
                    direction: SortDirectionWire::Desc,
                    null_order: NullOrderWire::Last,
                },
                SortKeyWire {
                    column: "id".to_owned(),
                    direction: SortDirectionWire::Asc,
                    null_order: NullOrderWire::Last,
                },
            ],
            bloom_columns: vec!["value".to_owned()],
        }
    }

    /// MCP describe returns the identical canonical layout HTTP describe returns.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires the gated Bifrost real-server journey lane"]
    async fn bifrost_layout_matches_http_description() {
        let server = WyrdTestServer::start_bound()
            .await
            .expect("bound Bifrost test server");
        let base_url = server
            .base_url()
            .expect("bound server has an HTTP base url")
            .to_owned();
        let bootstrap = server
            .bootstrap_service("mcp-layout-describe", &["admin"])
            .await
            .expect("service principal bootstraps");
        let client = Arc::new(
            WyrdClient::with_config(ClientConfig {
                http: HttpConfig {
                    base_url,
                    ..HttpConfig::default()
                },
                api_key: Some(bootstrap.api_key().expect("machine API key").clone()),
                ..ClientConfig::default()
            })
            .expect("public HTTP client assembles"),
        );

        let name = format!(
            "mcp_layout_{}",
            bootstrap.id().to_string().replace('-', "_")
        );
        let request = RegisterTableRequest {
            namespace: NAMESPACE.to_owned(),
            name: name.clone(),
            fields: vec![
                field("id", DataTypeSpec::Int64),
                field("value", DataTypeSpec::Utf8),
            ],
            physical_layout: Some(declared_layout()),
        };
        let _: RegisterTableResponse = client
            .request_json(Method::POST, "/v1/bifrost/tables", Some(&request))
            .await
            .expect("declared layout registers over HTTP");

        let path = format!("/v1/bifrost/tables/{NAMESPACE}/{name}");
        let http: BifrostTableDescription = client
            .request_json(Method::GET, &path, None::<&()>)
            .await
            .expect("HTTP describe returns the canonical description");

        let registry = ToolRegistry::new();
        register_bifrost_tools(&registry, Arc::clone(&client)).expect("Bifrost tools register");
        let mcp = registry
            .resolve("bifrost.describe_table")
            .expect("describe_table is registered")
            .invoke(json!({"namespace": NAMESPACE, "name": name}))
            .await
            .expect("MCP describe succeeds for the authorized principal");

        assert_eq!(
            mcp,
            serde_json::to_value(&http).expect("HTTP description serializes"),
            "MCP describe must project the HTTP description verbatim"
        );

        // Guard the comparison against a vacuous pass: the layout both surfaces
        // agree on is the canonical one, not an empty or defaulted object.
        let layout = &http.physical_layout;
        assert_eq!(layout.partition_granularity, TimeGranularityWire::Hour);
        assert_eq!(layout.sort_keys, declared_layout().sort_keys);
        assert_eq!(
            layout.bloom_columns,
            vec![
                "run_id".to_owned(),
                "card_uid".to_owned(),
                "principal_id".to_owned(),
                "value".to_owned(),
            ],
            "the canonical Bloom set is the managed floor followed by the caller's column"
        );
        assert!(
            !layout
                .sort_keys
                .iter()
                .any(|key| key.column == "data_tenant_id"),
            "the per-file-constant tenant column is never a canonical sort key"
        );
        assert!(
            !layout
                .bloom_columns
                .iter()
                .any(|column| column == "data_tenant_id"),
            "the per-file-constant tenant column is never a canonical Bloom column"
        );

        server.shutdown().await.expect("server shuts down");
    }
}
