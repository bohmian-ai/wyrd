//! `extract` with `extractor: pull_request` over admitted pull requests,
//! through the real webhook route, the Verifier runtime, the GitHub App mock
//! and repository Postgres.
//!
//! The tenant connects installation 101 with `acme/policy` (4242) selected
//! and declares the `policy` Repository binding the queued `pr-review`
//! Verifier to its pull requests. GitHub serves `acme/policy` whose `main`
//! is commit `c…` holding the user module and handler; pull request 7 changes
//! the handler at head `a…` and again at head `b…`.

use std::io::Write as _;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::{Value, json};
use sha1::{Digest as _, Sha1};
use uuid::Uuid;
use wyrd_loader::{build_declaration_input, load};
use wyrd_testing::github::{GithubAppMock, PullRequest};
use wyrd_testing::{Bootstrap, WyrdTestServer};

use super::pg_github_pull_requests::install;

/// The connected repository.
const REPOSITORY: u64 = 4242;
/// The connected installation.
const INSTALLATION: u64 = 101;
/// The reviewed pull request.
const PR: u64 = 7;
/// The Repository binding `pr-review` to its pull requests.
const POLICY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./pr-review.yaml\n      runs_on: {kind: pull_request}\n";
/// A queued Verifier extracting the pull request and requiring a change.
const REVIEW: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: pr-review\n  version: 1.0.0\nspec:\n  description: The pull request changes source.\n  steps:\n    - id: pr\n      action: {kind: extract, extractor: pull_request}\n      requirements:\n        - id: changes\n          statement: The reviewed change touches a file\n          check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}\n";

/// Declares `lookupUser`.
const USER: &[u8] = b"export function lookupUser(id: string): string {\n  return id;\n}\n";
/// The handler on `main`.
const HANDLER: &[u8] = b"import { lookupUser } from \"./user\";\n\nexport const handle = (id: string) => lookupUser(id);\n";
/// The handler at head `a…`.
const HANDLER_A: &[u8] = b"import { lookupUser } from \"./user\";\n\nexport const handle = (id: string) => lookupUser(id.trim());\n";
/// The handler at head `b…`.
const HANDLER_B: &[u8] = b"import { lookupUser } from \"./user\";\n\nexport const handle = (id: string) => lookupUser(id.trim().toLowerCase());\n";
/// `main` to head `a…`.
const PATCH_A: &str = "@@ -1,3 +1,3 @@\n import { lookupUser } from \"./user\";\n \n-export const handle = (id: string) => lookupUser(id);\n+export const handle = (id: string) => lookupUser(id.trim());";
/// Head `a…` to head `b…`.
const PATCH_AB: &str = "@@ -1,3 +1,3 @@\n import { lookupUser } from \"./user\";\n \n-export const handle = (id: string) => lookupUser(id.trim());\n+export const handle = (id: string) => lookupUser(id.trim().toLowerCase());";
/// `main` to head `b…`.
const PATCH_B: &str = "@@ -1,3 +1,3 @@\n import { lookupUser } from \"./user\";\n \n-export const handle = (id: string) => lookupUser(id);\n+export const handle = (id: string) => lookupUser(id.trim().toLowerCase());";

/// Forty copies of `ch`, a commit or tree id.
fn sha(ch: char) -> String {
    ch.to_string().repeat(40)
}

/// Git blob id of `bytes`.
fn blob(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Gzipped tarball of `files` under GitHub's single top-level directory.
///
/// # Panics
/// Panics if building or compressing the archive fails.
fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("acme-policy-ccccccc/{path}"), *bytes)
            .expect("entry");
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&builder.into_inner().expect("tar"))
        .expect("gzip");
    gz.finish().expect("gzip finish")
}

/// A connected, declared deployment whose GitHub serves `acme/policy`.
struct Deployment {
    /// GitHub.
    mock: GithubAppMock,
    /// Wyrd, running queued work.
    server: WyrdTestServer,
    /// The declaring editor's JWT.
    jwt: String,
}

