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
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_loader::{build_declaration_input, load};
use wyrd_runtime::Permission;
use wyrd_testing::github::{GithubAppMock, PullRequest};
use wyrd_testing::server::ScribeReservation;
use wyrd_testing::{Bootstrap, WyrdTestServer};

use super::pg_github_pull_requests::install;

/// The connected repository.
const REPOSITORY: u64 = 4242;
/// The connected installation.
const INSTALLATION: u64 = 101;
/// The role invoking the gateway model.
const MODEL_ROLE: &str = "pr_review_model";
/// The role invoking Agent tools.
const TOOL_ROLE: &str = "pr_review_tools";
/// The reviewed pull request.
const PR: u64 = 7;
/// The Repository binding `pr-review` to its pull requests.
const POLICY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./pr-review.yaml\n      runs_on: {kind: pull_request}\n";
/// A queued Verifier extracting the pull request and requiring a change.
const REVIEW: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: pr-review\n  version: 1.0.0\nspec:\n  description: The pull request changes source.\n  steps:\n    - id: pr\n      action: {kind: extract, extractor: pull_request}\n      requirements:\n        - id: changes\n          statement: The reviewed change touches a file\n          check: {kind: assert, path: steps.pr.output.changes.files, operator: is_non_empty}\n";
/// [`REVIEW`] requiring complete library context instead.
const COMPLETE_REVIEW: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: pr-review\n  version: 1.0.0\nspec:\n  description: The pull request is reviewed with complete context.\n  steps:\n    - id: pr\n      action: {kind: extract, extractor: pull_request}\n      requirements:\n        - id: complete\n          statement: No related declaration was left out\n          check: {kind: assert, path: steps.pr.output.context.omissions, operator: is_empty}\n";
/// [`POLICY`] binding [`AGENT_REVIEW`] instead.
const AGENT_POLICY: &str = "apiVersion: wyrd/v1\nkind: Repository\nmetadata:\n  name: policy\n  version: 1.0.0\nspec:\n  repository_id: \"4242\"\n  verified_by:\n    - verifier: ./agent-review.yaml\n      runs_on: {kind: pull_request}\n";
/// A queued Verifier whose Agent reads the extraction twice: `pr`,
/// `changes` and `context` as Prompt values, and the whole output mounted
/// as `/inputs/pr.json`.
const AGENT_REVIEW: &str = "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: agent-review\n  version: 1.0.0\nspec:\n  description: An Agent reviews the pull request.\n  steps:\n    - id: pr\n      action: {kind: extract, extractor: pull_request}\n    - id: review\n      depends_on: [pr]\n      action:\n        kind: agent\n        agent: ./reviewer.yaml\n        inputs: {pr: steps.pr.output.pr, changes: steps.pr.output.changes, context: steps.pr.output.context}\n        workspace: {/inputs/pr.json: steps.pr.output}\n  requirements:\n    - id: reviewed\n      statement: The Agent answered\n      check: {kind: assert, path: steps.review.output, operator: is_non_empty}\n";
/// The reviewing Agent.
const REVIEWER: &str = "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: reviewer\n  version: 1.0.0\nspec:\n  execution_route: {kind: wyrd_gateway, deployment: openai}\n  tool_names: [fs.read]\n  run_config: {max_iterations: 3}\n  prompt:\n    provider: openai\n    model: gpt-test\n    messages:\n      - role: user\n        content: \"Review pull request {{pr}} changing {{changes}} in {{context}}; the whole extraction is /inputs/pr.json.\"\n";

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

/// The files of a tree holding every `(path, text)` of `helpers` and the
/// handler `handler`.
fn tree<'a>(helpers: &'a [(String, String)], handler: &'a str) -> Vec<(&'a str, &'a [u8])> {
    let mut files: Vec<(&str, &[u8])> = helpers
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_bytes()))
        .collect();
    files.push(("src/handler.ts", handler.as_bytes()));
    files
}

/// A connected, declared deployment whose GitHub serves `acme/policy`.
struct Deployment {
    /// GitHub.
    mock: GithubAppMock,
    /// Wyrd, running queued work.
    server: WyrdTestServer,
    /// The gateway's model provider.
    model: MockServer,
    /// The declaring editor's JWT.
    jwt: String,
}

impl Deployment {
    /// Start Wyrd with its Verifier runtime and a gateway model, connect
    /// the tenant through GitHub's install page, declare `cards` (whose
    /// `policy.yaml` is the Repository), and serve `main` at `c…`.
    ///
    /// # Panics
    /// Panics when a step fails.
    async fn start(cards: &[(&str, &str)]) -> Self {
        let mock = GithubAppMock::start().expect("the GitHub mock starts");
        mock.add_installation(INSTALLATION, &[(REPOSITORY, "acme/policy")]);
        mock.add_user("octo", &[INSTALLATION]);
        mock.sign_in("octo");
        mock.install_target(INSTALLATION);
        let model = MockServer::start().await;
        let server = WyrdTestServer::builder()
            .with_github_app_for_test(&mock)
            .with_verification_runtime_for_test()
            .with_gateway_provider_root_for_test(model.uri().parse().expect("model URL parses"))
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
        server
            .deploy_gateway_model_for_test("gpt-test", MODEL_ROLE)
            .await
            .expect("the model deploys");
        server
            .seed_role(TOOL_ROLE, &[Permission::tool_invoke_any()])
            .await
            .expect("the tool role seeds");
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("pull-request-editor", &["editor", MODEL_ROLE, TOOL_ROLE])
            .await
            .expect("editor bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        let deployment = Self {
            mock,
            server,
            model,
            jwt,
        };
        deployment
            .declare(cards, "pull-request-extraction-policy")
            .await;
        deployment.serve_main();
        deployment
    }

