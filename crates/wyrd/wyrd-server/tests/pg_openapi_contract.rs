//! Served-contract coverage for `GET /openapi.json`.
//!
//! Every case drives [`WyrdTestServer`], so the document under assertion is the
//! one the composed production router actually serves rather than a rendering
//! of the document type in isolation. Routing and documentation come out of one
//! `utoipa-axum` registration in the owning route modules, so the suite asserts
//! what that registration cannot make true by construction: the document is
//! served as JSON and nothing else is, the composed surface carries its nesting
//! prefix, every problem body names a real catalog code under its own status,
//! the operations that clear the document-wide security requirement are exactly
//! the ones an anonymous caller can reach, and a refusal produced at runtime
//! carries a stable code that the owning operation already documents.

use axum::response::Response;
use std::collections::BTreeSet;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use secrecy::ExposeSecret as _;
use serde_json::Value;
use wyrd_spec::error::WyrdError;
use wyrd_spec::storage::ids::UploadId;
use wyrd_testing::WyrdTestServer;

/// Media type RFC 9457 problem bodies are served with.
const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";

/// Name the contract gives the one Wyrd authentication scheme.
const WYRD_ACCESS_TOKEN_SCHEME: &str = "wyrdAccessToken";

/// The HTTP methods an `OpenAPI` path item may key an operation by.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

/// Fetch and parse the document the assembled server serves.
///
/// Asserts the transport contract alongside the payload — status, media type,
/// and JSON shape — because a caller that cannot identify the media type cannot
/// use the document regardless of its contents.
///
/// # Panics
/// Panics when the request fails, the response is not a `200 application/json`,
/// or the body is not JSON.
async fn served_document(server: &WyrdTestServer) -> Value {
    let response = server
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json"),
        "the contract is served as JSON"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    serde_json::from_slice(&body).expect("the served contract is JSON")
}

/// Collect a response body and parse it as the Wyrd problem+json envelope.
///
/// # Panics
/// Panics when the body cannot be collected or is not valid JSON.
async fn problem_json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    serde_json::from_slice(&body).expect("problem JSON")
}

/// Pull every `WYRD_…` stable code named in a response description.
///
/// Descriptions are prose with codes in parentheses rather than a structured
/// field, so the codes are recovered by scanning for the one prefix the catalog
/// uses and taking the identifier that follows.
fn stable_codes(description: &str) -> Vec<String> {
    description
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| token.starts_with("WYRD_"))
        .map(str::to_owned)
        .collect()
}

/// The HTTP status the catalog declares for one stable code.
///
/// [`WyrdError::from_code`] reconstructs only the variants whose fields are
/// `{ message, details }`, which leaves the delegated sub-catalogs — storage and
/// Bifrost — unreachable through it. Those codes carry their status in their own
/// second segment, which is written beside the `status = N` the derive reads, so
/// a code that disagrees with the response it is documented under is caught
/// either way.
fn catalog_status(code: &str) -> Option<u16> {
    WyrdError::from_code(code, "documented refusal".to_owned(), serde_json::json!({}))
        .map(|error| error.status())
        .or_else(|| code.split('_').nth(2)?.parse().ok())
}

/// The served document describes the routes the server mounts.
///
/// Routing and documentation come out of one `utoipa-axum` registration for
/// every route registered through `routes!`. What is pinned here is that the
/// composition actually ran: that the nesting prefix reached the operations and
/// that the surfaces mounted on both planes are present.
#[tokio::test]
async fn the_served_document_describes_the_composed_surface() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let paths = document["paths"].as_object().expect("paths object");

    for path in [
        "/v1/cards",
        "/v1/cards/by-uid/{kind}/{card_uid}",
        "/v1/cards/by-ref",
        "/v1/cards/{kind}/{space}/{name}/latest",
        "/v1/cards/{kind}/{space}/{name}/versions",
        "/v1/cards/{card_uid}/artifacts",
        "/v1/cards/{card_uid}/complete",
        "/v1/cards/download/init",
        "/v1/principals/{principal_id}/credentials/{credential_id}",
        "/v1/verification/bindings/{binding_id}",
        "/v1/verification/runs",
        "/v1/verification/runs/{run_id}",
        "/v1/bifrost/tables",
        "/v1/bifrost/tables/{namespace}/{name}",
        "/auth/token",
        "/platform/tenants",
    ] {
        assert!(paths.contains_key(path), "missing {path}");
    }
    assert!(
        !paths.contains_key("/v1/cards/{card_uid}/abort"),
        "a route the server does not mount is not documented"
    );
    assert!(
        !paths.contains_key("/mcp"),
        "the MCP endpoint speaks its own protocol and is not an OpenAPI operation"
    );

    server.shutdown().await.expect("server shuts down");
}