impl Deployment {
    /// Start Wyrd with its Verifier runtime, connect the tenant through
    /// GitHub's install page, declare [`POLICY`], and serve `main` at `c…`.
    ///
    /// # Panics
    /// Panics when a step fails.
    async fn start() -> Self {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(INSTALLATION, &[(REPOSITORY, "acme/policy")]);
        mock.add_user("octo", &[INSTALLATION]);
        mock.sign_in("octo");
        mock.install_target(INSTALLATION);
        let server = WyrdTestServer::builder()
            .with_github_app_for_test(&mock)
            .with_verification_runtime_for_test()
            .start_bound()
            .await
            .expect("bound test server starts");
        install(
            &server,
            &mock,
            server.data_tenant_id(),
            server.pg_fixture().tenant_slug(),
            "github_admin",
        )
        .await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("pull-request-editor", &["editor"])
            .await
            .expect("editor bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        let deployment = Self { mock, server, jwt };
        deployment.declare().await;
        deployment.serve_main();
        deployment
    }

    /// Declare [`POLICY`] binding [`REVIEW`].
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    async fn declare(&self) {
        let dir = tempfile::tempdir().expect("checkout creates");
        for (name, contents) in [("pr-review.yaml", REVIEW), ("policy.yaml", POLICY)] {
            std::fs::write(dir.path().join(name), contents).expect("card file writes");
        }
        let input = build_declaration_input(load(&dir.path().join("policy.yaml")).expect("loads"))
            .expect("declaration input builds");
        let body = serde_json::to_value(input.to_declare_card_request()).expect("serializes");
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/cards")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", "pull-request-extraction-policy")
            .body(Body::from(body.to_string()))
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
            .await
            .expect("declaration responds");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    /// Serve `acme/policy` with `main` at commit `c…` (tree `7…`).
    fn serve_main(&self) {
        let files = [("src/user.ts", USER), ("src/handler.ts", HANDLER)];
        self.json(
            "",
            &json!({"id": REPOSITORY, "full_name": "acme/policy", "default_branch": "main", "private": true}),
        );
        self.json(
            "/pulls?state=closed&base=main&sort=updated&direction=desc&per_page=100&page=1",
            &json!([]),
        );
        self.json(
            "/branches/main",
            &json!({"name": "main", "commit": {"sha": sha('c'), "commit": {
                "tree": {"sha": sha('7')}, "committer": {"date": "2026-02-01T12:00:00Z"}}}}),
        );
        self.commit(&sha('c'), &sha('7'), &files);
        self.mock.serve_repository(
            REPOSITORY,
            &format!("/tarball/{}", sha('c')),
            StatusCode::OK,
            tarball(&files),
        );
    }

    /// Serve `commit` with root `tree` holding the user module and
    /// `handler`, its recursive listing and every blob.
    fn commit(&self, commit: &str, tree: &str, files: &[(&str, &[u8])]) {
        self.json(
            &format!("/git/commits/{commit}"),
            &json!({"sha": commit, "tree": {"sha": tree}, "committer": {"date": "2026-03-01T00:00:00Z"}}),
        );
        let entries: Vec<_> = files
            .iter()
            .map(|(path, bytes)| {
                self.mock.serve_repository(
                    REPOSITORY,
                    &format!("/git/blobs/{}", blob(bytes)),
                    StatusCode::OK,
                    bytes.to_vec(),
                );
                json!({"path": path, "mode": "100644", "type": "blob", "sha": blob(bytes), "size": bytes.len()})
            })
            .collect();
        self.json(
            &format!("/git/trees/{tree}?recursive=1"),
            &json!({"sha": tree, "truncated": false, "tree": entries}),
        );
    }

    /// Serve head `head` (tree `tree`) whose handler is `handler`.
    fn head(&self, head: &str, tree: &str, handler: &[u8]) {
        self.commit(
            head,
            tree,
            &[("src/user.ts", USER), ("src/handler.ts", handler)],
        );
    }

    /// Serve the comparison `base...head`, `head` ahead of `merge_base`,
    /// modifying the handler to `handler` with `patch`.
    fn compare(&self, base: &str, head: &str, merge_base: &str, handler: &[u8], patch: &str) {
        self.json(
            &format!("/compare/{base}...{head}"),
            &json!({"status": "ahead", "base_commit": {"sha": base},
                    "merge_base_commit": {"sha": merge_base},
                    "commits": [{"sha": head}],
                    "files": [{"filename": "src/handler.ts", "status": "modified",
                               "sha": blob(handler), "patch": patch}]}),
        );
    }

    /// Serve `value` at `path` below the repository.
    fn json(&self, path: &str, value: &Value) {
        self.mock.serve_repository(
            REPOSITORY,
            path,
            StatusCode::OK,
            value.to_string().into_bytes(),
        );
    }

    /// Point PR [`PR`] at `head`, deliver `action`, and return the
    /// admitted execution once its result is visible.
    ///
    /// # Panics
    /// Panics when the delivery is refused or no result appears in a minute.
    async fn review(&self, head: &str, action: &str) -> Uuid {
        match action {
            "opened" => {
                self.mock.set_pull_request(
                    REPOSITORY,
                    PR,
                    PullRequest::open(REPOSITORY, &sha('c'), head),
                );
            }
            _ => self
                .mock
                .edit_pull_request(REPOSITORY, PR, |pull| head.clone_into(&mut pull.head_sha)),
        }
        let payload = self
            .mock
            .pull_request_event(INSTALLATION, REPOSITORY, PR, action);
        let (status, _) = self
            .mock
            .deliver_new(
                self.server.base_url().expect("bound"),
                "pull_request",
                &payload,
            )
            .await
            .expect("delivery sends");
        assert_eq!(status, StatusCode::ACCEPTED);
        let mut conn = self
            .server
            .tenant_conn_for(self.server.data_tenant_id())
            .await
            .expect("tenant connection opens");
        let execution: Uuid = sqlx::query_scalar(
            "SELECT execution_id FROM wyrd.verifier_executions
              WHERE origin = 'pull_request' AND pull_request->'head'->>'sha' = $1",
        )
        .bind(head)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("the head was admitted");
        conn.commit().await.expect("read commits");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
        while self
            .get(&format!("/v1/verification/executions/{execution}"))
            .await["result"]
            .is_null()
        {
            assert!(
                tokio::time::Instant::now() < deadline,
                "execution {execution} never published its result"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        execution
    }

    /// The extraction step of `execution`'s retained details.
    ///
    /// # Panics
    /// Panics when the details are not served or hold no `pr` step.
    async fn extraction(&self, execution: Uuid) -> Value {
        let details = self
            .get(&format!("/v1/verification/executions/{execution}/details"))
            .await;
        let step = &details["outcome"]["steps"][0];
        assert_eq!(
            (&step["id"], &step["status"]),
            (&json!("pr"), &json!("succeeded")),
            "{details}"
        );
        step["output"].clone()
    }

    /// `GET uri` as the editor.
    ///
    /// # Panics
    /// Panics when the read is not `200 OK` JSON.
    async fn get(&self, uri: &str) -> Value {
        let request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .expect("request builds");
        let response = self
            .server
            .oneshot_authenticated(&self.jwt, request)
            .await
            .expect("read responds");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 22)
            .await
            .expect("body reads");
        let body: Value = serde_json::from_slice(&bytes).expect("JSON");
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        body
    }
}

/// The first review of a pull request extracts its pinned source in full
/// with `first_review`; the next head reviews only what changed since the
/// completed first review, naming it as `previous`, while `full_pr` keeps
/// the whole pull request.
///
/// # Panics
/// Panics when an extraction differs.
#[tokio::test(flavor = "multi_thread")]
async fn successive_heads_extract_the_full_then_the_incremental_change() {
    let deployment = Deployment::start().await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);

    let first = deployment.review(&sha('a'), "opened").await;
    let extraction = deployment.extraction(first).await;
    assert_eq!(extraction["format"], "pull_request", "{extraction}");
    assert_eq!(extraction["pr"]["number"], PR);
    assert_eq!(extraction["pr"]["repository"]["repository_id"], "4242");
    assert_eq!(extraction["pr"]["repository"]["card"]["name"], "policy");
    assert_eq!(extraction["pr"]["base"]["sha"], sha('c'));
    assert_eq!(extraction["pr"]["head"]["sha"], sha('a'));
    assert_eq!(extraction["pr"]["merge_base"], sha('c'));
    assert_eq!(
        extraction["review"].as_object().map(|review| (
            &review["mode"],
            &review["fallback_reason"],
            &review["previous"]
        )),
        Some((&json!("full"), &json!("first_review"), &Value::Null))
    );
    assert_eq!(extraction["review"]["comparison_base"], sha('c'));
    let file = &extraction["changes"]["files"][0];
    assert_eq!(file["path"], "src/handler.ts");
    assert_eq!(file["status"], "modified");
    assert_eq!(
        file["after"],
        json!({"kind": "text", "blob": blob(HANDLER_A), "text": std::str::from_utf8(HANDLER_A).expect("utf-8")})
    );
    assert_eq!(
        file["before"],
        json!({"kind": "text", "blob": blob(HANDLER), "text": std::str::from_utf8(HANDLER).expect("utf-8")})
    );
    assert_eq!(extraction["review"]["full_pr"], extraction["changes"]);
    assert!(extraction["context"].is_object(), "{extraction}");

    deployment.head(&sha('b'), &sha('9'), HANDLER_B);
    deployment.compare(&sha('a'), &sha('b'), &sha('a'), HANDLER_B, PATCH_AB);
    deployment.compare(&sha('c'), &sha('b'), &sha('c'), HANDLER_B, PATCH_B);
    let second = deployment.review(&sha('b'), "synchronize").await;
    let extraction = deployment.extraction(second).await;
    assert_eq!(extraction["review"]["mode"], "incremental", "{extraction}");
    assert_eq!(extraction["review"]["fallback_reason"], Value::Null);
    assert_eq!(
        extraction["review"]["previous"],
        json!({"execution_id": first, "head": sha('a')})
    );
    assert_eq!(extraction["review"]["comparison_base"], sha('a'));
    assert_eq!(extraction["pr"]["merge_base"], sha('c'));
    assert_eq!(
        extraction["changes"]["files"][0]["after"]["text"],
        std::str::from_utf8(HANDLER_B).expect("utf-8")
    );
    assert_eq!(
        extraction["changes"]["files"][0]["before"]["text"],
        std::str::from_utf8(HANDLER_A).expect("utf-8")
    );
    assert_eq!(extraction["review"]["full_pr"]["base"], sha('c'));
    assert_eq!(
        extraction["review"]["full_pr"]["files"][0]["before"]["text"],
        std::str::from_utf8(HANDLER).expect("utf-8")
    );
}