    /// Restart Wyrd over the same Postgres, Bifrost volume, GitHub and model,
    /// as a crashed process comes back, and sign the editor in again.
    ///
    /// # Panics
    /// Panics when the server does not restart or the editor cannot sign in.
    async fn restart(self) -> Self {
        let Self {
            mock,
            server,
            model,
            ..
        } = self;
        let server = Box::pin(
            server.restart_bound(
                WyrdTestServer::builder()
                    .with_github_app_for_test(&mock)
                    .with_verification_runtime_for_test()
                    .with_gateway_provider_root_for_test(
                        model.uri().parse().expect("model URL parses"),
                    ),
            ),
        )
        .await
        .expect("server restarts");
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user(
                "pull-request-editor-again",
                &["editor", MODEL_ROLE, TOOL_ROLE],
            )
            .await
            .expect("editor bootstraps")
        else {
            panic!("user bootstrap returned a non-user principal");
        };
        Self {
            mock,
            server,
            model,
            jwt,
        }
    }

    /// Declare `cards`, whose `policy.yaml` is the Repository, under
    /// idempotency `key`.
    ///
    /// # Panics
    /// Panics when the declaration is refused.
    async fn declare(&self, cards: &[(&str, &str)], key: &str) {
        let dir = tempfile::tempdir().expect("checkout creates");
        for (name, contents) in cards {
            std::fs::write(dir.path().join(name), contents).expect("card file writes");
        }
        let input = build_declaration_input(load(&dir.path().join("policy.yaml")).expect("loads"))
            .expect("declaration input builds");
        let body = serde_json::to_value(input.to_declare_card_request()).expect("serializes");
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/cards")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
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
        let execution = self.admit(head, action).await;
        self.settled(execution).await;
        execution
    }

    /// Point PR [`PR`] at `head`, deliver `action`, and return the admitted
    /// execution.
    ///
    /// # Panics
    /// Panics when the delivery is refused or admits nothing for `head`.
    async fn admit(&self, head: &str, action: &str) -> Uuid {
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
        execution
    }

    /// Wait until `execution`'s result is visible.
    ///
    /// # Panics
    /// Panics when no result appears in a minute.
    async fn settled(&self, execution: Uuid) {
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
    }

    /// Start with [`REVIEW`] bound and complete the first review of PR
    /// [`PR`] at head `a…`; returns the deployment and that execution.
    ///
    /// # Panics
    /// Panics when a step fails.
    async fn reviewed_at_a() -> (Self, Uuid) {
        let deployment = Self::start(&[("pr-review.yaml", REVIEW), ("policy.yaml", POLICY)]).await;
        deployment.head(&sha('a'), &sha('8'), HANDLER_A);
        deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);
        let first = deployment.review(&sha('a'), "opened").await;
        (deployment, first)
    }

    /// Serve head `b…` and the full comparison `c…...b…`.
    fn serve_b(&self) {
        self.head(&sha('b'), &sha('9'), HANDLER_B);
        self.compare(&sha('c'), &sha('b'), &sha('c'), HANDLER_B, PATCH_B);
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
    let deployment =
        Deployment::start(&[("pr-review.yaml", REVIEW), ("policy.yaml", POLICY)]).await;
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

/// A model answer calling `fs.read` on `/inputs/pr.json`, then a final
/// answer.
async fn reads_the_workspace(model: &MockServer) {
    let answer = |message: Value, finish: &str| {
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-review", "object": "chat.completion", "created": 1, "model": "gpt-test",
            "choices": [{"index": 0, "finish_reason": finish, "message": message}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        }))
    };
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(answer(
            json!({"role": "assistant", "content": null, "tool_calls": [{
                "id": "call_read", "type": "function",
                "function": {"name": "fs_read", "arguments": "{\"path\":\"/inputs/pr.json\"}"}}]}),
            "tool_calls",
        ))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(model)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(answer(
            json!({"role": "assistant", "content": "The handler trims the id."}),
            "stop",
        ))
        .mount(model)
        .await;
}