/// The local backend's byte-transfer operations are public Wyrd operations and
/// are published with their real contract.
///
/// Upload initialization and download initialization hand these URLs out and
/// the shared client dispatches to them, so an independent client needs their
/// method, typed locator, binary body, authentication, and stable refusals from
/// the same document as every other operation. The in-process test server runs
/// the local backend, so both are mounted here.
#[tokio::test]
async fn local_transfer_operations_publish_their_binary_contract() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let problem_ref = "#/components/schemas/WyrdProblem";
    let octets = "application/octet-stream";

    let upload = &document["paths"]["/v1/cards/upload/local/{id}"]["put"];
    assert!(upload.is_object(), "the local upload is a documented PUT");
    let id = upload["parameters"]
        .as_array()
        .and_then(|params| params.iter().find(|param| param["name"] == "id"))
        .expect("the upload names its identifier");
    assert_eq!(id["in"], "path");
    assert_eq!(id["required"], true);
    assert_eq!(
        id["schema"]["$ref"], "#/components/schemas/UploadId",
        "the upload locator is the typed upload identifier"
    );
    assert!(
        upload["requestBody"]["content"][octets].is_object(),
        "the upload body is raw bytes: {}",
        upload["requestBody"]
    );
    assert_eq!(
        upload["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/LocalBlobUploadResponse"
    );

    let download = &document["paths"]["/v1/cards/download/local"]["get"];
    assert!(
        download.is_object(),
        "the local download is a documented GET"
    );
    let path = download["parameters"]
        .as_array()
        .and_then(|params| params.iter().find(|param| param["name"] == "path"))
        .expect("the download names its object");
    assert_eq!(
        path["in"], "query",
        "a slash-bearing object path is one query value"
    );
    assert_eq!(path["required"], true);
    assert!(
        download["responses"]["200"]["content"][octets].is_object(),
        "the download answers with raw bytes: {}",
        download["responses"]["200"]
    );

    for (operation, specific) in [
        (upload, ["400", "404", "409"].as_slice()),
        (download, &["400", "404"]),
    ] {
        assert!(
            operation.get("security").is_none(),
            "a local transfer inherits the document's authentication requirement"
        );
        for status in ["401", "403", "500", "503"].iter().chain(specific) {
            let response = &operation["responses"][*status];
            assert_eq!(
                response["content"][PROBLEM_MEDIA_TYPE]["schema"]["$ref"], problem_ref,
                "{status} is published as WyrdProblem problem+json"
            );
            assert!(
                !stable_codes(response["description"].as_str().unwrap_or_default()).is_empty(),
                "{status} names its stable codes"
            );
        }
    }
    assert!(
        document["paths"]
            .as_object()
            .expect("paths object")
            .keys()
            .all(|key| !key.contains('*')),
        "no wildcard route template reaches the document"
    );

    server.shutdown().await.expect("server shuts down");
}

/// Every documented problem body is served as `application/problem+json` and
/// names real catalog codes for its status.
///
/// A generated client branches on the media type and on the code; declaring a
/// problem as plain `application/json`, or naming a code that disagrees with the
/// status it is documented under, breaks that branch silently.
///
/// Every operation is held to this, not a chosen subset of tags: a caller
/// reaching the storage, evaluation, authorization, or OTLP surface branches on
/// refusals exactly the way an operator branches on an administrative one.
#[tokio::test]
async fn every_problem_response_declares_its_media_type_and_stable_code() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let catalog: BTreeSet<&'static str> = WyrdError::codes().into_iter().collect();
    let mut problems = 0_usize;
    let mut defects = Vec::new();

    for (path, item) in document["paths"].as_object().expect("paths object") {
        for (method, operation) in item.as_object().expect("path item is an object") {
            let responses = operation["responses"]
                .as_object()
                .expect("an operation declares responses");
            for (status, response) in responses {
                let content = &response["content"];
                if content["application/json"]["schema"]["$ref"]
                    .as_str()
                    .is_some_and(|reference| reference.ends_with("/WyrdProblem"))
                {
                    defects.push(format!(
                        "{method} {path} {status} is problem+json as plain JSON"
                    ));
                    continue;
                }
                if !content[PROBLEM_MEDIA_TYPE].is_object() {
                    continue;
                }
                problems += 1;
                let description = response["description"].as_str().unwrap_or_default();
                let codes = stable_codes(description);
                if codes.is_empty() {
                    defects.push(format!(
                        "{method} {path} {status} names no code: {description}"
                    ));
                    continue;
                }
                // `default` is the catch-all arm rather than one status, so its
                // codes are checked for existence and nothing more.
                let expected: Option<u16> = status.parse().ok();
                for code in codes {
                    if !catalog.contains(code.as_str()) {
                        defects.push(format!(
                            "{method} {path} {status} names {code}, absent from the catalog"
                        ));
                        continue;
                    }
                    let Some(declared) = catalog_status(&code) else {
                        continue;
                    };
                    if expected.is_some_and(|expected| declared != expected) {
                        defects.push(format!(
                            "{method} {path} documents {code} under {status}, but the catalog \
                             gives it {declared}"
                        ));
                    }
                }
            }
        }
    }

    assert!(defects.is_empty(), "{}", defects.join("\n"));
    assert!(
        problems > 0,
        "the contract declares no problem responses at all"
    );

    server.shutdown().await.expect("server shuts down");
}

