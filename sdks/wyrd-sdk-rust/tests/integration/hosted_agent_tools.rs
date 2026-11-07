//! Hosted Agent tools, end to end: `publish-desk`'s drafter writes and
//! revises files in its private `/work` scratch with `fs.write`, publishes a
//! file, a directory, a JSON value, and an explicit `null` under logical
//! names, and a downstream consumer reads exactly the accepted content
//! through its mounted inputs and the collection manifest; `scratch-writes`'
//! Agents only write and read their own scratch.
//!
//! The local upstream stands in for the model provider and scripts each
//! Agent by its prompt marker, the question under review, and the number of
//! assistant turns already in the conversation. The test then inspects what
//! the model received, the recorded tool calls, and the publications read
//! back through the public execution surface.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::Notify;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_ai::cards::CardRef;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Execution, ExecutionPageQuery, PublicationContent, PublicationKind, PublicationName,
    PublicationSelector, PublicationSummary, TaskId,
};
use wyrd_ai::verify::{Submission, VerificationExecutionStatus};
use wyrd_ai::{StepStatus, VerificationResult, WyrdClient};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::auth::TokenAudience;
use wyrd_testing::server::WyrdTestServer;

use crate::local_development::configure_gateway;
use crate::support::{Deployment, declare, pull};
use crate::verifier_building_blocks::{completion, grants, requirement, step, tool_call};

/// The stable code of a permission denial, carried by every denied tool
/// call's error.
const DENIED: &str = "WYRD_PERMISSION_403_DENIED_RBAC";

/// The stable code of a refused tool argument.
const INVALID: &str = "WYRD_SPEC_400_VALIDATION";

/// The report the drafter publishes; later scratch writes never change it.
const REPORT: &str = "# Final report\n";

/// The report the drafter's second attempt publishes after its first
/// attempt gets an undecodable model reply.
const RETRIED: &str = "# Retried report\n";

/// A file of 3,000 numbered lines, larger than one tool result.
fn big() -> String {
    (0..3_000)
        .map(|line| format!("line {line:04}: the quick brown fox jumps over the lazy dog\n"))
        .collect()
}

/// The JSON assessment the drafter publishes by value.
fn assessment() -> Value {
    json!({ "risk": "low", "score": 0.9, "notes": ["é", "two words"] })
}

/// The messages of the Chat Completions `request`.
fn messages(request: &Request) -> Vec<Value> {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    body["messages"].as_array().cloned().unwrap_or_default()
}