/// A normal Agent consumes one extraction twice: its Prompt receives the
/// pinned `pr`, active `changes` with the exact head source, and `context`,
/// and its workspace mounts the whole same output as JSON. GitHub is
/// compared once for both consumers.
///
/// # Panics
/// Panics when the Agent does not see the exact source in both, or the
/// pull request is acquired again.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_reads_the_extraction_as_prompt_values_and_workspace_json() {
    let deployment = Deployment::start(&[
        ("agent-review.yaml", AGENT_REVIEW),
        ("reviewer.yaml", REVIEWER),
        ("policy.yaml", AGENT_POLICY),
    ])
    .await;
    reads_the_workspace(&deployment.model).await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);

    let execution = deployment.review(&sha('a'), "opened").await;
    let extraction = deployment.extraction(execution).await;
    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    assert_eq!(details["outcome"]["verdict"], "passed", "{details}");

    let changed_line = "lookupUser(id.trim());";
    let requests = deployment
        .model
        .received_requests()
        .await
        .expect("requests are recorded");
    assert_eq!(requests.len(), 2, "one tool call, then the answer");
    let first: Value = serde_json::from_slice(&requests[0].body).expect("JSON");
    let prompt = first["messages"][0]["content"]
        .as_str()
        .expect("the Prompt renders to text");
    for bound in [changed_line, &blob(HANDLER_A), &sha('a'), "\"number\":7"] {
        assert!(prompt.contains(bound), "{bound} in the Prompt: {prompt}");
    }
    assert!(
        !prompt.contains("first_review"),
        "the unbound review section stays out of the Prompt: {prompt}"
    );
    let second: Value = serde_json::from_slice(&requests[1].body).expect("JSON");
    let read = second["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|message| message["role"] == "tool")
        .and_then(|message| message["content"].as_str())
        .expect("the tool result is returned to the model");
    let read: Value = serde_json::from_str(read).expect("fs.read answers JSON");
    assert!(read.get("next").is_none_or(Value::is_null), "{read}");
    let mounted: Vec<&str> = read["units"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|unit| unit["text"].as_str().expect("line text"))
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&mounted.join("\n")).expect("the mount is JSON"),
        extraction,
        "/inputs/pr.json is the whole extraction"
    );
    assert_eq!(extraction["review"]["fallback_reason"], "first_review");
    assert_eq!(
        deployment
            .mock
            .repository_reads(REPOSITORY)
            .iter()
            .filter(|read| read.starts_with("/compare/"))
            .count(),
        1,
        "both consumers share one acquisition"
    );
}

/// Assert `extraction` reviews the whole pull request from its merge base
/// `c…` for `reason`, with no previous review.
///
/// # Panics
/// Panics when the review is incremental, names another reason, or names a
/// previous review.
fn assert_full_review(extraction: &Value, reason: &str) {
    let review = &extraction["review"];
    assert_eq!(review["mode"], "full", "{extraction}");
    assert_eq!(review["fallback_reason"], reason, "{extraction}");
    assert_eq!(review["previous"], Value::Null, "{extraction}");
    assert_eq!(review["comparison_base"], sha('c'), "{extraction}");
}

/// A force-pushed head that no longer descends from the reviewed head is
/// reviewed in full with `history_rewritten`.
///
/// # Panics
/// Panics when the review is not full.
#[tokio::test(flavor = "multi_thread")]
async fn a_rewritten_history_reviews_the_whole_pull_request() {
    let (deployment, _) = Deployment::reviewed_at_a().await;
    deployment.serve_b();
    deployment.json(
        &format!("/compare/{}...{}", sha('a'), sha('b')),
        &json!({"status": "diverged", "base_commit": {"sha": sha('a')},
                "merge_base_commit": {"sha": sha('c')}, "commits": [], "files": []}),
    );

    let second = deployment.review(&sha('b'), "synchronize").await;

    let extraction = deployment.extraction(second).await;
    assert_full_review(&extraction, "history_rewritten");
    assert_eq!(extraction["changes"], extraction["review"]["full_pr"]);
}

/// A reviewed head GitHub can no longer find is reviewed in full with
/// `history_unavailable`.
///
/// # Panics
/// Panics when the review is not full.
#[tokio::test(flavor = "multi_thread")]
async fn an_unavailable_baseline_reviews_the_whole_pull_request() {
    let (deployment, _) = Deployment::reviewed_at_a().await;
    deployment.serve_b();

    let second = deployment.review(&sha('b'), "synchronize").await;

    let extraction = deployment.extraction(second).await;
    assert_full_review(&extraction, "history_unavailable");
}

/// After the Repository rebinds a new version of the Verifier, the next
/// head is reviewed in full with `policy_changed`.
///
/// # Panics
/// Panics when the review is not full.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_policy_reviews_the_whole_pull_request() {
    let (deployment, _) = Deployment::reviewed_at_a().await;
    deployment.serve_b();
    deployment.compare(&sha('a'), &sha('b'), &sha('a'), HANDLER_B, PATCH_AB);
    deployment
        .declare(
            &[
                ("pr-review.yaml", &REVIEW.replace("1.0.0", "1.0.1")),
                ("policy.yaml", &POLICY.replace("1.0.0", "1.0.1")),
            ],
            "pull-request-extraction-policy-v2",
        )
        .await;

    let second = deployment.review(&sha('b'), "synchronize").await;

    let extraction = deployment.extraction(second).await;
    assert_full_review(&extraction, "policy_changed");
}