/// The contract names one authentication scheme and requires it by default.
///
/// Authentication is a property of the whole surface, so the requirement is
/// declared once on the document and inherited. An operation a caller reaches
/// before it can have a session clears the requirement beside its own handler
/// with `security(())`, which is the only override the contract permits: a
/// per-operation requirement naming some *other* scheme would be a second
/// authentication story, and there is only one header.
#[tokio::test]
async fn every_authenticated_path_declares_the_one_wyrd_scheme() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let scheme = &document["components"]["securitySchemes"][WYRD_ACCESS_TOKEN_SCHEME];
    assert_eq!(scheme["type"], "apiKey");
    assert_eq!(scheme["in"], "header");
    assert_eq!(scheme["name"], "X-Wyrd-Access-Token");
    assert_eq!(
        document["security"],
        serde_json::json!([{ WYRD_ACCESS_TOKEN_SCHEME: [] }]),
        "the document requires the scheme by default"
    );

    let mut cleared = BTreeSet::new();
    for (path, item) in document["paths"].as_object().expect("paths object") {
        for (method, operation) in item.as_object().expect("path item is an object") {
            let Some(overridden) = operation.get("security") else {
                continue;
            };
            // utoipa renders `security(())` as one empty requirement object,
            // which is OpenAPI's way of saying the operation needs nothing.
            assert!(
                overridden == &serde_json::json!([]) || overridden == &serde_json::json!([{}]),
                "{method} {path} overrides the document requirement with a second scheme"
            );
            cleared.insert(path.clone());
        }
    }
    assert!(
        !cleared.is_empty(),
        "sign-in and credential exchange cannot themselves require a session"
    );

    server.shutdown().await.expect("server shuts down");
}

/// A protected tenant operation documents only the refusals its local verifier
/// can reach.
///
/// Tenant access tokens are five-minute permission snapshots verified without
/// a database, so a request can be unauthenticated, invalid, or expired, but
/// never `WYRD_AUTH_401_CREDENTIAL_REVOKED`: revocation refuses the next
/// issuance instead. Operations that clear the document requirement (issuance)
/// and the platform plane, which revalidates current state, are excluded.
#[tokio::test]
async fn protected_tenant_operations_document_no_revocation_refusal() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;

    let mut protected = 0;
    for (path, item) in document["paths"].as_object().expect("paths object") {
        if path.starts_with("/platform") {
            continue;
        }
        for (method, operation) in item.as_object().expect("path item is an object") {
            if !METHODS.contains(&method.as_str()) || operation.get("security").is_some() {
                continue;
            }
            protected += 1;
            let description = operation["responses"]["401"]["description"]
                .as_str()
                .unwrap_or_default();
            assert!(
                !stable_codes(description).contains(&"WYRD_AUTH_401_CREDENTIAL_REVOKED".to_owned()),
                "{method} {path} documents a revocation refusal its verifier cannot emit"
            );
        }
    }
    assert!(
        protected > 0,
        "the document serves protected tenant operations"
    );

    server.shutdown().await.expect("server shuts down");
}