/// The text of every message in `messages`, joined.
fn text(messages: &[Value]) -> String {
    messages
        .iter()
        .filter_map(|message| message["content"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// How many assistant turns `messages` already holds.
fn turn(messages: &[Value]) -> usize {
    messages
        .iter()
        .filter(|message| message["role"] == "assistant")
        .count()
}

/// Every tool result in `messages`, by the provider call ID it answers,
/// parsed from its JSON content.
fn results(messages: &[Value]) -> BTreeMap<String, Value> {
    messages
        .iter()
        .filter(|message| message["role"] == "tool")
        .filter_map(|message| {
            let id = message["tool_call_id"].as_str()?.to_owned();
            let content = message["content"].as_str()?;
            Some((id, serde_json::from_str(content).unwrap_or(Value::Null)))
        })
        .collect()
}

/// The text of an `fs.read` result: its units' lines, joined by newlines.
fn lines(read: &Value) -> String {
    read["units"]
        .as_array()
        .map(|units| {
            units
                .iter()
                .filter_map(|unit| unit["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// An assistant turn calling each `(id, tool, arguments)` at once.
fn calls(calls: &[(&str, &str, Value)]) -> ResponseTemplate {
    let calls: Vec<Value> = calls
        .iter()
        .map(|(id, tool, arguments)| tool_call(id, tool, arguments))
        .collect();
    completion(json!({ "role": "assistant", "content": null, "tool_calls": calls }))
}

/// The final `{"done": true}` answer.
fn done() -> ResponseTemplate {
    completion(json!({ "role": "assistant", "content": r#"{"done":true}"# }))
}

/// The drafter's turns that write and publish all four publications: the
/// report revised once, a tree with a nested Unicode, spaced filename and an
/// empty directory, the JSON assessment, and an explicit `null`. Call IDs
/// carry `prefix`, so attempts are told apart.
fn publish_all(turn: usize, prefix: &str, report: &str) -> Option<ResponseTemplate> {
    let id = |name: &str| format!("{prefix}{name}");
    let write = |path: &str, content: &str| json!({ "path": path, "content": content });
    Some(match turn {
        0 => calls(&[(&id("w1"), "fs.write", write("/work/report.md", "draft v1"))]),
        1 => calls(&[
            (&id("w2"), "fs.write", write("/work/report.md", report)),
            (&id("w3"), "fs.write", write("/work/tree/a.md", "alpha")),
            (
                &id("w4"),
                "fs.write",
                write("/work/tree/nested dir/é b.md", "beta"),
            ),
            (
                &id("w5"),
                "code.run",
                json!({ "source": "import os\nos.makedirs('/work/tree/empty')" }),
            ),
        ]),
        2 => calls(&[
            (&id("r1"), "fs.read", json!({ "path": "/work/report.md" })),
            (
                &id("p-report"),
                "publish",
                json!({ "name": "report", "path": "/work/report.md" }),
            ),
            (
                &id("p-tree"),
                "publish",
                json!({ "name": "tree", "path": "/work/tree" }),
            ),
            (
                &id("p-assessment"),
                "publish",
                json!({ "name": "assessment", "value": assessment() }),
            ),
            (
                &id("p-nothing"),
                "publish",
                json!({ "name": "nothing", "value": null }),
            ),
        ]),
        _ => return None,
    })
}

/// The drafter after publishing everything in the `FULL-RUN` review: it
/// mutates scratch and tries every refused publication and write, then
/// finishes.
fn refusals(turn: usize) -> ResponseTemplate {
    if turn > 3 {
        return done();
    }
    calls(&[
        (
            "w6",
            "fs.write",
            json!({ "path": "/work/report.md", "content": "mutated after publishing" }),
        ),
        (
            "x-conflict",
            "publish",
            json!({ "name": "report", "value": 1 }),
        ),
        (
            "x-both",
            "publish",
            json!({ "name": "both", "path": "/work/report.md", "value": 1 }),
        ),
        ("x-neither", "publish", json!({ "name": "neither" })),
        (
            "x-name",
            "publish",
            json!({ "name": "bad name!", "value": 1 }),
        ),
        (
            "x-missing",
            "publish",
            json!({ "name": "ghost", "path": "/work/missing.md" }),
        ),
        (
            "x-inputs",
            "publish",
            json!({ "name": "foreign", "path": "/inputs/report.md" }),
        ),
        (
            "x-escape",
            "publish",
            json!({ "name": "escape", "path": "/work/../etc" }),
        ),
        (
            "x-forged",
            "publish",
            json!({ "name": "forged", "value": 1, "attempt": 9, "step_id": "consume" }),
        ),
        (
            "x-write-inputs",
            "fs.write",
            json!({ "path": "/inputs/x.md", "content": "x" }),
        ),
        (
            "x-write-escape",
            "fs.write",
            json!({ "path": "/work/../x.md", "content": "x" }),
        ),
    ])
}

/// What the scripted model remembers across requests.
#[derive(Default)]
struct Script {
    /// The `RETRY-RUN` drafter's first turns so far, so its two attempts
    /// carry distinct call IDs and only the first gets an undecodable reply.
    attempts: AtomicUsize,
    /// The `RESTART-RUN` consumer's first turns so far; only the first is
    /// held.
    held: AtomicUsize,
    /// Raised when a scripted Agent starts waiting on a held reply.
    waiting: Notify,
}

/// The scripted model: one reply per request, chosen by the Agent's prompt
/// marker, the review's marker, and the turn. The `CANCEL-RUN` drafter
/// publishes `early`, then raises `waiting` and holds its next reply, which
/// would publish `late`, long enough to be cancelled; the `RESTART-RUN`
/// consumer raises it and holds its first reply long enough for the server
/// to restart.
fn reply(request: &Request, script: &Script) -> ResponseTemplate {
    let messages = messages(request);
    let prompt = text(&messages);
    let turn = turn(&messages);
    if prompt.contains("DRAFT:") {
        if prompt.contains("LIMITS-RUN") {
            return match turn {
                0 => calls(&[
                    (
                        "l-big",
                        "fs.write",
                        json!({ "path": "/work/big.md", "content": big() }),
                    ),
                    (
                        "l-huge",
                        "publish",
                        json!({ "name": "huge", "value": "x".repeat(70_000) }),
                    ),
                    (
                        "l-a",
                        "fs.write",
                        json!({ "path": "/work/a.md", "content": "a" }),
                    ),
                    (
                        "l-b",
                        "fs.write",
                        json!({ "path": "/work/b.md", "content": "b" }),
                    ),
                ]),
                1 => calls(&[
                    ("l-read", "fs.read", json!({ "path": "/work/big.md" })),
                    (
                        "l-publish",
                        "publish",
                        json!({ "name": "big", "path": "/work/big.md" }),
                    ),
                ]),
                _ => done(),
            };
        }
        if prompt.contains("CANCEL-RUN") {
            return match turn {
                0 => calls(&[("early", "publish", json!({ "name": "early", "value": 1 }))]),
                1 => {
                    script.waiting.notify_one();
                    calls(&[("late", "publish", json!({ "name": "late", "value": 2 }))])
                        .set_delay(Duration::from_secs(20))
                }
                _ => done(),
            };
        }
        if prompt.contains("NO-PUBLISH") {
            return done();
        }
        if prompt.contains("RETRY-RUN") {
            if turn == 0 && script.attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                return calls(&[(
                    "a1-p-report",
                    "publish",
                    json!({ "name": "report", "value": "attempt 1" }),
                )]);
            }
            if results(&messages).contains_key("a1-p-report") {
                return ResponseTemplate::new(200).set_body_json(json!({ "garbled": true }));
            }
            if turn == 3 {
                let replayed = json!({ "name": "replayed", "value": "once" });
                return calls(&[
                    ("a2-replay", "publish", replayed.clone()),
                    ("a2-replay", "publish", replayed),
                ]);
            }
            return publish_all(turn, "a2-", RETRIED).unwrap_or_else(done);
        }
        if prompt.contains("RESTART-RUN") {
            return publish_all(turn, "", REPORT).unwrap_or_else(done);
        }
        return publish_all(turn, "", REPORT).unwrap_or_else(|| refusals(turn));
    }
    if prompt.contains("CONSUME:") {
        if prompt.contains("RESTART-RUN")
            && turn == 0
            && script.held.fetch_add(1, Ordering::SeqCst) == 0
        {
            script.waiting.notify_one();
            return done().set_delay(Duration::from_secs(15));
        }
        return match turn {
            0 => calls(&[
                (
                    "c-report",
                    "fs.read",
                    json!({ "path": "/inputs/report.md" }),
                ),
                ("c-tree", "fs.list", json!({ "path": "/inputs/tree" })),
                (
                    "c-assessment",
                    "fs.read",
                    json!({ "path": "/inputs/assessment.json" }),
                ),
                (
                    "c-nothing",
                    "fs.read",
                    json!({ "path": "/inputs/nothing.json" }),
                ),
                (
                    "c-manifest",
                    "fs.read",
                    json!({ "path": "/inputs/all/manifest.json" }),
                ),
            ]),
            1 => {
                let manifest: Value =
                    serde_json::from_str(&lines(&results(&messages)["c-manifest"]))
                        .unwrap_or_default();
                let entry = manifest["entries"]
                    .as_array()
                    .and_then(|entries| entries.iter().find(|entry| entry["name"] == "report"))
                    .and_then(|entry| entry["path"].as_str())
                    .unwrap_or("missing");
                calls(&[(
                    "c-entry",
                    "fs.read",
                    json!({ "path": format!("/inputs/all/{entry}") }),
                )])
            }
            _ => done(),
        };
    }
    if prompt.contains("SCRATCH:") {
        let write = |path: &str, content: &str| json!({ "path": path, "content": content });
        return match turn {
            0 => calls(&[
                (
                    "s-create",
                    "fs.write",
                    write("/work/notes/day 1/é.md", "first"),
                ),
                ("s-inputs", "fs.write", write("/inputs/x.md", "x")),
                ("s-escape", "fs.write", write("/work/../escape.md", "x")),
                ("s-dot", "fs.write", write("/work/./x.md", "x")),
                ("s-root", "fs.write", write("/work", "x")),
            ]),
            1 => calls(&[
                (
                    "s-replace",
                    "fs.write",
                    write("/work/notes/day 1/é.md", "second, longer"),
                ),
                (
                    "s-parent",
                    "fs.write",
                    write("/work/notes/day 1/é.md/child.md", "x"),
                ),
            ]),
            2 => calls(&[
                (
                    "s-read",
                    "fs.read",
                    json!({ "path": "/work/notes/day 1/é.md" }),
                ),
                ("s-list", "fs.list", json!({ "path": "/work" })),
            ]),
            _ => done(),
        };
    }
    if prompt.contains("PEEK:") {
        return match turn {
            0 => calls(&[
                ("k-list", "fs.list", json!({ "path": "/work" })),
                (
                    "k-write",
                    "fs.write",
                    json!({ "path": "/work/mine.md", "content": "mine" }),
                ),
            ]),
            _ => done(),
        };
    }
    ResponseTemplate::new(500)
}

/// The publish desk deployment: a verification-capable server whose gateway
/// deploys `gpt-4o` on the scripted upstream, with `publish-desk` registered
/// and hydrated; verifications run as a principal granted every tool.
struct PublishDesk {
    /// The deployment.
    deployment: Deployment,
    /// The scripted model upstream.
    upstream: MockServer,
    /// The Card bundle, which every state loads from.
    bundle: TempDir,
    /// The registered `publish-desk` Service.
    root: CardRef,
    /// The scripted model's memory.
    script: Arc<Script>,
    /// The every-tool principal's key.
    key: String,
    /// The hydrated Service, as a principal granted every tool.
    state: WyrdState,
}

impl PublishDesk {
    /// Start the scripted upstream and the server, deploy `openai` on the
    /// upstream, declare and pull `publish-desk`, and load it as a principal
    /// that may verify, invoke the gateway, and call every tool.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        let script = Arc::new(Script::default());
        let scripted = Arc::clone(&script);
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(move |request: &Request| reply(request, &scripted))
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(builder(&upstream)).await;
        configure_gateway(&deployment.admin()).await;
        let cards = deployment.cards().await;
        let desk = declare(&cards, "cards/hosted_agent_tools/publish-desk.yaml").await;
        let bundle = pull(&cards, &desk.root).await;
        let key = deployment
            .granted_key("every_tool", &grants(json!("all")))
            .await;
        let state =
            WyrdState::from_path_with(bundle.path().join("bundle"), deployment.client(&key))
                .expect("bundle loads offline");
        Self {
            deployment,
            upstream,
            bundle,
            root: desk.root,
            script,
            key,
            state,
        }
    }

    /// Restart the server over the same Postgres and storage, as a process
    /// restart does, and reload the every-tool principal's state against it.
    ///
    /// # Panics
    /// Panics when the server does not restart or the bundle does not load.
    async fn restart(self) -> Self {
        let deployment = self.deployment.restart(builder(&self.upstream)).await;
        let state = WyrdState::from_path_with(
            self.bundle.path().join("bundle"),
            deployment.client(&self.key),
        )
        .expect("bundle loads offline");
        Self {
            deployment,
            state,
            ..self
        }
    }

    /// The hydrated Service, loaded with its server calls fixed to `client`
    /// and Bifrost not started.
    ///
    /// # Panics
    /// Panics when the bundle does not load.
    fn state_as(&self, client: WyrdClient) -> WyrdState {
        WyrdState::from_path_with(self.bundle.path().join("bundle"), client)
            .expect("bundle loads offline")
    }

    /// Verify `question` with `verifier` now, as the every-tool principal.
    ///
    /// # Panics
    /// Panics when the verify is refused.
    async fn verify(&self, verifier: &str, question: &str) -> VerificationResult {
        self.verify_as(&self.state, verifier, question).await
    }

    /// Verify `question` with `verifier` now through `state`.
    ///
    /// # Panics
    /// Panics when the verify is refused.
    async fn verify_as(
        &self,
        state: &WyrdState,
        verifier: &str,
        question: &str,
    ) -> VerificationResult {
        state
            .run()
            .observe()
            .verify(verifier, json!(question))
            .await
            .expect("the verification runs")
    }

    /// Every Chat Completions request the upstream received whose messages
    /// carry `marker`, in arrival order.
    ///
    /// # Panics
    /// Panics when the upstream does not record requests.
    async fn requests(&self, marker: &str) -> Vec<Request> {
        self.upstream
            .received_requests()
            .await
            .expect("the upstream records requests")
            .into_iter()
            .filter(|request| text(&messages(request)).contains(marker))
            .collect()
    }

    /// Every tool result the Agent prompted with `marker` returned, by call
    /// ID, read from the last request it sent.
    ///
    /// # Panics
    /// Panics when the Agent sent no request.
    async fn results(&self, marker: &str) -> BTreeMap<String, Value> {
        let requests = self.requests(marker).await;
        results(&messages(
            requests.last().expect("the Agent called the model"),
        ))
    }

    /// Every publication of `result`'s execution, read back as the
    /// deployment's admin through the public paged listing.
    ///
    /// # Panics
    /// Panics when the execution or a page cannot be read.
    async fn publications(&self, result: &VerificationResult) -> Vec<PublicationSummary> {
        let execution = self.execution(result).await;
        let mut publications = Vec::new();
        let mut cursor = None;
        loop {
            let page = execution
                .publications(&ExecutionPageQuery {
                    limit: Some(2),
                    cursor,
                })
                .await
                .expect("a publication page reads");
            publications.extend(page.items);
            cursor = page.next_cursor;
            if cursor.is_none() {
                return publications;
            }
        }
    }

    /// `result`'s execution as the deployment's admin.
    ///
    /// # Panics
    /// Panics when the handle cannot connect.
    async fn execution(&self, result: &VerificationResult) -> Execution {
        Execution::connect_with(result.execution_id, &self.deployment.admin())
            .await
            .expect("the execution handle connects")
    }
}

/// A verification-capable server whose gateway deploys on `upstream`.
fn builder(upstream: &MockServer) -> wyrd_testing::server::WyrdTestServerBuilder {
    WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .with_gateway_provider_root_for_test(upstream.uri().parse().expect("upstream URL parses"))
}

/// The function names the model was offered in `request`, in offer order.
fn offered(request: &Request) -> Vec<String> {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The stable code and repair rule of a refused tool result.
fn refusal(result: &Value) -> (&str, &str) {
    (
        result["code"].as_str().unwrap_or_default(),
        result["details"]["rule"].as_str().unwrap_or_default(),
    )
}

/// The publication selector of step attempt `attempt`, at `path` when given.
fn selector(attempt: u32, path: Option<&str>) -> PublicationSelector {
    PublicationSelector {
        attempt,
        item_index: None,
        iteration_index: None,
        path: path.map(str::to_owned),
    }
}

/// The base64 SHA-256 a download plan reports for `bytes`.
fn digest(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(Sha256Hex::digest(bytes).as_bytes())
}

/// The scribe writes, replaces, and reads back a nested Unicode file in its
/// private scratch without any declared input; writes to `/inputs`, through
/// traversal, at `/work` itself, or below a file are refused with their
/// typed rule and change nothing; the peeker, a later step, sees none of the
/// scribe's files and writes its own.
///
/// # Panics
/// Panics when the verification is refused or a write, read, refusal, or
/// listing differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_writes_private_workspace_files() {
    let desk = PublishDesk::start().await;

    let result = desk.verify("scratch-writes", "SCRATCH-RUN").await;

    assert_eq!(
        requirement(&result, "written").outcome,
        wyrd_ai::RequirementOutcome::Passed,
        "{result:?}"
    );
    let scribe = desk.results("SCRATCH:").await;
    assert_eq!(
        scribe["s-create"],
        json!({ "path": "/work/notes/day 1/é.md", "bytes": 5 })
    );
    assert_eq!(
        scribe["s-replace"],
        json!({ "path": "/work/notes/day 1/é.md", "bytes": 14 })
    );
    assert_eq!(lines(&scribe["s-read"]), "second, longer");
    let listed: Vec<&str> = scribe["s-list"]["files"]
        .as_array()
        .expect("the scratch lists")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert_eq!(listed, ["/work/notes/day 1/é.md"], "nothing refused landed");
    for (call, rule) in [
        ("s-inputs", "write_path"),
        ("s-escape", "write_path"),
        ("s-dot", "write_path"),
        ("s-root", "write_path"),
        ("s-parent", "write_parent"),
    ] {
        assert_eq!(refusal(&scribe[call]), (INVALID, rule), "{call}");
    }
    let peeker = desk.results("PEEK:").await;
    assert_eq!(
        peeker["k-list"]["files"],
        json!([]),
        "a later step never sees another step's scratch"
    );
    assert_eq!(
        peeker["k-write"],
        json!({ "path": "/work/mine.md", "bytes": 4 })
    );
    let write = step(&result, "write");
    let mut recorded: Vec<(Value, bool)> = write
        .tool_calls
        .iter()
        .map(|call| {
            (
                serde_json::to_value(call.tool).expect("the tool serializes"),
                call.error.is_none(),
            )
        })
        .collect();
    recorded.sort_by_key(|(tool, ok)| (tool.to_string(), *ok));
    assert_eq!(
        recorded,
        [
            (json!("fs.list"), true),
            (json!("fs.read"), true),
            (json!("fs.write"), false),
            (json!("fs.write"), false),
            (json!("fs.write"), false),
            (json!("fs.write"), false),
            (json!("fs.write"), false),
            (json!("fs.write"), true),
            (json!("fs.write"), true),
        ],
        "every call is recorded under its dotted built-in name"
    );
    desk.deployment.shutdown().await;
}

/// The drafter revises a report in scratch and publishes it, a tree with a
/// nested Unicode, spaced filename and an empty directory, a JSON
/// assessment, and an explicit `null`; each receipt is exactly
/// `{name, kind}`. Its later scratch mutation, a same-name republish, both
/// or neither source, an invalid name, a missing, `/inputs`, or escaping
/// path, and a forged producer field are refused and publish nothing. The
/// consumer reads exactly the published bytes through its mounts and finds
/// the report through the collection manifest's actual path; the public
/// readback returns the same content with matching download digests.
///
/// # Panics
/// Panics when the verification is refused or a receipt, refusal, mounted
/// read, or readback differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_publishes_file_directory_and_json() {
    let desk = PublishDesk::start().await;

    let result = desk.verify("report-pipeline", "FULL-RUN").await;

    assert_eq!(
        requirement(&result, "consumed").outcome,
        wyrd_ai::RequirementOutcome::Passed,
        "{result:?}"
    );
    let mut drafted = offered(&desk.requests("DRAFT:").await[0]);
    drafted.sort();
    assert_eq!(drafted, ["code_run", "fs_read", "fs_write", "publish"]);
    let mut consumed = offered(&desk.requests("CONSUME:").await[0]);
    consumed.sort();
    assert_eq!(
        consumed,
        ["fs_list", "fs_read"],
        "the consumer is offered no write tools"
    );
    let drafter = desk.results("DRAFT:").await;
    for (call, name, kind) in [
        ("p-report", "report", "file"),
        ("p-tree", "tree", "directory"),
        ("p-assessment", "assessment", "json"),
        ("p-nothing", "nothing", "json"),
    ] {
        assert_eq!(
            drafter[call],
            json!({ "name": name, "kind": kind }),
            "{call}"
        );
    }
    assert_eq!(lines(&drafter["r1"]), REPORT.trim_end());
    assert_eq!(
        drafter["w6"],
        json!({ "path": "/work/report.md", "bytes": 24 })
    );
    for (call, rule) in [
        ("x-conflict", "publication_name"),
        ("x-both", "publication_source"),
        ("x-neither", "publication_source"),
        ("x-missing", "workspace_missing"),
        ("x-inputs", "publication_path"),
        ("x-escape", "publication_path"),
        ("x-write-inputs", "write_path"),
        ("x-write-escape", "write_path"),
    ] {
        assert_eq!(refusal(&drafter[call]), (INVALID, rule), "{call}");
    }
    assert_eq!(
        (
            drafter["x-name"]["code"].as_str(),
            &drafter["x-name"]["details"]["field"]
        ),
        (Some(INVALID), &json!("name"))
    );
    assert_eq!(
        drafter["x-forged"]["code"], "SKALD_TOOL_422_INPUT",
        "the host owns the producer identity"
    );

    let consumer = desk.results("CONSUME:").await;
    assert_eq!(
        lines(&consumer["c-report"]),
        REPORT.trim_end(),
        "scratch changes after publishing never reach the publication"
    );
    assert_eq!(lines(&consumer["c-entry"]), REPORT.trim_end());
    let tree: Vec<&str> = consumer["c-tree"]["files"]
        .as_array()
        .expect("the tree lists")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert_eq!(
        tree,
        ["/inputs/tree/a.md", "/inputs/tree/nested dir/é b.md"]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&lines(&consumer["c-assessment"])).ok(),
        Some(assessment())
    );
    assert_eq!(lines(&consumer["c-nothing"]), "null");
    let manifest: Value = serde_json::from_str(&lines(&consumer["c-manifest"]))
        .expect("the collection manifest is JSON");
    let mut entries: Vec<(&str, &str)> = manifest["entries"]
        .as_array()
        .expect("the manifest lists entries")
        .iter()
        .map(|entry| {
            (
                entry["name"].as_str().unwrap_or_default(),
                entry["kind"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    entries.sort_unstable();
    assert_eq!(
        entries,
        [
            ("assessment", "json"),
            ("nothing", "json"),
            ("report", "file"),
            ("tree", "directory"),
        ],
        "publishes in one turn run concurrently, in acceptance order"
    );

    let publications = desk.publications(&result).await;
    let mut listed: Vec<(&str, &str, u32, PublicationKind)> = publications
        .iter()
        .map(|publication| {
            (
                publication.producer.step_id.as_str(),
                publication.name.as_str(),
                publication.producer.attempt,
                publication.kind,
            )
        })
        .collect();
    listed.sort_by_key(|(_, name, _, _)| *name);
    assert_eq!(
        listed,
        [
            ("draft", "assessment", 1, PublicationKind::Json),
            ("draft", "nothing", 1, PublicationKind::Json),
            ("draft", "report", 1, PublicationKind::File),
            ("draft", "tree", 1, PublicationKind::Directory),
        ],
        "refused calls and the final answer publish nothing"
    );
    let execution = desk.execution(&result).await;
    let draft = TaskId::new("draft").expect("step");
    let name = |name: &str| PublicationName::new(name).expect("name");
    let read = |publication: &str| {
        let execution = &execution;
        let draft = &draft;
        let publication = name(publication);
        async move {
            execution
                .publication(draft, &publication, &selector(1, None))
                .await
                .expect("the publication reads")
                .content
        }
    };
    let PublicationContent::Directory { manifest } = read("tree").await else {
        panic!("the tree is a directory publication");
    };
    let paths: Vec<&str> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, ["a.md", "nested dir/é b.md"]);
    assert_eq!(manifest.directories, ["empty", "nested dir"]);
    assert_eq!(
        read("assessment").await,
        PublicationContent::Json {
            value: assessment()
        }
    );
    assert_eq!(
        read("nothing").await,
        PublicationContent::Json { value: Value::Null }
    );
    let plan = execution
        .download(&draft, &name("report"), &selector(1, None))
        .await
        .expect("the report download plans");
    assert_eq!(
        (plan.size_bytes, plan.sha256),
        (REPORT.len() as u64, digest(REPORT.as_bytes()))
    );
    let plan = execution
        .download(
            &draft,
            &name("tree"),
            &selector(1, Some("nested dir/é b.md")),
        )
        .await
        .expect("the tree file download plans");
    assert_eq!((plan.size_bytes, plan.sha256), (4, digest(b"beta")));
    desk.deployment.shutdown().await;
}

/// Every result `messages` holds for provider call `id`, in order.
fn answers(messages: &[Value], id: &str) -> Vec<Value> {
    messages
        .iter()
        .filter(|message| message["role"] == "tool" && message["tool_call_id"] == id)
        .filter_map(|message| serde_json::from_str(message["content"].as_str()?).ok())
        .collect()
}

/// The drafter's first attempt publishes `report` and then gets an
/// undecodable model reply; the fresh second attempt reuses the name, and its publications,
/// not the first attempt's, feed the consumer, each attributed to the
/// attempt the host ran. Replaying one provider call ID returns the one
/// original receipt and accepts one publication. A drafter that only
/// answers publishes nothing, and its consumer fails before any model call.
///
/// # Panics
/// Panics when a verification is refused or an attempt, attribution,
/// receipt, or consumer outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_publish_retries_use_host_identity() {
    let desk = PublishDesk::start().await;

    let result = desk.verify("report-pipeline", "RETRY-RUN").await;

    assert_eq!(
        requirement(&result, "consumed").outcome,
        wyrd_ai::RequirementOutcome::Passed,
        "{result:?}"
    );
    assert_eq!(step(&result, "draft").attempts, 2);
    let consumer = desk.results("CONSUME:").await;
    assert_eq!(lines(&consumer["c-report"]), RETRIED.trim_end());
    let drafted = desk.requests("DRAFT:").await;
    let last = messages(drafted.last().expect("the drafter called the model"));
    assert_eq!(
        answers(&last, "a2-replay"),
        [
            json!({ "name": "replayed", "kind": "json" }),
            json!({ "name": "replayed", "kind": "json" }),
        ],
        "a replayed call returns its original receipt"
    );
    let mut listed: Vec<(u32, String, PublicationKind)> = desk
        .publications(&result)
        .await
        .into_iter()
        .map(|publication| {
            (
                publication.producer.attempt,
                publication.name.as_str().to_owned(),
                publication.kind,
            )
        })
        .collect();
    listed.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    let expected: Vec<(u32, String, PublicationKind)> = [
        (1, "report", PublicationKind::Json),
        (2, "assessment", PublicationKind::Json),
        (2, "nothing", PublicationKind::Json),
        (2, "replayed", PublicationKind::Json),
        (2, "report", PublicationKind::File),
        (2, "tree", PublicationKind::Directory),
    ]
    .into_iter()
    .map(|(attempt, name, kind)| (attempt, name.to_owned(), kind))
    .collect();
    assert_eq!(listed, expected, "each attempt keeps its own attribution");
    let execution = desk.execution(&result).await;
    let draft = TaskId::new("draft").expect("step");
    let report = PublicationName::new("report").expect("name");
    assert_eq!(
        execution
            .publication(&draft, &report, &selector(1, None))
            .await
            .expect("the first attempt's report reads")
            .content,
        PublicationContent::Json {
            value: json!("attempt 1")
        }
    );

    let result = desk.verify("report-pipeline", "NO-PUBLISH").await;

    assert_eq!(step(&result, "draft").status, StepStatus::Succeeded);
    let consume = step(&result, "consume");
    assert_eq!(consume.status, StepStatus::Failed, "{result:?}");
    assert!(
        consume
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("published no")),
        "{consume:?}"
    );
    assert!(
        desk.publications(&result).await.is_empty(),
        "a final answer is not a publication"
    );
    assert!(
        desk.requests("CONSUME:")
            .await
            .iter()
            .all(|request| !text(&messages(request)).contains("NO-PUBLISH")),
        "the consumer never reached the model"
    );
    desk.deployment.shutdown().await;
}

/// Every refused selection is refused at registration with the loader's
/// stable code and its `agent_tools` diagnostic, before any model call: an unknown tool, the
/// provider-facing spelling of a built-in, a repeated tool, and `publish` on
/// a requirement check's Agent. The selected built-ins keep their dotted
/// wire identity in the step's records and reach the model only under
/// their provider-facing names.
///
/// # Panics
/// Panics when a refused selection registers, its refusal differs, a model
/// call is made for it, or a selected tool's identity differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn selected_hosted_tools_resolve_before_model_calls() {
    let desk = PublishDesk::start().await;
    let cards = desk.deployment.cards().await;

    for (name, unknown) in [
        ("unknown-tool", "http.fetch is not a built-in tool"),
        ("provider-spelling", "fs_write is not a built-in tool"),
        ("duplicate-tool", "publish is selected more than once"),
        (
            "check-publishes",
            "publish is available only to step Agents",
        ),
    ] {
        let refused = cards
            .declare_from_path(crate::support::fixture(&format!(
                "cards/hosted_agent_tools/invalid/{name}.yaml"
            )))
            .await
            .expect_err("the selection is refused");
        assert_eq!(
            refused.code(),
            "WYRD_LOADER_400_INVALID_ENVELOPE",
            "{name}: {refused}"
        );
        let problem = format!("{refused:?}");
        assert!(problem.contains("agent_tools"), "{name}: {problem}");
        assert!(problem.contains(unknown), "{name}: {problem}");
    }
    assert!(
        desk.requests("NEVER CALLED").await.is_empty(),
        "no refused selection reached the model"
    );

    let result = desk.verify("report-pipeline", "FULL-RUN").await;
    let mut tools: Vec<Value> = step(&result, "draft")
        .tool_calls
        .iter()
        .map(|call| serde_json::to_value(call.tool).expect("the tool serializes"))
        .collect();
    tools.sort_by_key(Value::to_string);
    tools.dedup();
    assert_eq!(
        tools,
        [
            json!("code.run"),
            json!("fs.read"),
            json!("fs.write"),
            json!("publish")
        ]
    );
    let called: Vec<String> = desk
        .requests("DRAFT:")
        .await
        .iter()
        .flat_map(|request| {
            messages(request)
                .into_iter()
                .filter_map(|message| message["tool_calls"].as_array().cloned())
                .flatten()
                .filter_map(|call| call["function"]["name"].as_str().map(str::to_owned))
        })
        .collect();
    assert!(
        called.iter().all(|name| !name.contains('.')),
        "the model sees provider-facing names only: {called:?}"
    );
    desk.deployment.shutdown().await;
}

/// Assert `result`'s drafter called the `FULL-RUN` tools where only its
/// `fs.write` calls ran and every other call was denied with [`DENIED`] and
/// returned nothing, so it published nothing and its consumer failed before
/// any model call for `question`.
///
/// # Panics
/// Panics when a call, publication, or the consumer differs.
async fn assert_writes_only(desk: &PublishDesk, result: &VerificationResult, question: &str) {
    let draft = step(result, "draft");
    assert!(!draft.tool_calls.is_empty(), "{question}");
    for call in &draft.tool_calls {
        let tool = serde_json::to_value(call.tool).expect("the tool serializes");
        let denied = call
            .error
            .as_deref()
            .is_some_and(|error| error.contains(DENIED));
        if tool == "fs.write" {
            assert!(!denied, "{question}: {call:?}");
        } else {
            assert!(denied, "{question}: {call:?}");
            assert_eq!(
                (call.output_sha256.as_ref(), call.returned_bytes),
                (None, 0),
                "{question}: a denied call returns nothing"
            );
        }
    }
    let drafted = desk.requests(question).await;
    let denied = results(&messages(
        drafted.last().expect("the drafter called the model"),
    ));
    assert!(
        denied["p-assessment"].get("value").is_none()
            && !denied["p-assessment"].to_string().contains("low"),
        "{question}: a denied publish echoes no content"
    );
    assert!(desk.publications(result).await.is_empty(), "{question}");
    assert_eq!(
        step(result, "consume").status,
        StepStatus::Failed,
        "{question}"
    );
    assert!(
        desk.requests(question)
            .await
            .iter()
            .all(|request| !text(&messages(request)).contains("CONSUME:")),
        "{question}: the consumer never reached the model"
    );
}

/// Every hosted tool call is authorized on its own against the accepted
/// authority: a principal granted only `fs.write`, and a delegate granted
/// only `fs.write` acting for a principal granted every tool, both write
/// scratch while each read, code, and publish call is denied with nothing
/// returned or published. Cancelling a queued execution while its drafter
/// waits on the model keeps the publication accepted before the
/// cancellation and accepts nothing later.
///
/// # Panics
/// Panics when a principal does not seed, a verification or cancellation
/// is refused, or a call, publication, or status differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn tool_calls_recheck_permissions_and_revocation() {
    let desk = PublishDesk::start().await;
    let writes = grants(json!({ "tool": "fs.write" }));

    let key = desk.deployment.granted_key("writes_only", &writes).await;
    let state = desk.state_as(desk.deployment.client(&key));
    let result = desk
        .verify_as(&state, "report-pipeline", "FULL-RUN narrowed")
        .await;
    assert_writes_only(&desk, &result, "FULL-RUN narrowed").await;

    let subject = desk
        .deployment
        .granted("publish_subject", &grants(json!("all")))
        .await;
    let actor = desk.deployment.granted("publish_delegate", &writes).await;
    let token = desk
        .deployment
        .server()
        .exchange_api_key(subject.api_key().expect("the subject has a key"))
        .await
        .expect("the subject's token exchanges");
    let delegated = desk
        .deployment
        .rooted(
            actor
                .api_key()
                .expect("the delegate has a key")
                .expose_secret(),
            &desk.root,
        )
        .on_behalf_of(SecretString::from(token), TokenAudience::Wyrd)
        .await
        .expect("the delegate acts for the subject");
    let result = desk
        .verify_as(
            &desk.state_as(delegated),
            "report-pipeline",
            "FULL-RUN delegated",
        )
        .await;
    assert_writes_only(&desk, &result, "FULL-RUN delegated").await;

    let Submission::Queued(queued) = desk
        .state
        .run()
        .observe()
        .submit("report-pipeline", json!("CANCEL-RUN"))
        .await
        .expect("the review is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    desk.script.waiting.notified().await;
    let execution = Execution::connect_with(*queued.id(), &desk.deployment.admin())
        .await
        .expect("the execution handle connects");
    let cancelled = execution.cancel().await.expect("the execution cancels");
    assert_eq!(cancelled.status, VerificationExecutionStatus::Cancelled);
    let listed: Vec<(String, u32)> = execution
        .publications(&ExecutionPageQuery {
            limit: None,
            cursor: None,
        })
        .await
        .expect("the publications read")
        .items
        .into_iter()
        .map(|publication| {
            (
                publication.name.as_str().to_owned(),
                publication.producer.attempt,
            )
        })
        .collect();
    assert_eq!(
        listed,
        [("early".to_owned(), 1)],
        "cancellation keeps accepted work and accepts nothing later"
    );
    assert!(
        desk.requests("CANCEL-RUN")
            .await
            .iter()
            .all(|request| !text(&messages(request)).contains("CONSUME:")),
        "cancelled work starts no later Agent"
    );
    desk.deployment.shutdown().await;
}

/// Tool work stays bounded and its provenance exact: a JSON value over the
/// tool result limit is refused as `publication_size` and publishes
/// nothing, a file larger than one result reads back as one bounded page
/// that continues, the same file publishes whole, and concurrent scratch
/// writers all land. Each recorded call keeps its dotted tool, its
/// arguments, and the digest and size of exactly the result the model got,
/// and a refused call records its stable code with no output.
///
/// # Panics
/// Panics when the verification is refused or a bound, record,
/// publication, or download plan differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn tool_limits_release_capacity_and_preserve_provenance() {
    let desk = PublishDesk::start().await;

    let result = desk.verify("report-pipeline", "LIMITS-RUN").await;

    let drafter = desk.results("LIMITS-RUN").await;
    assert_eq!(refusal(&drafter["l-huge"]), (INVALID, "publication_size"));
    for call in ["l-a", "l-b"] {
        assert!(drafter[call]["bytes"] == 1, "{call}: {}", drafter[call]);
    }
    let read = &drafter["l-read"];
    let size = serde_json::to_vec(read).expect("the read serializes").len();
    assert!(
        size <= 64 * 1024,
        "one result stays within the limit: {size}"
    );
    assert!(lines(read).starts_with("line 0000: "), "{read}");
    assert!(
        read["next"]
            .as_u64()
            .is_some_and(|next| next > 0 && next < 3_000),
        "a long file continues on its next page: {}",
        read["next"]
    );
    let receipt = json!({ "name": "big", "kind": "file" });
    assert_eq!(drafter["l-publish"], receipt);

    let draft = step(&result, "draft");
    let publish = draft
        .tool_calls
        .iter()
        .find(|call| call.arguments["name"] == "big")
        .expect("the publish is recorded");
    let returned = serde_json::to_vec(&receipt).expect("the receipt serializes");
    assert_eq!(
        (
            serde_json::to_value(publish.tool).expect("the tool serializes"),
            &publish.arguments,
            publish.output_sha256,
            publish.returned_bytes,
            publish.error.as_deref(),
        ),
        (
            json!("publish"),
            &json!({ "name": "big", "path": "/work/big.md" }),
            Some(Sha256Hex::digest(&returned)),
            returned.len() as u64,
            None,
        )
    );
    let huge = draft
        .tool_calls
        .iter()
        .find(|call| call.arguments["name"] == "huge")
        .expect("the refused publish is recorded");
    assert_eq!((huge.output_sha256, huge.returned_bytes), (None, 0));
    assert!(
        huge.error
            .as_deref()
            .is_some_and(|error| error.starts_with(INVALID)),
        "{huge:?}"
    );
    let names: Vec<String> = desk
        .publications(&result)
        .await
        .into_iter()
        .map(|publication| publication.name.as_str().to_owned())
        .collect();
    assert_eq!(names, ["big"], "the refused value publishes nothing");
    let plan = desk
        .execution(&result)
        .await
        .download(
            &TaskId::new("draft").expect("step"),
            &PublicationName::new("big").expect("name"),
            &selector(1, None),
        )
        .await
        .expect("the file download plans");
    assert_eq!(
        (plan.size_bytes, plan.sha256),
        (big().len() as u64, digest(big().as_bytes()))
    );
    desk.deployment.shutdown().await;
}

/// A queued review takes the same tool and state path as a direct one, and
/// its accepted publications survive a process restart: the drafter
/// commits, the server restarts while the consumer waits on the model, and
/// the interrupted call spends the consumer's first attempt, and its retry
/// reads exactly the accepted content from the restored state without the
/// drafter running again.
///
/// # Panics
/// Panics when the submission, restart, or recovery fails, or the drafter
/// runs again, or a read or publication differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn publications_follow_shared_run_state() {
    let desk = PublishDesk::start().await;
    let Submission::Queued(queued) = desk
        .state
        .run()
        .observe()
        .submit("report-pipeline", json!("RESTART-RUN"))
        .await
        .expect("the review is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    let id = *queued.id();
    desk.script.waiting.notified().await;
    let drafted = desk.requests("DRAFT: ").await.len();
    desk.deployment
        .server()
        .flush_bifrost()
        .await
        .expect("Bifrost flushes");

    let desk = desk.restart().await;
    let server = desk.deployment.server();
    wyrd_testing::verification::VerificationFixture::provision(
        server.state(),
        server.data_tenant_id(),
    )
    .await
    .expect("the tenant provisions")
    .expire_deadlines(id)
    .await
    .expect("the stopped process's lease expires");
    let result = Execution::connect_with(id, &desk.deployment.admin())
        .await
        .expect("the execution handle connects")
        .wait(Duration::from_secs(90))
        .await
        .expect("the recovered execution settles");

    assert_eq!(
        requirement(&result, "consumed").outcome,
        wyrd_ai::RequirementOutcome::Passed,
        "{result:?}"
    );
    assert_eq!(
        desk.requests("DRAFT: ").await.len(),
        drafted,
        "the committed drafter never runs again"
    );
    assert_eq!(step(&result, "consume").attempts, 2);
    let consumer = desk.results("CONSUME:").await;
    assert_eq!(lines(&consumer["c-report"]), REPORT.trim_end());
    assert_eq!(lines(&consumer["c-nothing"]), "null");
    let mut names: Vec<String> = desk
        .publications(&result)
        .await
        .into_iter()
        .map(|publication| publication.name.as_str().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["assessment", "nothing", "report", "tree"]);
    desk.deployment.shutdown().await;
}