/// A new head that changes nothing since the reviewed head is still
/// evidence: the extraction succeeds with no active change, and the
/// requirement that a file changed fails rather than passing.
///
/// # Panics
/// Panics when the empty change passes or is not extracted.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_incremental_change_is_evidence_and_never_a_pass() {
    let (deployment, _) = Deployment::reviewed_at_a().await;
    deployment.head(&sha('b'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('b'), &sha('c'), HANDLER_A, PATCH_A);
    deployment.json(
        &format!("/compare/{}...{}", sha('a'), sha('b')),
        &json!({"status": "ahead", "base_commit": {"sha": sha('a')},
                "merge_base_commit": {"sha": sha('a')},
                "commits": [{"sha": sha('b')}], "files": []}),
    );

    let second = deployment.review(&sha('b'), "synchronize").await;

    let extraction = deployment.extraction(second).await;
    assert_eq!(extraction["review"]["mode"], "incremental", "{extraction}");
    assert_eq!(extraction["changes"]["files"], json!([]));
    assert_eq!(
        extraction["review"]["full_pr"]["files"][0]["path"],
        "src/handler.ts"
    );
    let details = deployment
        .get(&format!("/v1/verification/executions/{second}/details"))
        .await;
    assert_eq!(details["outcome"]["verdict"], "failed", "{details}");
}

/// A review whose source could not be extracted never becomes the baseline:
/// after the completed review at `a…`, GitHub cannot serve head `d…`, so its
/// extraction step fails and the review is inconclusive; head `b…` is then
/// reviewed incrementally from `a…`.
///
/// # Panics
/// Panics when the unreadable head extracts, passes or becomes the baseline.
#[tokio::test(flavor = "multi_thread")]
async fn an_unextracted_review_never_becomes_the_baseline() {
    let (deployment, first) = Deployment::reviewed_at_a().await;

    let unreadable = deployment.review(&sha('d'), "synchronize").await;
    let details = deployment
        .get(&format!("/v1/verification/executions/{unreadable}/details"))
        .await;
    assert_eq!(details["outcome"]["steps"][0]["id"], "pr", "{details}");
    assert_eq!(
        details["outcome"]["steps"][0]["status"], "failed",
        "{details}"
    );
    assert_eq!(details["outcome"]["verdict"], "inconclusive", "{details}");

    deployment.serve_b();
    deployment.compare(&sha('a'), &sha('b'), &sha('a'), HANDLER_B, PATCH_AB);
    let second = deployment.review(&sha('b'), "synchronize").await;

    let extraction = deployment.extraction(second).await;
    assert_eq!(extraction["review"]["mode"], "incremental", "{extraction}");
    assert_eq!(
        extraction["review"]["previous"],
        json!({"execution_id": first, "head": sha('a')})
    );
}

/// A review that extracted its source is the next baseline whatever its
/// verdict: head `b…` changes nothing since `a…`, so its review fails, and
/// head `d…` is still reviewed incrementally from `b…`.
///
/// # Panics
/// Panics when the failed review is not the next baseline.
#[tokio::test(flavor = "multi_thread")]
async fn an_extracted_review_that_fails_is_the_next_baseline() {
    let (deployment, _) = Deployment::reviewed_at_a().await;
    deployment.head(&sha('b'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('b'), &sha('c'), HANDLER_A, PATCH_A);
    deployment.json(
        &format!("/compare/{}...{}", sha('a'), sha('b')),
        &json!({"status": "ahead", "base_commit": {"sha": sha('a')},
                "merge_base_commit": {"sha": sha('a')},
                "commits": [{"sha": sha('b')}], "files": []}),
    );
    let failed = deployment.review(&sha('b'), "synchronize").await;
    let details = deployment
        .get(&format!("/v1/verification/executions/{failed}/details"))
        .await;
    assert_eq!(details["outcome"]["verdict"], "failed", "{details}");

    deployment.head(&sha('d'), &sha('9'), HANDLER_B);
    deployment.compare(&sha('c'), &sha('d'), &sha('c'), HANDLER_B, PATCH_B);
    deployment.compare(&sha('b'), &sha('d'), &sha('b'), HANDLER_B, PATCH_AB);
    let next = deployment.review(&sha('d'), "synchronize").await;

    let extraction = deployment.extraction(next).await;
    assert_eq!(extraction["review"]["mode"], "incremental", "{extraction}");
    assert_eq!(
        extraction["review"]["previous"],
        json!({"execution_id": failed, "head": sha('b')})
    );
}

/// Verifying the pull request Verifier now refuses before any step: it runs
/// only as queued work for an admitted pull request, so no source is read.
///
/// # Panics
/// Panics when the direct request is accepted or reads GitHub.
#[tokio::test(flavor = "multi_thread")]
async fn direct_verification_of_a_pull_request_verifier_refuses_before_any_step() {
    let deployment =
        Deployment::start(&[("pr-review.yaml", REVIEW), ("policy.yaml", POLICY)]).await;
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let mut uid = async |kind: &str, name: &str| -> Uuid {
        sqlx::query_scalar("SELECT card_uid FROM wyrd.cards WHERE kind = $1 AND name = $2")
            .bind(kind)
            .bind(name)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("the Card is declared")
    };
    let verifier = uid("Verifier", "pr-review").await;
    let repository = uid("Repository", "policy").await;
    conn.commit().await.expect("read commits");
    let reads_before = deployment.mock.repository_reads(REPOSITORY).len();

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/verification/execute")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"verifier_card_uid": verifier, "card_uid": repository, "inputs": {}})
                .to_string(),
        ))
        .expect("request builds");
    let response = deployment
        .server
        .oneshot_authenticated(&deployment.jwt, request)
        .await
        .expect("execute responds");
    let status = response.status();
    let problem: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads"),
    )
    .expect("problem JSON");

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{problem}");
    assert_eq!(problem["code"], "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED");
    assert_eq!(
        problem["detail"],
        "this Verifier runs only as queued work; submit it instead of verifying now"
    );
    assert_eq!(
        problem["details"]["verifier_card_uid"],
        verifier.to_string()
    );
    assert_eq!(
        deployment.mock.repository_reads(REPOSITORY).len(),
        reads_before,
        "no source is read"
    );
}