/// Every public Bifrost table, query, and lifecycle operation publishes its
/// pre-stream refusals as typed `WyrdProblem` bodies.
#[tokio::test]
async fn bifrost_operations_publish_typed_problem_refusals() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let problem_ref = "#/components/schemas/WyrdProblem";
    let operations: [(&str, &str, &[&str]); 7] = [
        ("/v1/bifrost/tables", "post", &["400", "409", "503"]),
        ("/v1/bifrost/tables", "get", &["503"]),
        (
            "/v1/bifrost/tables/{namespace}/{name}",
            "get",
            &["400", "404", "503"],
        ),
        ("/v1/query", "post", &["400", "503"]),
        ("/v1/query/running", "get", &["409", "503"]),
        (
            "/v1/query/{request_id}",
            "get",
            &["400", "404", "409", "503"],
        ),
        (
            "/v1/query/{request_id}",
            "delete",
            &["400", "404", "409", "503"],
        ),
    ];
    for (path, method, specific) in operations {
        let responses = &document["paths"][path][method]["responses"];
        for status in ["401", "403", "default"].iter().chain(specific) {
            assert_eq!(
                responses[*status]["content"][PROBLEM_MEDIA_TYPE]["schema"]["$ref"], problem_ref,
                "{method} {path} must publish {status} as WyrdProblem problem+json"
            );
            assert!(
                responses[*status]["content"]["application/json"].is_null(),
                "{method} {path} must not publish {status} as plain JSON"
            );
        }
    }

    server.shutdown().await.expect("server shuts down");
}

/// Follow `schema` to the component it names in the served `document`.
///
/// Resolves a `$ref`, and unwraps the single non-null alternative utoipa emits
/// for an optional reference (`oneOf`/`anyOf`/`allOf` with a `null` arm), so a
/// caller can walk a property chain without caring how optionality is encoded.
///
/// # Panics
/// Panics when a `$ref` is not a local component reference.
fn resolve_schema<'a>(document: &'a Value, schema: &'a Value) -> &'a Value {
    if let Some(reference) = schema["$ref"].as_str() {
        let name = reference
            .strip_prefix("#/components/schemas/")
            .expect("schema references are local components");
        return resolve_schema(document, &document["components"]["schemas"][name]);
    }
    for combinator in ["oneOf", "anyOf", "allOf"] {
        if let Some(arms) = schema[combinator].as_array() {
            let mut concrete = arms.iter().filter(|arm| arm["type"] != "null");
            if let (Some(arm), None) = (concrete.next(), concrete.next()) {
                return resolve_schema(document, arm);
            }
        }
    }
    schema
}

/// The card surface publishes its typed lifecycle, parameter, and problem
/// shapes.
///
/// Also walks the served `Card -> Status -> verification -> binding_ids`
/// chain, proving the read-side binding identities are a UUID array.
///
/// # Panics
/// Panics when the server fails to start or stop or any published shape
/// differs.
#[tokio::test]
async fn card_contract_publishes_typed_lifecycle_and_problem_shapes() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let paths = document["paths"].as_object().expect("paths object");

    let parameters = paths["/v1/cards"]["get"]["parameters"]
        .as_array()
        .expect("list parameters");
    let parameter_names: BTreeSet<&str> = parameters
        .iter()
        .filter_map(|parameter| parameter["name"].as_str())
        .collect();
    assert_eq!(
        parameter_names,
        BTreeSet::from([
            "kind",
            "space",
            "name",
            "version_range",
            "status",
            "filter",
            "include_prerelease",
            "limit",
            "cursor",
        ])
    );

    let card = &document["components"]["schemas"]["Card"];
    let required = card["required"].as_array().expect("Card required fields");
    assert!(required.iter().any(|field| field == "apiVersion"));
    assert_eq!(
        document["components"]["schemas"]["Spec"]["oneOf"]
            .as_array()
            .expect("typed spec alternatives")
            .len(),
        15,
        "one typed spec per registrable native Card kind"
    );

    let status = resolve_schema(&document, &card["properties"]["status"]);
    let verification = resolve_schema(&document, &status["properties"]["verification"]);
    assert_eq!(
        verification, &document["components"]["schemas"]["VerificationStatus"],
        "Card status carries the named verification status"
    );
    let binding_ids = resolve_schema(&document, &verification["properties"]["binding_ids"]);
    assert_eq!(binding_ids["type"], "array", "binding IDs are an array");
    let binding_id = resolve_schema(&document, &binding_ids["items"]);
    assert_eq!(binding_id["type"], "string");
    assert_eq!(binding_id["format"], "uuid", "each binding ID is a UUID");

    let problem = &document["components"]["schemas"]["WyrdProblem"];
    let problem_required: BTreeSet<&str> = problem["required"]
        .as_array()
        .expect("problem required fields")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(
        problem_required,
        BTreeSet::from([
            "type",
            "title",
            "status",
            "detail",
            "code",
            "details",
            "remediation",
        ])
    );

    server.shutdown().await.expect("server shuts down");
}

