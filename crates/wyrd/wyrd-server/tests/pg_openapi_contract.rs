//! Served-contract coverage for `GET /openapi.json`.
//!
//! Every case drives [`WyrdTestServer`], so the document under assertion is the
//! one the composed production router actually serves rather than a rendering
//! of [`WyrdApiDoc`] in isolation. The suite locks the four properties the
//! contract has to hold: the document is served as JSON and nothing else is,
//! every route the server mounts is described by it, the operations that clear
//! the document-wide security requirement are exactly the ones an anonymous
//! caller can reach, and a refusal produced at runtime carries a stable code
//! that the owning operation already documents.
//!
//! [`WyrdApiDoc`]: wyrd_server::http::openapi::WyrdApiDoc

use std::collections::BTreeMap;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use wyrd_spec::storage::ids::UploadId;
use wyrd_testing::WyrdTestServer;

/// The HTTP methods an OpenAPI path item may key an operation by.
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
async fn problem_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body collects");
    serde_json::from_slice(&body).expect("problem JSON")
}

/// Extract every literal path passed to `.route("…")` inside one source span.
///
/// Axum exposes no route table, so the registrations themselves are the only
/// declaration of what the server mounts. Wildcard captures are normalized
/// (`{*id}` → `{id}`) because OpenAPI has one template syntax. `.route_service`
/// is deliberately not matched: the MCP endpoint it mounts speaks its own
/// protocol and is not an OpenAPI operation.
fn routes_in(source: &str) -> Vec<String> {
    source
        .split(".route(")
        .skip(1)
        .filter_map(|rest| {
            let open = rest.find('"')?;
            let close = rest[open + 1..].find('"')?;
            Some(rest[open + 1..=open + close].replace("{*", "{"))
        })
        .collect()
}

/// Extract every identifier passed to `.merge(…)` inside one source span.
fn merges_in(source: &str) -> Vec<String> {
    source
        .split(".merge(")
        .skip(1)
        .filter_map(|rest| {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .collect()
}

/// Return the source span between `open` and the first following `close`.
///
/// # Panics
/// Panics when either marker is absent, which means the router was restructured
/// and this derivation needs to follow it.
fn span<'a>(source: &'a str, open: &str, close: &str) -> &'a str {
    let start = source.find(open).unwrap_or_else(|| panic!("{open} exists"));
    let rest = &source[start..];
    let end = rest.find(close).unwrap_or_else(|| panic!("{close} exists"));
    &rest[..end]
}

/// Return the body of the top-level `fn <name>` in one source file.
///
/// Scoping to the defining function keeps routes registered by a test module
/// beside it out of the derivation.
///
/// # Panics
/// Panics when the file defines no such function.
fn fn_body(source: &str, name: &str) -> String {
    let start = source
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("fn {name} is defined here"));
    let rest = &source[start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |offset| offset + 2);
    rest[..end].to_owned()
}

/// Read the source of the router-building function `build_router` calls under
/// `name`, following the router's own `use … as …` aliases to the module that
/// defines it and otherwise searching the crate for the definition.
///
/// # Panics
/// Panics when neither the alias target nor a crate-wide definition resolves.
fn router_source(router: &str, name: &str) -> String {
    let alias = format!(" as {name};");
    if let Some(line) = router
        .lines()
        .find(|line| line.starts_with("use ") && line.ends_with(&alias))
    {
        let mut parts: Vec<&str> = line
            .trim_start_matches("use ")
            .split(" as ")
            .next()
            .expect("the import names a path")
            .split("::")
            .collect();
        let function = parts.pop().expect("the path names a function");
        parts.remove(0);
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(parts.join("/"));
        let file = [base.with_extension("rs"), base.join("mod.rs")]
            .into_iter()
            .find(|candidate| candidate.exists())
            .unwrap_or_else(|| panic!("{name} resolves to a module file"));
        return fn_body(
            &std::fs::read_to_string(file).expect("module reads"),
            function,
        );
    }
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in walk(&src) {
        let source = std::fs::read_to_string(&file).expect("source file reads");
        if source.contains(&format!("fn {name}(")) {
            return fn_body(&source, name);
        }
    }
    panic!("{name} is defined somewhere under src/");
}

/// Every `.rs` file under `root`, recursively.
fn walk(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// Derive every public operation the assembled router mounts, as
/// `(method-agnostic) path` strings carrying the prefix the router nests them
/// under.
///
/// The derivation follows `build_router`: the component routers merged into
/// `v1_group` are nested under `/v1`, the ones merged into `protected` are
/// served unprefixed. The liveness probes are not merged into either group and
/// so are excluded by construction, not by a list.
fn mounted_paths() -> Vec<String> {
    let router = include_str!("../src/http/router.rs");
    let aliases: BTreeMap<&str, &str> = [("auth_routes", "auth_router")].into_iter().collect();
    let mut paths = Vec::new();
    for (span_open, span_close, prefix) in [
        ("let v1_group = Router::new()", ".fallback(", "/v1"),
        ("let protected = apply_protected_edge(", ".nest(", ""),
    ] {
        for name in merges_in(span(router, span_open, span_close)) {
            let name = aliases.get(name.as_str()).map_or(name.as_str(), |it| it);
            if !name.ends_with("_router") {
                continue;
            }
            paths.extend(
                routes_in(&router_source(router, name))
                    .into_iter()
                    .map(|path| format!("{prefix}{path}")),
            );
        }
    }
    paths
}

/// Every route the composed router mounts must be described by the document it
/// serves, so a caller that reads the contract learns the whole public surface.
#[tokio::test]
async fn every_mounted_public_route_is_documented() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let document = served_document(&server).await;

    let documented = document["paths"].as_object().expect("paths object");
    let mounted = mounted_paths();
    assert!(!mounted.is_empty(), "the router mounts public routes");
    let undocumented: Vec<&String> = mounted
        .iter()
        .filter(|path| !documented.contains_key(path.as_str()))
        .collect();

    assert!(
        undocumented.is_empty(),
        "mounted routes missing from /openapi.json: {undocumented:?}"
    );

    server.shutdown().await.expect("server shuts down");
}

/// `GET /openapi.json` is the whole contract: no YAML projection is routed, and
/// the served document identifies itself as OpenAPI and describes real paths.
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