/// Queued work that was not admitted from a pull request has no accepted
/// source, so the extractor fails its step before any acquisition and the
/// review is never a pass.
///
/// # Panics
/// Panics when the enqueue is refused, the step reads GitHub or succeeds, or
/// the execution passes.
#[tokio::test(flavor = "multi_thread")]
async fn queued_work_without_an_admitted_pull_request_extracts_nothing() {
    let deployment =
        Deployment::start(&[("pr-review.yaml", REVIEW), ("policy.yaml", POLICY)]).await;
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let mut uid = async |kind: &str, name: &str| -> Uuid {
        sqlx::query_scalar("SELECT card_uid FROM wyrd.cards WHERE kind = $1 AND name = $2")
            .bind(kind)
            .bind(name)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("the Card is declared")
    };
    let verifier = uid("Verifier", "pr-review").await;
    let repository = uid("Repository", "policy").await;
    conn.commit().await.expect("read commits");
    let reads_before = deployment.mock.repository_reads(REPOSITORY).len();

    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/verification/enqueue")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"verifier_card_uid": verifier, "card_uid": repository, "inputs": {}})
                .to_string(),
        ))
        .expect("request builds");
    let response = deployment
        .server
        .oneshot_authenticated(&deployment.jwt, request)
        .await
        .expect("enqueue responds");
    let status = response.status();
    let accepted: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 1 << 20)
            .await
            .expect("body reads"),
    )
    .expect("JSON");
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let execution: Uuid =
        serde_json::from_value(accepted["execution_id"].clone()).expect("an execution ID");
    deployment.settled(execution).await;

    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    let step = &details["outcome"]["steps"][0];
    assert_eq!(
        (&step["id"], &step["status"]),
        (&json!("pr"), &json!("failed")),
        "{details}"
    );
    assert_eq!(
        step["reason"],
        "the pull_request extractor reads only a queued pull request execution's accepted source",
        "{details}"
    );
    assert_eq!(details["outcome"]["verdict"], "inconclusive", "{details}");
    assert_eq!(
        deployment.mock.repository_reads(REPOSITORY).len(),
        reads_before,
        "no source is read"
    );
}

/// Execution history the extractor cannot read is a retryable failure, never
/// permission to review as if no prior review existed: the step spends its
/// retries, reads no comparison and the execution is never a baseline.
///
/// # Panics
/// Panics when the history read succeeds, the step is not retried, any
/// comparison is read, or the execution passes or becomes eligible.
#[tokio::test(flavor = "multi_thread")]
async fn unreadable_execution_history_is_retried_and_never_a_first_review() {
    let review = REVIEW.replace(
        "action: {kind: extract, extractor: pull_request}\n",
        "action: {kind: extract, extractor: pull_request}\n      retries: 1\n",
    );
    let deployment =
        Deployment::start(&[("pr-review.yaml", &review), ("policy.yaml", POLICY)]).await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);
    // Only the baseline query reads `pr_source_extracted`, so withholding
    // that one column from every runtime grantee makes execution history
    // unreadable without touching admission, claims or completion.
    let admin = deployment
        .server
        .pg_fixture()
        .superuser_pool()
        .expect("the superuser pool is available");
    sqlx::query(
        "DO $$
         DECLARE
             grantee text;
             visible text;
         BEGIN
             SELECT string_agg(quote_ident(column_name), ', ') INTO visible
               FROM information_schema.columns
              WHERE table_schema = 'wyrd' AND table_name = 'verifier_executions'
                AND column_name <> 'pr_source_extracted';
             FOR grantee IN
                 SELECT DISTINCT g.grantee FROM information_schema.role_table_grants g
                  WHERE g.table_schema = 'wyrd' AND g.table_name = 'verifier_executions'
                    AND g.privilege_type = 'SELECT'
                    AND g.grantee <> (SELECT tableowner FROM pg_tables
                                       WHERE schemaname = 'wyrd' AND tablename = 'verifier_executions')
             LOOP
                 EXECUTE format('REVOKE SELECT ON wyrd.verifier_executions FROM %I', grantee);
                 EXECUTE format('GRANT SELECT (%s) ON wyrd.verifier_executions TO %I', visible, grantee);
             END LOOP;
         END $$",
    )
    .execute(&admin)
    .await
    .expect("the history column is withheld");

    let execution = deployment.review(&sha('a'), "opened").await;

    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    let step = &details["outcome"]["steps"][0];
    assert_eq!(
        (&step["id"], &step["status"], &step["attempts"]),
        (&json!("pr"), &json!("failed"), &json!(2)),
        "{details}"
    );
    assert_eq!(
        step["reason"], "execution history is unavailable now",
        "{details}"
    );
    assert!(
        step["output"].is_null(),
        "no review is pretended: {details}"
    );
    assert_eq!(details["outcome"]["verdict"], "inconclusive", "{details}");
    assert_eq!(
        deployment
            .mock
            .repository_reads(REPOSITORY)
            .iter()
            .filter(|read| read.starts_with("/compare/"))
            .count(),
        0,
        "no source is acquired"
    );
    let extracted: bool = sqlx::query_scalar(
        "SELECT pr_source_extracted FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution)
    .fetch_one(&admin)
    .await
    .expect("the execution is recorded");
    assert!(!extracted, "an unextracted review is never a baseline");
}