/// The Verification surface is exactly three typed operations.
///
/// Binding status, manual run request, and run status publish their typed
/// request and response schemas, the manual request answers `202 Accepted`,
/// and no result or other Verification operation is routed: verdicts are
/// read from Bifrost by `result_id`.
///
/// # Panics
/// Panics when the server fails to start or stop, or when a Verification path,
/// method, status, or schema reference differs.
#[tokio::test]
async fn verification_contract_publishes_exactly_three_typed_operations() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let paths = document["paths"].as_object().expect("paths object");

    let verification: BTreeSet<(&str, &str)> = paths
        .iter()
        .filter(|(path, _)| path.starts_with("/v1/verification"))
        .flat_map(|(path, item)| {
            item.as_object()
                .expect("path item object")
                .keys()
                .filter(|key| ["get", "post", "put", "patch", "delete"].contains(&key.as_str()))
                .map(move |method| (path.as_str(), method.as_str()))
        })
        .collect();
    assert_eq!(
        verification,
        BTreeSet::from([
            ("/v1/verification/bindings/{binding_id}", "get"),
            ("/v1/verification/runs", "post"),
            ("/v1/verification/runs/{run_id}", "get"),
        ])
    );

    let schema_ref = |operation: &Value, status: &str| {
        operation["responses"][status]["content"]["application/json"]["schema"]["$ref"]
            .as_str()
            .map(str::to_owned)
    };
    let start = &paths["/v1/verification/runs"]["post"];
    assert_eq!(
        start["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/StartVerificationRunRequest"
    );
    assert_eq!(
        schema_ref(start, "202").as_deref(),
        Some("#/components/schemas/StartVerificationRunResponse")
    );
    assert_eq!(
        schema_ref(
            &paths["/v1/verification/bindings/{binding_id}"]["get"],
            "200"
        )
        .as_deref(),
        Some("#/components/schemas/VerificationBindingStatus")
    );
    assert_eq!(
        schema_ref(&paths["/v1/verification/runs/{run_id}"]["get"], "200").as_deref(),
        Some("#/components/schemas/VerificationRunStatus")
    );

    server.shutdown().await.expect("server shuts down");
}

/// Issued and listed credential ids publish the same UUID contract the revoke
/// path parameter and the MCP tools use, so no surface advertises free text.
///
/// Covers the principal credential routes and the Card-bound
/// `POST /auth/issue-key` response, whose `key_id` is the same credential id.
///
/// # Panics
///
/// Panics when the server cannot start or a credential id is not published
/// with the `uuid` format.
#[tokio::test]
async fn credential_ids_publish_their_uuid_contract() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;

    for component in ["IssuedCredential", "CredentialMetadata"] {
        let id = &document["components"]["schemas"][component]["properties"]["id"];
        assert_eq!(id["format"], "uuid", "{component}.id is a UUID: {id}");
    }
    let key_id = &document["components"]["schemas"]["IssueKeyResponse"]["properties"]["key_id"];
    assert_eq!(
        key_id["format"], "uuid",
        "IssueKeyResponse.key_id is a UUID: {key_id}"
    );

    server.shutdown().await.expect("server shuts down");
}

/// `GET /openapi.json` is the whole contract: no YAML projection is routed, and
/// the served document identifies itself as `OpenAPI` and describes real paths.
#[tokio::test]
async fn no_yaml_projection_is_routed() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");

    let document = served_document(&server).await;
    assert!(
        document["openapi"]
            .as_str()
            .is_some_and(|v| v.starts_with('3')),
        "the served document declares its OpenAPI version"
    );
    assert!(document["info"]["title"].is_string());
    assert!(
        document["paths"]
            .as_object()
            .is_some_and(|paths| !paths.is_empty()),
        "the served document describes the server's routes"
    );

    // The edge answers an anonymous caller 401 for any unmatched path, so the
    // probe carries a credential: only a genuinely unrouted path reaches the
    // router's own not-found.
    let reader = server
        .bootstrap_service("openapi-yaml-probe", &["reader"])
        .await
        .expect("service bootstraps");
    let token = server
        .exchange_api_key(reader.api_key().expect("machine has key"))
        .await
        .expect("api key exchanges");
    let response = server
        .oneshot_authenticated(
            &token,
            Request::builder()
                .uri("/openapi.yaml")
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "no YAML projection of the document is routed"
    );

    server.shutdown().await.expect("server shuts down");
}