/// Hold every slot of `server`'s reserved Scribe share, so nothing more can
/// be staged for persistence until the returned reservations drop.
///
/// Staged work is flushed between attempts, so slots that in-flight writes
/// free are taken as they return.
///
/// # Panics
/// Panics when Bifrost cannot flush or the share is not held within 30
/// seconds.
async fn hold_scribe(server: &WyrdTestServer) -> Vec<ScribeReservation> {
    let outbox = server.state().bifrost.scribe_outbox();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut share = Vec::new();
    loop {
        server.flush_bifrost().await.expect("staged work drains");
        let full = outbox
            .reserve(usize::MAX)
            .expect_err("no reservation holds every slot");
        if full.admitted == full.limit {
            return share;
        }
        share.push(
            outbox
                .reserve(full.limit - full.admitted)
                .expect("the free reservable share is held"),
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "Scribe slots stay held elsewhere"
        );
    }
}

/// An Agent consumes the live extraction while nothing can be persisted:
/// the extraction stalls on GitHub, Scribe's whole reserved share is held,
/// and once GitHub answers the Agent's model call already carries the
/// extracted source while the execution is still running. Releasing
/// persistence settles it with no second model call or acquisition.
///
/// # Panics
/// Panics when the Agent waits for persistence, the execution settles while
/// persistence is held, or settling repeats any work.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_consumes_the_live_extraction_while_persistence_is_held() {
    let deployment = Deployment::start(&[
        ("agent-review.yaml", AGENT_REVIEW),
        ("reviewer.yaml", REVIEWER),
        ("policy.yaml", AGENT_POLICY),
    ])
    .await;
    reads_the_workspace(&deployment.model).await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);
    let head = sha('a');
    let admission_read = deployment.mock.hold_next_pull_request_read();
    let (execution, extraction_read) = tokio::join!(deployment.admit(&head, "opened"), async {
        admission_read.arrived().await;
        let extraction_read = deployment.mock.hold_next_pull_request_read();
        admission_read.release();
        extraction_read
    });
    extraction_read.arrived().await;

    let share = hold_scribe(&deployment.server).await;
    extraction_read.release();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    while deployment
        .model
        .received_requests()
        .await
        .expect("requests are recorded")
        .len()
        < 2
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the Agent waited for persistence"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let requests = deployment
        .model
        .received_requests()
        .await
        .expect("requests are recorded");
    let prompt: Value = serde_json::from_slice(&requests[0].body).expect("JSON");
    let prompt = prompt["messages"][0]["content"]
        .as_str()
        .expect("the Prompt renders to text");
    assert!(
        prompt.contains("lookupUser(id.trim());"),
        "the Prompt carries the extracted head source: {prompt}"
    );
    let status = deployment
        .get(&format!("/v1/verification/executions/{execution}"))
        .await;
    assert_eq!(status["status"], "running", "{status}");
    assert_eq!(status["result"], Value::Null, "{status}");

    drop(share);
    deployment.settled(execution).await;
    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    assert_eq!(details["outcome"]["verdict"], "passed", "{details}");
    assert_eq!(
        deployment
            .model
            .received_requests()
            .await
            .expect("requests are recorded")
            .len(),
        2,
        "settling calls the model no more"
    );
    assert_eq!(
        deployment
            .mock
            .repository_reads(REPOSITORY)
            .iter()
            .filter(|read| read.starts_with("/compare/"))
            .count(),
        1,
        "settling acquires nothing again"
    );
}

/// Recovery reuses the accepted extraction exactly, even once the review
/// library's context records have expired: the Agent, allowed one retry,
/// stalls on its first
/// model call after the extraction's checkpoint is accepted, the library's
/// contexts expire, Wyrd restarts and the lost claim's lease expires. The
/// reclaimed execution gives the
/// Agent the identical Prompt without reading GitHub again, and its
/// completion records that the source was extracted.
///
/// # Panics
/// Panics when recovery reads GitHub, changes the extraction, or does not
/// record the extracted source.
#[tokio::test(flavor = "multi_thread")]
async fn recovery_reuses_the_accepted_extraction_after_library_contexts_expire() {
    let retrying = AGENT_REVIEW.replace(
        "      depends_on: [pr]\n",
        "      depends_on: [pr]\n      retries: 1\n",
    );
    let deployment = Deployment::start(&[
        ("agent-review.yaml", &retrying),
        ("reviewer.yaml", REVIEWER),
        ("policy.yaml", AGENT_POLICY),
    ])
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_delay(std::time::Duration::from_secs(600)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&deployment.model)
        .await;
    reads_the_workspace(&deployment.model).await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);
    let execution = deployment.admit(&sha('a'), "opened").await;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
    while deployment
        .model
        .received_requests()
        .await
        .expect("requests are recorded")
        .is_empty()
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the Agent never called its model"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    deployment
        .server
        .flush_bifrost()
        .await
        .expect("the extraction checkpoint is accepted");
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let expired = sqlx::query(
        "UPDATE wyrd.code_review_contexts SET expires_at = published_at + interval '1 microsecond'",
    )
    .execute(&mut **conn.transaction())
    .await
    .expect("contexts expire");
    conn.commit().await.expect("expiry commits");
    assert!(
        expired.rows_affected() > 0,
        "the library published a context"
    );
    let reads = deployment.mock.repository_reads(REPOSITORY);

    let deployment = deployment.restart().await;
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let expired = sqlx::query(
        "UPDATE wyrd.verifier_executions SET lease_expires_at = statement_timestamp()
          WHERE execution_id = $1 AND status = 'running'",
    )
    .bind(execution)
    .execute(&mut **conn.transaction())
    .await
    .expect("the lost lease expires");
    conn.commit().await.expect("expiry commits");
    assert_eq!(
        expired.rows_affected(),
        1,
        "the crashed claim was still running"
    );
    deployment.settled(execution).await;

    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    assert_eq!(details["outcome"]["verdict"], "passed", "{details}");
    assert_eq!(
        deployment.mock.repository_reads(REPOSITORY),
        reads,
        "recovery reads GitHub no more"
    );
    let requests = deployment
        .model
        .received_requests()
        .await
        .expect("requests are recorded");
    assert_eq!(
        requests.len(),
        3,
        "the stalled call, then the recovered Agent"
    );
    let recovered: Value = serde_json::from_slice(&requests[1].body).expect("JSON");
    let prompt = recovered["messages"][0]["content"]
        .as_str()
        .expect("the Prompt renders to text");
    for bound in ["lookupUser(id.trim());", &blob(HANDLER_A), &sha('a')] {
        assert!(
            prompt.contains(bound),
            "{bound} in the recovered Prompt: {prompt}"
        );
    }
    let answered: Value = serde_json::from_slice(&requests[2].body).expect("JSON");
    let read = answered["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|message| message["role"] == "tool")
        .and_then(|message| message["content"].as_str())
        .expect("the tool result is returned to the model");
    let read: Value = serde_json::from_str(read).expect("fs.read answers JSON");
    let mounted: Vec<&str> = read["units"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|unit| unit["text"].as_str().expect("line text"))
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&mounted.join("\n")).expect("the mount is JSON"),
        deployment.extraction(execution).await,
        "the recovered workspace mounts the retained extraction"
    );
    let mut conn = deployment
        .server
        .tenant_conn_for(deployment.server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    let extracted: bool = sqlx::query_scalar(
        "SELECT pr_source_extracted FROM wyrd.verifier_executions WHERE execution_id = $1",
    )
    .bind(execution)
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("the execution reads");
    conn.commit().await.expect("read commits");
    assert!(
        extracted,
        "the recovered completion records the extracted source"
    );
}

/// Cancelled work never becomes the baseline: P is the completed review at
/// `a…`; A, at head `d…`, stalls mid-extraction on GitHub when head `b…` is
/// pushed, which cancels A. B is reviewed incrementally from P, and A,
/// released afterwards, stays cancelled.
///
/// # Panics
/// Panics when A survives or B is not reviewed from P.
#[tokio::test(flavor = "multi_thread")]
async fn superseded_running_work_never_becomes_the_baseline() {
    let (deployment, first) = Deployment::reviewed_at_a().await;
    deployment.head(&sha('d'), &sha('9'), HANDLER_B);
    deployment.compare(&sha('a'), &sha('d'), &sha('a'), HANDLER_B, PATCH_AB);
    let head = sha('d');
    let admission_read = deployment.mock.hold_next_pull_request_read();
    let (running, extraction_read) = tokio::join!(deployment.admit(&head, "synchronize"), async {
        admission_read.arrived().await;
        let extraction_read = deployment.mock.hold_next_pull_request_read();
        admission_read.release();
        extraction_read
    });
    extraction_read.arrived().await;

    deployment.serve_b();
    deployment.compare(&sha('a'), &sha('b'), &sha('a'), HANDLER_B, PATCH_AB);
    let second = deployment.review(&sha('b'), "synchronize").await;
    extraction_read.release();

    let extraction = deployment.extraction(second).await;
    assert_eq!(extraction["review"]["mode"], "incremental", "{extraction}");
    assert_eq!(
        extraction["review"]["previous"],
        json!({"execution_id": first, "head": sha('a')})
    );
    let status = deployment
        .get(&format!("/v1/verification/executions/{running}"))
        .await;
    assert_eq!(status["status"], "cancelled", "{status}");
}

/// A changed file whose source GitHub cannot serve fails the extraction,
/// so the review is inconclusive and never passes.
///
/// # Panics
/// Panics when the extraction succeeds or the review passes.
#[tokio::test(flavor = "multi_thread")]
async fn missing_changed_source_fails_the_extraction_and_never_passes() {
    let deployment =
        Deployment::start(&[("pr-review.yaml", REVIEW), ("policy.yaml", POLICY)]).await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.mock.serve_repository(
        REPOSITORY,
        &format!("/git/blobs/{}", blob(HANDLER_A)),
        StatusCode::NOT_FOUND,
        Vec::new(),
    );
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);

    let execution = deployment.review(&sha('a'), "opened").await;

    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    assert_eq!(
        details["outcome"]["steps"][0]["status"], "failed",
        "{details}"
    );
    assert_eq!(details["outcome"]["verdict"], "inconclusive", "{details}");
}