/// The operations that carry their own `security` key are exactly the ones an
/// anonymous caller can reach.
///
/// This is the behavioural replacement for the hand-written anonymous-path
/// list: each operation's own declaration is the input, and the assembled
/// server's answer to a credential-free request is the proof. Every other
/// operation must refuse with `WYRD_AUTH_401_UNAUTHENTICATED`, which is also
/// what proves it is mounted behind the default-deny layer.
#[tokio::test]
async fn unauthenticated_requests_match_each_operation_declared_security() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;

    let mut cleared = 0_usize;
    let mut defects: Vec<String> = Vec::new();
    for (path, item) in document["paths"].as_object().expect("paths object") {
        for method in METHODS {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let uri = template_to_uri(path);
            let response = server
                .oneshot(
                    Request::builder()
                        .method(method.to_uppercase().as_str())
                        .uri(&uri)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from("{}"))
                        .expect("request builds"),
                )
                .await
                .expect("router responds");
            let status = response.status();
            let code = if status == StatusCode::UNAUTHORIZED {
                problem_json(response).await["code"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            } else {
                String::new()
            };

            if operation.get("security").is_some() {
                cleared += 1;
                if code == "WYRD_AUTH_401_UNAUTHENTICATED" {
                    defects.push(format!(
                        "{method} {path} clears the security requirement but refused an \
                         anonymous caller as unauthenticated"
                    ));
                }
            } else if code != "WYRD_AUTH_401_UNAUTHENTICATED" {
                defects.push(format!(
                    "{method} {path} inherits the document-wide security requirement but \
                     answered {status} ({code}) without a token"
                ));
            }
        }
    }

    assert!(cleared > 0, "some operations clear the requirement");
    assert!(defects.is_empty(), "{}", defects.join("\n"));

    server.shutdown().await.expect("server shuts down");
}

/// Replace every `{template}` segment with a probe value so the path routes.
///
/// The value never reaches a handler in these probes — authentication refuses
/// first — so any non-empty segment that survives URI parsing will do.
fn template_to_uri(path: &str) -> String {
    let mut uri = String::with_capacity(path.len());
    let mut depth = 0_usize;
    for ch in path.chars() {
        match ch {
            '{' => {
                depth += 1;
                if depth == 1 {
                    uri.push_str("probe");
                }
            }
            '}' => depth -= 1,
            _ if depth == 0 => uri.push(ch),
            _ => {}
        }
    }
    uri
}

/// Return the description the document publishes for one operation's status.
///
/// # Panics
/// Panics when the operation or the status is absent, which means the runtime
/// answered with a refusal the contract does not describe at all.
fn documented_description(document: &Value, path: &str, method: &str, status: u16) -> String {
    document["paths"][path][method]["responses"][status.to_string()]["description"]
        .as_str()
        .unwrap_or_else(|| panic!("{method} {path} documents a {status} response"))
        .to_owned()
}

/// A storage lookup that finds nothing must answer with the stable code its own
/// operation documents, not with an undocumented shape.
#[tokio::test]
async fn an_unknown_upload_answers_with_a_code_the_operation_documents() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let writer = server
        .bootstrap_service("openapi-storage", &["writer"])
        .await
        .expect("service bootstraps");
    let token = server
        .exchange_api_key(writer.api_key().expect("machine has key"))
        .await
        .expect("api key exchanges");

    let response = server
        .oneshot_authenticated(
            &token,
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/cards/upload/{}/part-url?part_number=1",
                    UploadId::new()
                ))
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let problem = problem_json(response).await;
    let code = problem["code"].as_str().expect("problem carries a code");
    assert_eq!(code, "WYRD_STORAGE_404_UPLOAD_NOT_FOUND");
    assert!(
        documented_description(&document, "/v1/cards/upload/{id}/part-url", "post", 404)
            .contains(code),
        "the owning operation names {code} on its 404"
    );

    server.shutdown().await.expect("server shuts down");
}

/// A local transfer whose locator cannot be extracted answers with the
/// canonical problem response and a code its own operation documents.
///
/// A missing or undecodable download `path` query and an undecodable upload
/// identifier are refused by the route's extractor before the handler body
/// runs, so this drives each through the assembled authenticated router and
/// checks status, problem media type, envelope shape, and the documented code.
///
/// # Panics
/// Panics when the server cannot start, a request fails, or a refusal is not
/// the documented problem response.
#[tokio::test]
async fn an_unextractable_local_transfer_locator_answers_with_a_documented_problem() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let writer = server
        .bootstrap_service("openapi-local-transfer", &["writer"])
        .await
        .expect("service bootstraps");
    let token = server
        .exchange_api_key(writer.api_key().expect("machine has key"))
        .await
        .expect("api key exchanges");

    let cases = [
        (
            "GET",
            "/v1/cards/download/local",
            "/v1/cards/download/local",
            "get",
            "WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD",
        ),
        (
            "GET",
            "/v1/cards/download/local?path=a&path=b",
            "/v1/cards/download/local",
            "get",
            "WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD",
        ),
        (
            "PUT",
            "/v1/cards/upload/local/%FF",
            "/v1/cards/upload/local/{id}",
            "put",
            "WYRD_STORAGE_400_INVALID_UPLOAD_ID",
        ),
    ];
    for (method, uri, path, operation, expected) in cases {
        let response = server
            .oneshot_authenticated(
                &token,
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method} {uri}");
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some(PROBLEM_MEDIA_TYPE),
            "{method} {uri} is a problem response"
        );
        let problem = problem_json(response).await;
        assert_eq!(problem["status"], 400, "{method} {uri}: {problem}");
        assert!(problem["title"].is_string(), "{method} {uri}: {problem}");
        assert_eq!(problem["code"], expected, "{method} {uri}: {problem}");
        assert!(
            documented_description(&document, path, operation, 400).contains(expected),
            "{method} {path} names {expected} on its 400"
        );
    }

    server.shutdown().await.expect("server shuts down");
}

/// Resolve one operation's path-parameter schema, following a component `$ref`.
///
/// # Panics
/// Panics when the operation does not publish the named path parameter.
fn path_parameter_schema(document: &Value, path: &str, method: &str, name: &str) -> Value {
    let schema = document["paths"][path][method]["parameters"]
        .as_array()
        .and_then(|parameters| {
            parameters
                .iter()
                .find(|parameter| parameter["name"] == name && parameter["in"] == "path")
        })
        .unwrap_or_else(|| panic!("{method} {path} publishes path parameter {name}"))["schema"]
        .clone();
    match schema["$ref"].as_str() {
        Some(reference) => {
            let component = reference.trim_start_matches("#/components/schemas/");
            document["components"]["schemas"][component].clone()
        }
        None => schema,
    }
}

/// A malformed administrative path identifier is excluded by the published
/// typed parameter and refused at runtime with the problem that operation
/// documents.
///
/// Covers one tenant principal path, one platform principal path, and one
/// platform tenant path through the assembled authenticated router: each
/// publishes a UUID-formatted parameter, and a non-UUID segment answers `400`
/// `application/problem+json` with `WYRD_SPEC_400_VALIDATION`, which the
/// operation lists under its `400`.
///
/// # Panics
/// Panics when the server cannot start, a credential cannot be exchanged, or a
/// refusal disagrees with the published contract.
#[tokio::test]
async fn a_malformed_administrative_identifier_answers_with_a_documented_problem() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let tenant = server
        .bootstrap_service("openapi-malformed-id", &["reader"])
        .await
        .expect("service bootstraps");
    let tenant_token = server
        .exchange_api_key(tenant.api_key().expect("machine has key"))
        .await
        .expect("api key exchanges");
    let root = server
        .initialize_platform_root()
        .await
        .expect("platform root initializes");
    let exchanged = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/platform/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "credential": root.expose_secret() }).to_string(),
                ))
                .expect("request builds"),
        )
        .await
        .expect("platform token route responds");
    assert_eq!(exchanged.status(), StatusCode::OK);
    let platform_token = problem_json(exchanged).await["access_token"]
        .as_str()
        .expect("platform session token")
        .to_owned();

    let cases = [
        (
            tenant_token.as_str(),
            "/v1/principals/not-a-uuid/credentials",
            "/v1/principals/{principal_id}/credentials",
            "principal_id",
        ),
        (
            platform_token.as_str(),
            "/platform/admins/not-a-uuid/credentials",
            "/platform/admins/{principal_id}/credentials",
            "principal_id",
        ),
        (
            platform_token.as_str(),
            "/platform/tenants/not-a-uuid",
            "/platform/tenants/{tenant_id}",
            "tenant_id",
        ),
    ];
    for (token, uri, path, parameter) in cases {
        let schema = path_parameter_schema(&document, path, "get", parameter);
        assert_eq!(
            schema["format"], "uuid",
            "{path} publishes a typed {parameter}: {schema}"
        );

        let response = server
            .oneshot_authenticated(
                token,
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router responds");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "GET {uri}");
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some(PROBLEM_MEDIA_TYPE),
            "GET {uri} is a problem response"
        );
        let problem = problem_json(response).await;
        assert_eq!(problem["status"], 400, "GET {uri}: {problem}");
        assert_eq!(
            problem["code"], "WYRD_SPEC_400_VALIDATION",
            "GET {uri}: {problem}"
        );
        assert!(
            documented_description(&document, path, "get", 400)
                .contains("WYRD_SPEC_400_VALIDATION"),
            "GET {path} lists WYRD_SPEC_400_VALIDATION on its 400"
        );
    }

    server.shutdown().await.expect("server shuts down");
}