/// Related declarations the library's context leaves out are recorded as
/// omissions, never hidden: a change calling twenty helpers in twenty files
/// selects fifteen and omits five at the file limit, so a Verifier requiring
/// complete context fails.
///
/// # Panics
/// Panics when the omissions are not recorded or the review passes.
#[tokio::test(flavor = "multi_thread")]
async fn omitted_context_is_recorded_and_fails_a_completeness_requirement() {
    let deployment =
        Deployment::start(&[("pr-review.yaml", COMPLETE_REVIEW), ("policy.yaml", POLICY)]).await;
    let imports = (0..20)
        .map(|i| format!("import {{ helper{i} }} from \"./h{i}\";"))
        .collect::<Vec<_>>()
        .join("\n");
    let calls = (0..20)
        .map(|i| format!("helper{i}(id);"))
        .collect::<Vec<_>>()
        .join(" ");
    let changed = format!("export const handle = (id: string) => {{ {calls} return id; }};");
    let main_handler = format!("{imports}\n\nexport const handle = (id: string) => id;\n");
    let head_handler = format!("{imports}\n\n{changed}\n");
    let helpers: Vec<(String, String)> = (0..20)
        .map(|i| {
            (
                format!("src/h{i}.ts"),
                format!("export function helper{i}(id: string): string {{\n  return id;\n}}\n"),
            )
        })
        .collect();
    let main = tree(&helpers, &main_handler);
    deployment.commit(&sha('c'), &sha('7'), &main);
    deployment.mock.serve_repository(
        REPOSITORY,
        &format!("/tarball/{}", sha('c')),
        StatusCode::OK,
        tarball(&main),
    );
    deployment.commit(&sha('a'), &sha('8'), &tree(&helpers, &head_handler));
    deployment.json(
        &format!("/compare/{}...{}", sha('c'), sha('a')),
        &json!({"status": "ahead", "base_commit": {"sha": sha('c')},
                "merge_base_commit": {"sha": sha('c')},
                "commits": [{"sha": sha('a')}],
                "files": [{"filename": "src/handler.ts", "status": "modified",
                           "sha": blob(head_handler.as_bytes()),
                           "patch": format!("@@ -22,1 +22,1 @@\n-export const handle = (id: string) => id;\n+{changed}")}]}),
    );

    let execution = deployment.review(&sha('a'), "opened").await;

    let context = &deployment.extraction(execution).await["context"];
    let omitted: Vec<(&str, &str)> = context["omissions"]
        .as_array()
        .expect("omissions")
        .iter()
        .map(|omission| {
            (
                omission["path"].as_str().expect("path"),
                omission["reason"].as_str().expect("reason"),
            )
        })
        .collect();
    assert_eq!(
        omitted,
        [
            ("src/h5.ts", "FileLimit"),
            ("src/h6.ts", "FileLimit"),
            ("src/h7.ts", "FileLimit"),
            ("src/h8.ts", "FileLimit"),
            ("src/h9.ts", "FileLimit"),
        ],
        "{context}"
    );
    assert_eq!(
        context["spans"].as_array().map(Vec::len),
        Some(15),
        "{context}"
    );
    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    assert_eq!(details["outcome"]["verdict"], "failed", "{details}");
}

/// An extraction larger than the model accepts fails the Agent honestly:
/// the provider refuses the input as over its context length, the review
/// step fails, and the review is never passed.
///
/// # Panics
/// Panics when the Agent step succeeds or the review passes.
#[tokio::test(flavor = "multi_thread")]
async fn a_model_refusing_the_input_size_never_passes_the_review() {
    let deployment = Deployment::start(&[
        ("agent-review.yaml", AGENT_REVIEW),
        ("reviewer.yaml", REVIEWER),
        ("policy.yaml", AGENT_POLICY),
    ])
    .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": {
            "message": "This model's maximum context length is 8192 tokens.",
            "type": "invalid_request_error", "param": "messages",
            "code": "context_length_exceeded"}})))
        .mount(&deployment.model)
        .await;
    deployment.head(&sha('a'), &sha('8'), HANDLER_A);
    deployment.compare(&sha('c'), &sha('a'), &sha('c'), HANDLER_A, PATCH_A);

    let execution = deployment.review(&sha('a'), "opened").await;

    let details = deployment
        .get(&format!("/v1/verification/executions/{execution}/details"))
        .await;
    let steps = &details["outcome"]["steps"];
    assert_eq!(steps[0]["status"], "succeeded", "{details}");
    assert_eq!(steps[1]["id"], "review", "{details}");
    assert_eq!(steps[1]["status"], "failed", "{details}");
    assert_ne!(details["outcome"]["verdict"], "passed", "{details}");
    assert!(
        !deployment
            .model
            .received_requests()
            .await
            .expect("requests are recorded")
            .is_empty(),
        "the model was asked and refused"
    );
}