/// An audit store the server cannot append to must fail the decision closed
/// with the stable code its own operation documents.
///
/// The shared authorization path stages its row through the Vala outbox, so the
/// reachable refusal is `WYRD_VALA_500_AUDIT_UNAVAILABLE` — not the credential
/// issuance catalog's `WYRD_AUDIT_503_UNAVAILABLE`, which only the auth grant
/// handlers reach.
///
/// The failure is injected at the store: the canonical staging table is renamed
/// out from under the append, which is the one dependency every authorization
/// decision shares, so no handler-level seam has to be stubbed.
#[tokio::test]
async fn an_unavailable_audit_store_answers_with_a_code_the_operation_documents() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let writer = server
        .bootstrap_service("openapi-audit", &["writer"])
        .await
        .expect("service bootstraps");
    let token = server
        .exchange_api_key(writer.api_key().expect("machine has key"))
        .await
        .expect("api key exchanges");
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query("ALTER TABLE vala.audit_staging RENAME TO audit_staging_offline")
        .execute(&pool)
        .await
        .expect("audit staging goes offline");

    let response = server
        .oneshot_authenticated(
            &token,
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/cards/upload/{}/part-url?part_number=1",
                    UploadId::new()
                ))
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    let status = response.status();
    let problem = problem_json(response).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{problem}");
    let code = problem["code"].as_str().expect("problem carries a code");
    assert_eq!(code, "WYRD_VALA_500_AUDIT_UNAVAILABLE");
    assert!(
        documented_description(&document, "/v1/cards/upload/{id}/part-url", "post", 500)
            .contains(code),
        "the owning operation names {code} on its 500"
    );

    sqlx::query("ALTER TABLE vala.audit_staging_offline RENAME TO audit_staging")
        .execute(&pool)
        .await
        .expect("audit staging comes back");

    server.shutdown().await.expect("server shuts down");
}

/// A credential exchange whose audit cannot be staged fails closed with the
/// stable code `/auth/token` documents.
///
/// `/auth/token` is the one operation every caller reaches before it has a
/// session, so the set of refusals it declares is the set a client has to be
/// able to branch on. The audit-unavailable arm is the one that used to go
/// undeclared: it is reachable from a perfectly valid credential, and it is the
/// arm that proves the grant and its audit commit together.
///
/// The failure is injected at the store — the canonical staging table is
/// renamed out from under the append — so no handler seam has to be stubbed.
#[tokio::test]
async fn an_unstageable_exchange_audit_answers_with_a_code_the_token_operation_documents() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;
    let service = server
        .bootstrap_service("openapi-token-audit", &["reader"])
        .await
        .expect("service bootstraps");
    let api_key = service
        .api_key()
        .expect("machine bootstraps with a key")
        .expose_secret()
        .to_owned();
    let pool = server
        .pg_fixture()
        .superuser_pool()
        .await
        .expect("superuser pool opens");
    sqlx::query("ALTER TABLE vala.audit_staging RENAME TO audit_staging_offline")
        .execute(&pool)
        .await
        .expect("audit staging goes offline");

    let response = server
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "grant_type": "wyrd_api_key",
                        "api_key": api_key,
                    })
                    .to_string(),
                ))
                .expect("request builds"),
        )
        .await
        .expect("router responds");

    let status = response.status();
    let problem = problem_json(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
    let code = problem["code"].as_str().expect("problem carries a code");
    assert_eq!(code, "WYRD_AUDIT_503_UNAVAILABLE");
    assert!(
        documented_description(&document, "/auth/token", "post", 503).contains(code),
        "the token operation names {code} on its 503"
    );

    sqlx::query("ALTER TABLE vala.audit_staging_offline RENAME TO audit_staging")
        .execute(&pool)
        .await
        .expect("audit staging comes back");

    server.shutdown().await.expect("server shuts down");
}
