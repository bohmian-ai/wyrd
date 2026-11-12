//! Verifier orchestration, end to end: `orchestration-desk`'s Verifiers fan
//! identical reviewers out over one pinned question and consolidate their
//! settled publications, and select specialists by a planner's decisions,
//! consolidating every settled collection honestly.
//!
//! The local upstream stands in for the model provider and scripts each
//! Agent by its prompt marker and the number of assistant turns already in
//! its conversation; a reviewer numbers itself on its first turn and carries
//! that number in its call IDs, since a count fanout gives it no inputs that
//! tell items apart. The test then inspects what each conversation received
//! and the publications read back through the public execution surface.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_ai::cards::Cards;
use wyrd_ai::cli::{ExecutionArg, ExecutionVerb, PageArgs};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Execution, ExecutionInvocation, ExecutionPageQuery, File, Inputs, InvocationProgress,
    PublicationContent, PublicationName, PublicationSelector, PublicationSummary, Speculation,
    Submission, TaskId, VerificationExecutionStatus,
};
use wyrd_ai::{
    ExecutionId, RequirementOutcome, StepStatus, VerificationResult, VerificationVerdict,
    WyrdClient,
};
use wyrd_spec::verification::state::{ControlRecord, ExecutionState};
use wyrd_testing::gateway_capture::HeldUpstream;
use wyrd_testing::server::{ScribeReservation, WyrdTestServerBuilder};
use wyrd_testing::verification::VerificationFixture;

use crate::hosted_agent_tools::{
    builder, calls, done, lines, messages, results, text, turn, workspaces,
};
use crate::local_development::configure_gateway;
use crate::support::{Deployment, declare, fixture, pull};
use crate::verifier_building_blocks::{completion, grants, requirement, step};

/// The JSON body of `GET /v1/verification/executions/{path}`, read as
/// `desk`'s admin over HTTP.
///
/// # Panics
/// Panics when the token exchange or the read fails or the body is not JSON.
async fn http(path: &str, desk: &Desk) -> Value {
    let token = desk
        .deployment
        .admin()
        .access_token()
        .await
        .expect("the administrator token mints");
    reqwest::Client::new()
        .get(format!(
            "{}/v1/verification/executions/{path}",
            desk.deployment
                .server()
                .base_url()
                .expect("bound server has a URL")
        ))
        .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
        .send()
        .await
        .expect("the server answers")
        .error_for_status()
        .expect("the execution route reads")
        .json()
        .await
        .expect("the route answers JSON")
}

/// Run the CLI's execution `verb` in process as `desk`'s admin.
///
/// # Panics
/// Panics when the CLI call is refused.
async fn cli(verb: ExecutionVerb, desk: &Desk) -> Value {
    wyrd_ai::cli::execution(verb, Some(desk.deployment.admin()))
        .await
        .expect("the CLI call succeeds")
}

/// A final answer carrying `value`.
fn answer(value: &Value) -> ResponseTemplate {
    completion(json!({ "role": "assistant", "content": value.to_string() }))
}

/// How long a held section reply waits, so the sections holding the
/// workspace ceiling stay running while the next is refused.
const HELD: Duration = Duration::from_secs(3);

/// What the scripted model remembers across requests.
#[derive(Default)]
struct Script {
    /// Reviewer conversations started so far; each takes the next number.
    reviewers: AtomicUsize,
}

/// The reviewer number a conversation carries in its call IDs, once it has
/// taken one.
fn numbered(messages: &[Value]) -> Option<usize> {
    messages
        .iter()
        .filter_map(|message| message["tool_calls"].as_array())
        .flatten()
        .find_map(|call| {
            call["id"]
                .as_str()?
                .strip_prefix('r')?
                .split_once('-')?
                .0
                .parse()
                .ok()
        })
}

/// The scripted model: one reply per request, chosen by the Agent's prompt
/// marker and turn. Reviewer 0's first attempt gets an undecodable reply
/// after its first turn, so its item retries as a second attempt. The
/// planner decides by the question's marker and breaks without one; the
/// stylist publishes a partial draft, then breaks. The bootstrap, the
/// comparison, and the extra review answer at once; a speculative race
/// orders them by holding answers at a [`HeldUpstream`]. The refiner publishes its report, then scores it
/// by its iteration index, a constant, or a word, or breaks on iteration 1.
/// A section reviewer publishes its section and index; the escalation
/// publishes a note, then breaks; the summarizer reads both collections,
/// publishes a summary, and scores it by its iteration index; the document
/// judge passes, citing the document's title.
fn reply(request: &Request, script: &Script) -> ResponseTemplate {
    let messages = messages(request);
    let prompt = text(&messages);
    let turn = turn(&messages);
    if prompt.contains("REVIEW:") {
        let n =
            numbered(&messages).unwrap_or_else(|| script.reviewers.fetch_add(1, Ordering::SeqCst));
        let id = |name: &str| format!("r{n}-{name}");
        return match turn {
            0 => calls(&[
                (&id("list"), "fs.list", json!({ "path": "/work" })),
                (
                    &id("question"),
                    "fs.read",
                    json!({ "path": "/inputs/question.json" }),
                ),
            ]),
            1 if n == 0 => ResponseTemplate::new(200).set_body_json(json!({ "garbled": true })),
            1 => calls(&[(
                &id("write"),
                "fs.write",
                json!({ "path": "/work/notes.md", "content": format!("reviewer {n}") }),
            )]),
            2 => calls(&[
                (&id("read"), "fs.read", json!({ "path": "/work/notes.md" })),
                (
                    &id("publish"),
                    "publish",
                    json!({ "name": "account", "value": { "reviewer": n } }),
                ),
            ]),
            _ => done(),
        };
    }
    if prompt.contains("CONSOLIDATE:") {
        let read = |collection: &str| {
            (
                format!("c-{collection}"),
                "fs.read",
                json!({ "path": format!("/inputs/{collection}/manifest.json") }),
            )
        };
        let reads: Vec<_> = if prompt.contains("SWEEP-RUN") {
            vec![read("accounts")]
        } else if prompt.contains("SPEC-") {
            vec![read("extra")]
        } else {
            vec![read("general"), read("security"), read("style")]
        };
        return match turn {
            0 => calls(
                &reads
                    .iter()
                    .map(|(id, tool, arguments)| (id.as_str(), *tool, arguments.clone()))
                    .collect::<Vec<_>>(),
            ),
            _ => done(),
        };
    }
    if prompt.contains("PLAN:") {
        let decision = [
            ("NEITHER", json!({ "security": false, "style": false })),
            ("SECURITY-ONLY", json!({ "security": true, "style": false })),
            ("BOTH", json!({ "security": true, "style": true })),
            ("UNDECIDED", json!({ "security": null, "style": false })),
            ("FORGOTTEN", json!({ "style": false })),
            ("SILENT", json!({ "security": true, "style": false })),
        ]
        .into_iter()
        .find(|(marker, _)| prompt.contains(marker));
        return match decision {
            Some((_, decision)) => answer(&decision),
            None => ResponseTemplate::new(200).set_body_json(json!({ "garbled": true })),
        };
    }
    if prompt.contains("SECURITY:") && prompt.contains("SILENT") {
        return done();
    }
    if prompt.contains("GENERAL:") || prompt.contains("SECURITY:") {
        let focus = if prompt.contains("GENERAL:") {
            "general"
        } else {
            "security"
        };
        return match turn {
            0 => calls(&[(
                "s-publish",
                "publish",
                json!({ "name": "account", "value": { "focus": focus } }),
            )]),
            _ => done(),
        };
    }
    if prompt.contains("STYLE:") {
        return match turn {
            0 => calls(&[(
                "s-draft",
                "publish",
                json!({ "name": "draft", "value": { "partial": true } }),
            )]),
            _ => ResponseTemplate::new(200).set_body_json(json!({ "garbled": true })),
        };
    }
    if prompt.contains("BOOTSTRAP:") {
        return answer(&json!({ "done": true }));
    }
    if prompt.contains("COMPARE:") {
        let agree = if prompt.contains("ADMIT") {
            json!(false)
        } else if prompt.contains("BLOCK") {
            Value::Null
        } else {
            json!(true)
        };
        return answer(&json!({ "agree": agree }));
    }
    if prompt.contains("EXTRA:") {
        return match turn {
            0 => calls(&[(
                "e-publish",
                "publish",
                json!({ "name": "account", "value": { "extra": true } }),
            )]),
            _ => done(),
        };
    }
    if prompt.contains("SCORE:") {
        let index = prompt
            .split_once("index=")
            .and_then(|(_, rest)| rest.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(u64::MAX);
        return answer(&json!({ "score": index, "text": format!("v{index}") }));
    }
    if prompt.contains("REFINE:") {
        let index: u64 = prompt
            .split_once("index=")
            .and_then(|(_, rest)| rest.split_whitespace().next()?.parse().ok())
            .unwrap_or(u64::MAX);
        if prompt.contains("REFINE-BREAKS") && index == 1 {
            return ResponseTemplate::new(200).set_body_json(json!({ "garbled": true }));
        }
        let score = if prompt.contains("REFINE-EXHAUSTS") {
            json!(0)
        } else if prompt.contains("REFINE-INVALID") {
            json!("high")
        } else {
            json!(index)
        };
        return match turn {
            0 => calls(&[(
                "f-publish",
                "publish",
                json!({ "name": "report", "value": { "version": index } }),
            )]),
            _ => answer(&json!({ "score": score, "text": format!("v{index}") })),
        };
    }
    if prompt.contains("READ-REPORT:") {
        return match turn {
            0 => calls(&[(
                "r-report",
                "fs.read",
                json!({ "path": "/inputs/report.json" }),
            )]),
            _ => done(),
        };
    }
    if prompt.contains("SECTION:") {
        let (section, at) = prompt
            .split_once("SECTION: ")
            .and_then(|(_, rest)| rest.split_once(" at="))
            .map(|(section, at)| {
                (
                    section.to_owned(),
                    at.split_whitespace().next().unwrap_or(""),
                )
            })
            .unwrap_or_default();
        let at: u64 = at.parse().unwrap_or(u64::MAX);
        return match turn {
            0 => calls(&[(
                "d-account",
                "publish",
                json!({ "name": "account", "value": { "section": section, "at": at } }),
            )]),
            _ => done(),
        };
    }
    if prompt.contains("POLISH:") {
        let (section, round) = prompt
            .split_once("POLISH: ")
            .and_then(|(_, rest)| rest.split_once(" item="))
            .and_then(|(section, rest)| Some((section, rest.split_once("round=")?.1)))
            .and_then(|(section, round)| {
                Some((
                    section,
                    round.split_whitespace().next()?.parse::<u64>().ok()?,
                ))
            })
            .unwrap_or_default();
        return answer(&json!({ "score": round, "text": format!("{section} v{round}") }));
    }
    if prompt.contains("ESCALATE:") {
        return match turn {
            0 => calls(&[(
                "x-note",
                "publish",
                json!({ "name": "note", "value": { "escalated": true } }),
            )]),
            _ => ResponseTemplate::new(200).set_body_json(json!({ "garbled": true })),
        };
    }
    if prompt.contains("SUMMARIZE:") {
        let index: u64 = prompt
            .split_once("index=")
            .and_then(|(_, rest)| rest.split_whitespace().next()?.parse().ok())
            .unwrap_or(u64::MAX);
        return match turn {
            0 => calls(&[
                (
                    "m-accounts",
                    "fs.read",
                    json!({ "path": "/inputs/accounts/manifest.json" }),
                ),
                (
                    "m-escalation",
                    "fs.read",
                    json!({ "path": "/inputs/escalation/manifest.json" }),
                ),
            ]),
            1 => calls(&[(
                "m-summary",
                "publish",
                json!({ "name": "summary", "value": { "version": index } }),
            )]),
            _ => answer(&json!({ "score": index, "text": format!("summary v{index}") })),
        };
    }
    if prompt.contains("JUDGE-DOC:") {
        return answer(&json!({
            "outcome": "passed",
            "reason": "the summary names the report",
            "citations": [{ "slot": "document", "locator": { "json_pointer": "/title" } }],
        }));
    }
    if prompt.contains("QUOTE:") {
        return match turn {
            0 => calls(&[(
                "q-read",
                "fs.read",
                json!({ "path": "/inputs/security.json" }),
            )]),
            _ => done(),
        };
    }
    ResponseTemplate::new(500)
}

/// Configure the gateway as `admin`, declare and pull
/// `orchestration-desk` in `admin`'s tenant, and load it as `client`.
///
/// # Panics
/// Panics when a setup step fails.
async fn hydrate(client: &WyrdClient, admin: &WyrdClient) -> WyrdState {
    configure_gateway(admin).await;
    let cards = Cards::connect_with(admin).await.expect("handle connects");
    let desk = declare(
        &cards,
        "cards/verifier_orchestration/orchestration-desk.yaml",
    )
    .await;
    let bundle = pull(&cards, &desk.root).await;
    WyrdState::from_path_with(bundle.path().join("bundle"), client.clone())
        .expect("bundle loads offline")
}

/// Start the scripted upstream every desk's Agents call.
async fn upstream() -> MockServer {
    let upstream = MockServer::start().await;
    let script = Arc::new(Script::default());
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |request: &Request| reply(request, &script))
        .mount(&upstream)
        .await;
    upstream
}

/// The orchestration desk deployment: a verification-capable server whose
/// gateway deploys `gpt-4o` on the scripted upstream, with
/// `orchestration-desk` registered and hydrated; verifications run as a
/// principal granted every tool.
struct Desk {
    /// The deployment.
    deployment: Deployment,
    /// The scripted model upstream.
    upstream: MockServer,
    /// The hydrated Service, as a principal granted every tool.
    state: WyrdState,
}

impl Desk {
    /// Start the scripted upstream and the server, deploy `openai` on the
    /// upstream, declare and pull `orchestration-desk`, and load it as a
    /// principal that may verify, invoke the gateway, and call every tool.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start() -> Self {
        let upstream = upstream().await;
        let builder = builder(&upstream);
        Self::start_with(upstream, builder).await
    }

    /// [`Self::start`] with the server `builder` configures over
    /// `upstream`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn start_with(upstream: MockServer, builder: WyrdTestServerBuilder) -> Self {
        let deployment = Deployment::start_with(builder).await;
        let key = deployment
            .granted_key("every_tool", &grants(json!("all")))
            .await;
        let state = hydrate(&deployment.client(&key), &deployment.admin()).await;
        Self {
            deployment,
            upstream,
            state,
        }
    }

    /// `orchestration-desk` hydrated in a new tenant `slug` of this
    /// deployment, with its own gateway deployment on the same upstream,
    /// loaded as that tenant's administrator.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn tenant(&self, slug: &str) -> WyrdState {
        let admin = self
            .deployment
            .client(&self.deployment.other_tenant_admin(slug).await);
        hydrate(&admin, &admin).await
    }

    /// Queue `question` for `verifier` and wait for its result. The desk's
    /// Agents publish, which only queued execution accepts.
    ///
    /// # Panics
    /// Panics when the submission is refused or does not settle.
    async fn verify(&self, verifier: &str, question: &str) -> VerificationResult {
        self.settled(self.submit(verifier, json!(question)).await)
            .await
    }

    /// Queue the fixture document at `relative` for `verifier` and wait for
    /// its result.
    ///
    /// # Panics
    /// Panics when the upload or the submission is refused or the execution
    /// does not settle.
    async fn verify_document(&self, verifier: &str, relative: &str) -> VerificationResult {
        self.settled(self.submit(verifier, File::path(fixture(relative))).await)
            .await
    }

    /// The messages of every request whose messages carry `marker`, in
    /// arrival order.
    ///
    /// # Panics
    /// Panics when the upstream does not record requests.
    async fn requests(&self, marker: &str) -> Vec<Vec<Value>> {
        self.upstream
            .received_requests()
            .await
            .expect("the upstream records requests")
            .iter()
            .map(messages)
            .filter(|messages| text(messages).contains(marker))
            .collect()
    }

    /// The messages of every request whose messages carry both `marker` and
    /// `question`, in arrival order.
    ///
    /// # Panics
    /// Panics when the upstream does not record requests.
    async fn asked(&self, marker: &str, question: &str) -> Vec<Vec<Value>> {
        self.requests(marker)
            .await
            .into_iter()
            .filter(|messages| text(messages).contains(question))
            .collect()
    }

    /// Queue `input` for `verifier` and return the execution's identity.
    ///
    /// # Panics
    /// Panics when the submission is refused or runs at once.
    async fn submit(&self, verifier: &str, input: impl Into<Inputs>) -> ExecutionId {
        let Submission::Queued(queued) = self
            .state
            .run()
            .observe()
            .submit(verifier, input)
            .await
            .expect("the execution is submitted")
        else {
            panic!("Run evidence queues an execution");
        };
        *queued.id()
    }

    /// Wait until some request carrying both `marker` and `question` has
    /// reached the upstream.
    ///
    /// # Panics
    /// Panics when none arrives within 90 seconds.
    async fn stalled_in(&self, marker: &str, question: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(90);
        while self.asked(marker, question).await.is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "{question}: no {marker} request arrived"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Stop this desk's worker from claiming while it stays inside its held
    /// calls, flush its accepted state, and expire the lease of every
    /// execution in `executions`; a replica over the same storage then takes
    /// them over. Returns the stalled deployment and the desk the replica
    /// serves.
    ///
    /// # Panics
    /// Panics when the flush, a lease expiry, or the replica fails.
    async fn take_over(self, executions: &[ExecutionId]) -> (Deployment, Self) {
        let server = self.deployment.server();
        server.cancel_bound_workers();
        server.flush_bifrost().await.expect("Bifrost flushes");
        let fixture = VerificationFixture::provision(server.state(), server.data_tenant_id())
            .await
            .expect("the tenant provisions");
        for execution in executions {
            fixture
                .expire_lease(*execution)
                .await
                .expect("the stalled worker's lease expires");
        }
        let replica = self.deployment.replica(builder(&self.upstream)).await;
        (
            self.deployment,
            Self {
                deployment: replica,
                upstream: self.upstream,
                state: self.state,
            },
        )
    }

    /// The tenant's verification fixture over this desk's server.
    ///
    /// # Panics
    /// Panics when the tenant cannot be provisioned.
    async fn fixture(&self) -> VerificationFixture {
        let server = self.deployment.server();
        VerificationFixture::provision(server.state(), server.data_tenant_id())
            .await
            .expect("the tenant provisions")
    }

    /// The accepted Bifrost state of `execution` once it satisfies `ready`.
    ///
    /// Steps change state in memory and persist it in background, so a step
    /// the test has seen run may not be accepted yet; this reads again
    /// until it is.
    ///
    /// # Panics
    /// Panics when the state cannot be read or no accepted revision
    /// satisfies `ready` within 30 seconds.
    async fn accepted(
        &self,
        execution: ExecutionId,
        ready: impl Fn(&ExecutionState) -> bool,
    ) -> ExecutionState {
        let fixture = self.fixture().await;
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let state = fixture
                .execution_state(self.deployment.server().state(), execution)
                .await
                .expect("the accepted state reads");
            match state {
                Some(state) if ready(&state) => return state,
                last => assert!(
                    std::time::Instant::now() < deadline,
                    "the accepted state never became ready: {last:?}"
                ),
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Wait for the taken-over `execution` to settle.
    ///
    /// # Panics
    /// Panics when it does not settle within 90 seconds.
    async fn settled(&self, execution: ExecutionId) -> VerificationResult {
        Execution::connect_with(execution, &self.deployment.admin())
            .await
            .expect("the execution handle connects")
            .wait(Duration::from_secs(90))
            .await
            .expect("the execution settles")
    }

    /// Every publication of `result`'s execution, read back as the
    /// deployment's admin through the public paged listing.
    ///
    /// # Panics
    /// Panics when the execution or a page cannot be read.
    async fn publications(&self, result: &VerificationResult) -> Vec<PublicationSummary> {
        let execution = Execution::connect_with(result.execution_id, &self.deployment.admin())
            .await
            .expect("the execution handle connects");
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
}

/// A fixed sweep of three reviewers runs three items, each reading the same
/// pinned question and writing the same scratch name without seeing another
/// item's file. Reviewer 0's failed attempt retries as a second attempt of
/// its own item, never as a fourth reviewer, and starts from empty scratch
/// and a conversation holding none of the failed attempt's tool calls.
/// Every item publishes exactly one account under its own index, and the
/// consolidator's settled collection lists the three accounts.
///
/// # Panics
/// Panics when the verification is refused or an item, attempt, scratch
/// read, mount, or publication differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn fixed_sweeps_have_private_attempts() {
    let desk = Desk::start().await;

    let result = desk.verify("review-sweep", "SWEEP-RUN").await;

    assert_eq!(
        requirement(&result, "consolidated").outcome,
        RequirementOutcome::Passed,
        "{result:?}"
    );
    let review = step(&desk.deployment, &result, "review").await;
    assert_eq!(
        (review.status, review.attempts, review.output.clone()),
        (
            StepStatus::Succeeded,
            4,
            Some(json!([{ "done": true }, { "done": true }, { "done": true }]))
        ),
        "three items, one of them retried once"
    );
    let mut reviewers: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
    for messages in desk.requests("REVIEW:").await {
        if let Some(n) = numbered(&messages) {
            reviewers.insert(n, messages);
        }
    }
    assert_eq!(
        reviewers.keys().copied().collect::<Vec<_>>(),
        [0, 1, 2, 3],
        "three reviewers and one retry"
    );
    let mut questions = BTreeSet::new();
    let mut finished = BTreeSet::new();
    for (&n, conversation) in &reviewers {
        let foreign: Vec<&str> = conversation
            .iter()
            .filter_map(|message| message["tool_calls"].as_array())
            .flatten()
            .filter_map(|call| call["id"].as_str())
            .filter(|id| !id.starts_with(&format!("r{n}-")))
            .collect();
        assert!(
            foreign.is_empty(),
            "reviewer {n}'s history holds only its own attempt: {foreign:?}"
        );
        let results = results(conversation);
        let id = |name: &str| format!("r{n}-{name}");
        assert_eq!(
            results[&id("list")]["files"],
            json!([]),
            "reviewer {n} starts from empty scratch"
        );
        questions.insert(lines(&results[&id("question")]));
        if n == 0 {
            assert!(!results.contains_key(&id("write")), "reviewer 0 failed");
            continue;
        }
        assert_eq!(
            lines(&results[&id("read")]),
            format!("reviewer {n}"),
            "reviewer {n} reads only its own scratch"
        );
        finished.insert(n);
    }
    assert_eq!(questions, BTreeSet::from([json!("SWEEP-RUN").to_string()]));
    assert_eq!(finished, BTreeSet::from([1, 2, 3]));

    let accounts: BTreeMap<u32, u32> = desk
        .publications(&result)
        .await
        .iter()
        .filter(|publication| publication.producer.step_id.as_str() == "review")
        .map(|publication| {
            assert_eq!(publication.name.as_str(), "account");
            (
                publication.producer.item_index.expect("a fanout item"),
                publication.producer.attempt,
            )
        })
        .collect();
    assert_eq!(accounts.keys().copied().collect::<Vec<_>>(), [0, 1, 2]);
    let mut attempts: Vec<u32> = accounts.values().copied().collect();
    attempts.sort_unstable();
    assert_eq!(
        attempts,
        [1, 1, 2],
        "the retried item publishes as attempt 2"
    );

    let consolidator = desk.requests("CONSOLIDATE:").await;
    let last = consolidator
        .last()
        .expect("the consolidator called the model");
    let manifest: Value = serde_json::from_str(&lines(&results(last)["c-accounts"]))
        .expect("the collection manifest is JSON");
    let mut entries: Vec<(Option<u64>, Option<&str>)> = manifest["entries"]
        .as_array()
        .expect("the manifest lists entries")
        .iter()
        .map(|entry| (entry["item_index"].as_u64(), entry["name"].as_str()))
        .collect();
    entries.sort_unstable();
    assert_eq!(
        entries,
        [
            (Some(0), Some("account")),
            (Some(1), Some("account")),
            (Some(2), Some("account"))
        ]
    );
    desk.deployment.shutdown().await;
}

/// One `specialist-review` question's expected observations: each step's
/// status, which specialist conversations ran, and each consolidated
/// collection's status and entry names.
struct Case {
    /// The question, carrying the planner's marker.
    question: &'static str,
    /// The status of `plan`, `security`, `style`, and `quote`.
    statuses: [StepStatus; 4],
    /// The prompt markers whose Agents were called for this question.
    called: &'static [&'static str],
    /// The `security` and `style` collections' status and entry names.
    collections: [(&'static str, &'static [&'static str]); 2],
    /// A failed step and text its reason carries, when one failed for a
    /// reason the case pins.
    reason: Option<(&'static str, &'static str)>,
}

/// A planner's decisions select specialists: true runs one, false skips it
/// before its workspace binds (the stylist's skip reason names its condition,
/// never the security account it would mount) and makes no call, and a `null`, missing, or
/// unreadable decision fails it as an evaluation error rather than skipping.
/// The consolidator reads every settled collection whatever its outcome,
/// the stylist's failed attempt keeping its partial draft and its failed
/// status. The quoter, requiring the security account by name, runs only
/// when that account was produced: a security review that succeeds without
/// publishing it fails the quoter before its model is called. With no specialist selected and only the
/// specialist judged, the verdict is inconclusive, and a direct verify of
/// that graph is refused as queued-only before its tool-free planner runs.
/// The CLI and the HTTP
/// routes report the same branch statuses and publications as the SDK. A
/// held execution cancelled through the SDK makes its wait raise
/// `WYRD_SDK_409_VERIFICATION_CANCELLED` with its identity, while its status
/// keeps the stored `cancelled` code and no further work starts.
///
/// # Panics
/// Panics when a verification or CLI call is refused or a call, status,
/// collection, publication, or verdict differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn conditions_and_collections_preserve_missing_evidence() {
    use StepStatus::{Failed, Skipped, Succeeded};
    let desk = Desk::start().await;
    let cases = [
        Case {
            question: "NEITHER",
            statuses: [Succeeded, Skipped, Skipped, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("skipped", &[]), ("skipped", &[])],
            reason: None,
        },
        Case {
            question: "SECURITY-ONLY",
            statuses: [Succeeded, Succeeded, Skipped, Succeeded],
            called: &["GENERAL:", "SECURITY:", "CONSOLIDATE:", "QUOTE:"],
            collections: [("succeeded", &["account"]), ("skipped", &[])],
            reason: None,
        },
        Case {
            question: "BOTH",
            statuses: [Succeeded, Succeeded, Failed, Succeeded],
            called: &["GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"],
            collections: [("succeeded", &["account"]), ("failed", &["draft"])],
            reason: None,
        },
        Case {
            question: "UNDECIDED",
            statuses: [Succeeded, Failed, Skipped, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("failed", &[]), ("skipped", &[])],
            reason: Some(("security", "kind (null, bool)")),
        },
        Case {
            question: "FORGOTTEN",
            statuses: [Succeeded, Failed, Skipped, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("failed", &[]), ("skipped", &[])],
            reason: Some(("security", "security")),
        },
        Case {
            question: "SILENT",
            statuses: [Succeeded, Succeeded, Skipped, Failed],
            called: &["GENERAL:", "SECURITY:", "CONSOLIDATE:"],
            collections: [("succeeded", &[]), ("skipped", &[])],
            reason: Some(("quote", "published no `account`")),
        },
        Case {
            question: "UNPLANNED",
            statuses: [Failed, Failed, Failed, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("failed", &[]), ("failed", &[])],
            reason: None,
        },
    ];
    for case in cases {
        let question = case.question;
        let result = desk.verify("specialist-review", question).await;
        let mut statuses = Vec::new();
        for id in ["plan", "security", "style", "quote"] {
            statuses.push(step(&desk.deployment, &result, id).await.status);
        }
        assert_eq!(statuses, case.statuses, "{question}: {result:?}");
        assert_eq!(
            step(&desk.deployment, &result, "general").await.status,
            Succeeded,
            "{question}"
        );
        let style = step(&desk.deployment, &result, "style").await;
        if style.status == Skipped {
            assert!(
                style
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("`steps.plan.output.style` was false")),
                "{question}: the condition skips before the account binding resolves: {style:?}"
            );
        }
        assert_eq!(
            requirement(&result, "consolidated").outcome,
            RequirementOutcome::Passed,
            "{question}: consolidation reads failed and skipped collections"
        );
        for marker in ["GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"] {
            let ran = !desk.asked(marker, question).await.is_empty();
            assert_eq!(ran, case.called.contains(&marker), "{question}: {marker}");
        }
        if let Some((id, cause)) = case.reason {
            let failed = step(&desk.deployment, &result, id).await;
            assert!(
                failed
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains(cause)),
                "{question}: {failed:?}"
            );
        }
        let consolidator = desk.asked("CONSOLIDATE:", question).await;
        let read = results(
            consolidator
                .last()
                .expect("the consolidator called the model"),
        );
        for (collection, (status, names)) in ["security", "style"].into_iter().zip(case.collections)
        {
            let manifest: Value = serde_json::from_str(&lines(&read[&format!("c-{collection}")]))
                .unwrap_or_else(|error| panic!("{question}: {collection} manifest: {error}"));
            let entries: Vec<&str> = manifest["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry["name"].as_str())
                .collect();
            assert_eq!(
                (manifest["status"].as_str(), entries.as_slice()),
                (Some(status), names),
                "{question}: {collection} {manifest}"
            );
        }
        // A direct execution may read `completed` before its result is
        // visible, so wait for the result before reading status elsewhere.
        assert_eq!(
            desk.settled(result.execution_id).await.outcome,
            result.outcome,
            "{question}"
        );
        let target = ExecutionArg {
            execution_id: result.execution_id,
        };
        let status = cli(ExecutionVerb::Status(target.clone()), &desk).await;
        assert_eq!(
            status["result"]["outcome"],
            serde_json::to_value(&result.outcome).expect("the outcome serializes"),
            "{question}: the CLI reports the compact result: {status}"
        );
        let listed = cli(
            ExecutionVerb::Publications(PageArgs {
                execution: target,
                limit: None,
                cursor: None,
            }),
            &desk,
        )
        .await;
        let publications =
            serde_json::to_value(desk.publications(&result).await).expect("items serialize");
        assert_eq!(
            listed["items"], publications,
            "{question}: the CLI lists the same publications"
        );
        let id = result.execution_id;
        assert_eq!(
            http(&id.to_string(), &desk).await["result"]["outcome"]["steps"],
            status["result"]["outcome"]["steps"],
            "{question}: the HTTP route reports every branch's status"
        );
        assert_eq!(
            http(&format!("{id}/publications?limit=100"), &desk).await["items"],
            publications,
            "{question}: the HTTP route lists the same publications"
        );
    }

    let unjudged = desk.verify("specialists-only", "NEITHER-ONLY").await;
    assert_eq!(
        (
            step(&desk.deployment, &unjudged, "security").await.status,
            requirement(&unjudged, "secured").outcome,
            unjudged.outcome.verdict
        ),
        (
            Skipped,
            RequirementOutcome::NotApplicable,
            wyrd_ai::VerificationVerdict::Inconclusive
        )
    );
    let refused = desk
        .state
        .run()
        .observe()
        .verify("specialists-only", json!("DIRECT-PLAN"))
        .await
        .expect_err("a graph with a publishing step does not verify directly");
    assert_eq!(refused.code(), "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED");
    assert!(
        desk.requests("DIRECT-PLAN").await.is_empty(),
        "the refusal comes before the planner, which publishes nothing, runs"
    );

    Mock::given(method("POST"))
        .and(body_string_contains("PLAN:"))
        .and(body_string_contains("HELD"))
        .respond_with(done().set_delay(STALLED))
        .with_priority(1)
        .mount(&desk.upstream)
        .await;
    let held = desk.submit("specialist-review", json!("HELD")).await;
    desk.stalled_in("PLAN:", "HELD").await;
    let execution = Execution::connect_with(held, &desk.deployment.admin())
        .await
        .expect("the execution handle connects");
    assert_eq!(
        execution
            .cancel()
            .await
            .expect("the execution cancels")
            .status,
        VerificationExecutionStatus::Cancelled
    );
    let problem = execution
        .wait(Duration::from_secs(90))
        .await
        .expect_err("a cancelled execution yields no result")
        .problem();
    assert_eq!(
        (
            problem.code.as_str(),
            problem.status,
            &problem.details["execution_id"],
            &problem.details["status"]
        ),
        (
            "WYRD_SDK_409_VERIFICATION_CANCELLED",
            409,
            &json!(held),
            &json!("cancelled")
        ),
        "{problem:?}"
    );
    let state = execution.status().await.expect("the status reads");
    assert_eq!(
        (
            state.status,
            state.result.is_none(),
            state.error.map(|error| error.code)
        ),
        (
            VerificationExecutionStatus::Cancelled,
            true,
            Some("cancelled".to_owned())
        ),
        "the stored status keeps its cancelled code"
    );
    for marker in ["SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"] {
        assert!(
            desk.asked(marker, "HELD").await.is_empty(),
            "the cancelled execution starts no {marker} work"
        );
    }
    desk.deployment.shutdown().await;
}

/// The order a `speculative-review` question's race is held in.
#[derive(Clone, Copy)]
enum Race {
    /// The comparison answers only once the extra review's success is
    /// accepted.
    ActionFirst,
    /// The extra review's provider request arrives and its answer is held
    /// until the comparison's disposition is recorded.
    DecisionFirst,
    /// The bootstrap answers only once the comparison is accepted, so the
    /// decision is known before the extra review is eligible.
    Known,
}

/// One `speculative-review` question's expected observations.
struct Speculated {
    /// The question, carrying the comparison's marker.
    question: &'static str,
    /// The order its race is held in.
    race: Race,
    /// Whether the extra reviewer was called at all.
    called: bool,
    /// The extra step's status.
    status: StepStatus,
    /// The disposition the consolidator's manifest reports.
    speculation: &'static str,
    /// The entry names the consolidator was given.
    entries: &'static [&'static str],
    /// The disposition the execution's publication listing reports for the
    /// extra account, when one was published.
    retained: Option<Speculation>,
}

/// Wait until `execution`'s public invocation listing, read as `desk`'s
/// admin, holds an attempt of `step` that `reached` accepts, and return it.
///
/// # Panics
/// Panics when the listing cannot be read, or no such attempt is accepted
/// within 90 seconds.
async fn reached(
    desk: &Desk,
    execution: ExecutionId,
    step: &str,
    reached: impl Fn(&ExecutionInvocation) -> bool,
) -> ExecutionInvocation {
    let handle = Execution::connect_with(execution, &desk.deployment.admin())
        .await
        .expect("the execution handle connects");
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        let page = handle
            .invocations(&ExecutionPageQuery {
                limit: Some(200),
                cursor: None,
            })
            .await
            .expect("the invocations read");
        if let Some(invocation) = page
            .items
            .into_iter()
            .find(|invocation| invocation.producer.step_id.as_str() == step && reached(invocation))
        {
            return invocation;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{execution}: no accepted {step} attempt reached the milestone"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// After the bootstrap succeeds, the extra review runs while the comparison
/// is pending. A disagreement admits it whether it finished before or after
/// the decision; an agreement discards completed work or cancels running
/// work; a `null` comparison blocks it; a decision known before the extra
/// review is eligible makes no call. The consolidator only ever reads the
/// recorded disposition, never a pending one, and receives the account only
/// when it was admitted, while a caller holding only `verifier:read` still
/// inspects the retained excluded account, leaving its disposition unchanged.
///
/// Each race is ordered by holding provider answers and observing the
/// public invocation listing, never by elapsed time: an action-first case
/// sees the extra review's success accepted with no disposition recorded
/// before the comparison answers; a decision-first case sees the extra
/// review's request arrive, then the comparison accepted while the extra
/// review still runs, a discard recorded at once and an admission only once
/// the admitted review settles; the known case sees the comparison accepted before the bootstrap answers.
///
/// # Panics
/// Panics when a verification is refused or a milestone, call, status,
/// disposition, mounted entry, or retained publication differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn speculative_disposition_controls_consumer_admission() {
    use StepStatus::{Failed, Skipped, Succeeded};
    let upstream = upstream().await;
    let held = HeldUpstream::start(&upstream)
        .await
        .expect("the held upstream binds");
    let builder = builder(&upstream).with_gateway_provider_root_for_test(
        held.uri().parse().expect("the held upstream's URL parses"),
    );
    let desk = Desk::start_with(upstream, builder).await;
    let cases = [
        Speculated {
            question: "SPEC-ADMIT-LATE-DECISION",
            race: Race::ActionFirst,
            called: true,
            status: Succeeded,
            speculation: "admitted",
            entries: &["account"],
            retained: Some(Speculation::Admitted),
        },
        Speculated {
            question: "SPEC-ADMIT-EARLY-DECISION",
            race: Race::DecisionFirst,
            called: true,
            status: Succeeded,
            speculation: "admitted",
            entries: &["account"],
            retained: Some(Speculation::Admitted),
        },
        Speculated {
            question: "SPEC-DISCARD-LATE-DECISION",
            race: Race::ActionFirst,
            called: true,
            status: Skipped,
            speculation: "discarded",
            entries: &[],
            retained: Some(Speculation::Discarded),
        },
        Speculated {
            question: "SPEC-DISCARD-EARLY-DECISION",
            race: Race::DecisionFirst,
            called: true,
            status: Skipped,
            speculation: "discarded",
            entries: &[],
            retained: None,
        },
        Speculated {
            question: "SPEC-BLOCK-LATE-DECISION",
            race: Race::ActionFirst,
            called: true,
            status: Failed,
            speculation: "blocked",
            entries: &[],
            retained: Some(Speculation::Blocked),
        },
        Speculated {
            question: "SPEC-KNOWN",
            race: Race::Known,
            called: false,
            status: Skipped,
            speculation: "discarded",
            entries: &[],
            retained: None,
        },
    ];
    let extra = TaskId::new("extra").expect("step");
    let account = PublicationName::new("account").expect("name");
    let reader = desk.deployment.client(
        &desk
            .deployment
            .granted_key(
                "speculation_reader",
                &[json!({"resource": "verifier", "action": "read", "scope": "all"})],
            )
            .await,
    );
    for case in cases {
        let question = case.question;
        let compare = ["COMPARE:", question];
        let reviewing = ["EXTRA:", question];
        let bootstrap = ["BOOTSTRAP:", question];
        match case.race {
            Race::ActionFirst => held.hold(&compare),
            Race::DecisionFirst => {
                held.hold(&compare);
                held.hold(&reviewing);
            }
            Race::Known => held.hold(&bootstrap),
        }
        let execution = desk.submit("speculative-review", json!(question)).await;
        match case.race {
            Race::ActionFirst => {
                let settled = reached(&desk, execution, "extra", |invocation| {
                    matches!(invocation.progress, InvocationProgress::Action { .. })
                })
                .await;
                assert_eq!(
                    (settled.progress, settled.speculation),
                    (
                        InvocationProgress::Action { status: Succeeded },
                        Speculation::None
                    ),
                    "{question}: the extra review succeeds before any disposition is recorded"
                );
                held.release(&compare);
            }
            Race::DecisionFirst => {
                desk.stalled_in("EXTRA:", question).await;
                held.release(&compare);
                let compared = reached(&desk, execution, "compare", |invocation| {
                    matches!(invocation.progress, InvocationProgress::Action { .. })
                })
                .await;
                assert_eq!(
                    compared.progress,
                    InvocationProgress::Action { status: Succeeded },
                    "{question}: the comparison is accepted"
                );
                let running = if case.retained.is_some() {
                    reached(&desk, execution, "extra", |_| true).await
                } else {
                    reached(&desk, execution, "extra", |invocation| {
                        invocation.speculation != Speculation::None
                    })
                    .await
                };
                assert_eq!(
                    (running.progress, running.speculation),
                    (
                        InvocationProgress::Started,
                        case.retained
                            .map_or(Speculation::Discarded, |_| Speculation::None)
                    ),
                    "{question}: the decision lands while the extra review runs"
                );
                held.release(&reviewing);
            }
            Race::Known => {
                let compared = reached(&desk, execution, "compare", |invocation| {
                    matches!(invocation.progress, InvocationProgress::Action { .. })
                })
                .await;
                assert_eq!(
                    compared.progress,
                    InvocationProgress::Action { status: Succeeded },
                    "{question}: the comparison is accepted before the bootstrap answers"
                );
                assert!(
                    desk.asked("EXTRA:", question).await.is_empty(),
                    "{question}: nothing calls the extra review before it is eligible"
                );
                held.release(&bootstrap);
            }
        }
        let result = desk.settled(execution).await;
        assert_eq!(
            requirement(&result, "consolidated").outcome,
            RequirementOutcome::Passed,
            "{question}: {result:?}"
        );
        assert_eq!(
            step(&desk.deployment, &result, "extra").await.status,
            case.status,
            "{question}: {result:?}"
        );
        assert_eq!(
            !desk.asked("EXTRA:", question).await.is_empty(),
            case.called,
            "{question}"
        );
        let consolidator = desk.asked("CONSOLIDATE:", question).await;
        let read = results(
            consolidator
                .last()
                .expect("the consolidator called the model"),
        );
        let manifest: Value = serde_json::from_str(&lines(&read["c-extra"]))
            .unwrap_or_else(|error| panic!("{question}: manifest: {error}"));
        let entries: Vec<&str> = manifest["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert_eq!(
            (manifest["speculation"].as_str(), entries.as_slice()),
            (Some(case.speculation), case.entries),
            "{question}: {manifest}"
        );
        let retained: Vec<Speculation> = desk
            .publications(&result)
            .await
            .iter()
            .filter(|publication| publication.producer.step_id == extra)
            .map(|publication| publication.speculation)
            .collect();
        assert_eq!(retained, Vec::from_iter(case.retained), "{question}");
        if case.retained.is_some() {
            let execution = Execution::connect_with(result.execution_id, &reader)
                .await
                .expect("the execution handle connects");
            let inspected = execution
                .publication(
                    &extra,
                    &account,
                    &PublicationSelector {
                        attempt: 1,
                        item_index: None,
                        iteration_index: None,
                        path: None,
                    },
                )
                .await
                .expect("a verifier:read grant inspects the retained account");
            assert_eq!(
                inspected.content,
                PublicationContent::Json {
                    value: json!({ "extra": true })
                },
                "{question}"
            );
            let reread: Vec<Speculation> = desk
                .publications(&result)
                .await
                .iter()
                .filter(|publication| publication.producer.step_id == extra)
                .map(|publication| publication.speculation)
                .collect();
            assert_eq!(
                reread,
                Vec::from_iter(case.retained),
                "{question}: inspection leaves the disposition unchanged"
            );
        }
    }
    desk.deployment.shutdown().await;
}

/// The iteration index each `REFINE:` request for `question` was sent for,
/// in arrival order.
fn iterations(requests: &[Vec<Value>]) -> Vec<u64> {
    requests
        .iter()
        .filter_map(|messages| {
            text(messages)
                .split_once("index=")?
                .1
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .collect()
}

/// The refiner rewrites one iteration at a time: iteration 0 sees a `null`
/// previous answer and each later one its predecessor's, and no iteration
/// starts before the previous one finished. `until` stops it at score 2,
/// each iteration's requirement is judged under its own index, and the
/// reader receives only the final iteration's report. Exhausting
/// `max_iterations`, a score `until` cannot compare, or a broken iteration
/// fails the step without another iteration, and the reader requiring the
/// report never runs.
///
/// # Panics
/// Panics when a verification is refused or an iteration, previous value,
/// order, report, or outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn sequential_refinement_returns_final_admitted_report() {
    let desk = Desk::start().await;

    let result = desk.verify("refine-report", "REFINE-STOPS").await;
    let draft = step(&desk.deployment, &result, "draft").await;
    assert_eq!(
        (draft.status, draft.output.clone()),
        (
            StepStatus::Succeeded,
            Some(json!({ "score": 2, "text": "v2" }))
        ),
        "{result:?}"
    );
    assert_eq!(
        requirement(&result, "final").outcome,
        RequirementOutcome::Passed
    );
    let asked = desk.asked("REFINE:", "REFINE-STOPS").await;
    assert_eq!(
        iterations(&asked),
        [0, 0, 1, 1, 2, 2],
        "two turns per iteration, never overlapping"
    );
    let first = text(&asked[0]);
    assert!(first.contains("previous=null"), "{first}");
    let second = text(&asked[2]);
    assert!(
        second.contains(r#""score":0"#) && second.contains(r#""text":"v0""#),
        "iteration 1 sees iteration 0's answer: {second}"
    );
    let judged: Vec<(Option<u32>, RequirementOutcome)> = result
        .outcome
        .requirements
        .iter()
        .filter(|judged| judged.id.as_str() == "scored")
        .map(|judged| (judged.iteration_index, judged.outcome))
        .collect();
    assert_eq!(
        judged,
        (0..3)
            .map(|iteration| (Some(iteration), RequirementOutcome::Passed))
            .collect::<Vec<_>>()
    );
    let reader = desk.asked("READ-REPORT:", "REFINE-STOPS").await;
    let read = results(reader.last().expect("the reader called the model"));
    assert_eq!(
        serde_json::from_str::<Value>(&lines(&read["r-report"])).ok(),
        Some(json!({ "version": 2 })),
        "the reader receives the final iteration's report"
    );
    let published: Vec<Option<u32>> = desk
        .publications(&result)
        .await
        .iter()
        .filter(|publication| publication.producer.step_id.as_str() == "draft")
        .map(|publication| publication.producer.iteration_index)
        .collect();
    assert_eq!(published, [Some(0), Some(1), Some(2)]);

    for (question, reason, ran) in [
        (
            "REFINE-EXHAUSTS",
            "max_iterations 4",
            &[0, 0, 1, 1, 2, 2, 3, 3][..],
        ),
        ("REFINE-INVALID", "cannot be evaluated", &[0, 0][..]),
        ("REFINE-BREAKS", "", &[0, 0, 1][..]),
    ] {
        let result = desk.verify("refine-report", question).await;
        let draft = step(&desk.deployment, &result, "draft").await;
        assert_eq!(
            (draft.status, draft.output.clone()),
            (StepStatus::Failed, None),
            "{question}"
        );
        assert!(
            draft
                .reason
                .as_deref()
                .is_some_and(|text| text.contains(reason)),
            "{question}: {draft:?}"
        );
        assert_eq!(
            iterations(&desk.asked("REFINE:", question).await),
            ran,
            "{question}"
        );
        assert_eq!(
            step(&desk.deployment, &result, "read").await.status,
            StepStatus::Skipped,
            "{question}"
        );
        assert!(
            desk.asked("READ-REPORT:", question).await.is_empty(),
            "{question}"
        );
        assert_ne!(
            requirement(&result, "final").outcome,
            RequirementOutcome::Passed,
            "{question}"
        );
    }
    desk.deployment.shutdown().await;
}

/// The entries of the collection manifest an `fs.read` `result` returned,
/// as `(item_index, iteration_index, name)`, sorted.
fn entries(result: &Value) -> Vec<(Option<u64>, Option<u64>, String)> {
    let manifest: Value = serde_json::from_str(&lines(result)).unwrap_or_default();
    let mut entries: Vec<_> = manifest["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            (
                entry["item_index"].as_u64(),
                entry["iteration_index"].as_u64(),
                entry["name"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    entries.sort();
    entries
}

/// A non-repository document pipeline uses the same public controls: the
/// uploaded document's sections fan out as array work in order, the
/// duplicate section keeping its own index; each section is polished by its
/// own sequential repeat, the final values in input order; the empty appendix runs no
/// reviewer and is honestly empty; the flagged document escalates and its
/// failed escalation keeps its partial note, while an unflagged one skips
/// the escalation and mounts it as empty; the summarizer refines one
/// iteration at a time over both settled collections within its original
/// bound. The final requirements pass, fail, and stay inconclusive by their
/// own evidence, and the cited judgment points into the uploaded document.
///
/// # Panics
/// Panics when the verification is refused or an item, manifest,
/// iteration, citation, or outcome differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn document_pipeline_uses_array_repeat_and_publications() {
    let desk = Desk::start().await;

    let result = desk
        .verify_document(
            "document-pipeline",
            "cards/verifier_orchestration/samples/field-report.json",
        )
        .await;

    let sections = step(&desk.deployment, &result, "sections").await;
    assert_eq!(
        (sections.status, sections.output.clone()),
        (
            StepStatus::Succeeded,
            Some(json!([{ "done": true }, { "done": true }, { "done": true }]))
        ),
        "{result:?}"
    );
    let reviewed: BTreeSet<String> = desk
        .requests("SECTION:")
        .await
        .iter()
        .filter_map(|messages| {
            let prompt = text(messages);
            Some(prompt.split_once("SECTION: ")?.1.lines().next()?.to_owned())
        })
        .collect();
    assert_eq!(
        reviewed,
        BTreeSet::from(["claims at=1", "claims at=2", "intro at=0"].map(str::to_owned)),
        "the duplicate keeps its own index and the empty appendix calls no reviewer"
    );
    let polish = step(&desk.deployment, &result, "polish").await;
    assert_eq!(
        (polish.status, polish.output.clone()),
        (
            StepStatus::Succeeded,
            Some(json!([
                { "score": 1, "text": "intro v1" },
                { "score": 1, "text": "claims v1" },
                { "score": 1, "text": "claims v1" }
            ]))
        ),
        "each section repeats independently and the final values keep input order"
    );
    let mut rounds: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for messages in desk.requests("POLISH:").await {
        let prompt = text(&messages);
        if let Some((item, round)) = prompt
            .split_once(" item=")
            .and_then(|(_, rest)| rest.split_once(" round="))
        {
            rounds
                .entry(item.to_owned())
                .or_default()
                .push(round.split_whitespace().next().unwrap_or("").to_owned());
        }
    }
    assert_eq!(
        rounds,
        BTreeMap::from(
            ["0", "1", "2"].map(|item| (item.to_owned(), vec!["0".to_owned(), "1".to_owned()]))
        ),
        "every item runs its own rounds in order until its condition holds"
    );
    let appendix = step(&desk.deployment, &result, "appendix").await;
    assert_eq!(
        (appendix.status, appendix.output.clone()),
        (StepStatus::Succeeded, Some(json!([])))
    );
    assert_eq!(
        step(&desk.deployment, &result, "escalate").await.status,
        StepStatus::Failed
    );
    let summarize = step(&desk.deployment, &result, "summarize").await;
    assert_eq!(
        (summarize.status, summarize.output.clone()),
        (
            StepStatus::Succeeded,
            Some(json!({ "score": 1, "text": "summary v1" }))
        )
    );
    let summaries = desk.requests("SUMMARIZE:").await;
    assert_eq!(
        iterations(&summaries),
        [0, 0, 0, 1, 1, 1],
        "one iteration at a time, stopping within the bound of 3"
    );
    assert!(text(&summaries[0]).contains("previous=null"));
    let read = results(&summaries[1]);
    assert_eq!(
        entries(&read["m-accounts"]),
        (0..3)
            .map(|item| (Some(item), None, "account".to_owned()))
            .collect::<Vec<_>>()
    );
    let escalation: Value = serde_json::from_str(&lines(&read["m-escalation"])).unwrap_or_default();
    assert_eq!(
        (
            escalation["status"].as_str(),
            entries(&read["m-escalation"])
        ),
        (Some("failed"), vec![(None, None, "note".to_owned())]),
        "the failed escalation keeps its partial note"
    );
    let accounts: Vec<(Option<u32>, Value)> = {
        let execution = Execution::connect_with(result.execution_id, &desk.deployment.admin())
            .await
            .expect("the execution handle connects");
        let mut accounts = Vec::new();
        for publication in desk.publications(&result).await {
            if publication.producer.step_id.as_str() != "sections" {
                continue;
            }
            let read = execution
                .publication(
                    &publication.producer.step_id,
                    &publication.name,
                    &PublicationSelector {
                        attempt: publication.producer.attempt,
                        item_index: publication.producer.item_index,
                        iteration_index: None,
                        path: None,
                    },
                )
                .await
                .expect("the account reads");
            let PublicationContent::Json { value } = read.content else {
                panic!("an account is a JSON publication");
            };
            accounts.push((publication.producer.item_index, value));
        }
        accounts.sort_by_key(|(item, _)| *item);
        accounts
    };
    assert_eq!(
        accounts,
        [
            (Some(0), json!({ "section": "intro", "at": 0 })),
            (Some(1), json!({ "section": "claims", "at": 1 })),
            (Some(2), json!({ "section": "claims", "at": 2 })),
        ]
    );

    let outcomes: BTreeMap<&str, RequirementOutcome> = result
        .outcome
        .requirements
        .iter()
        .map(|judged| (judged.id.as_str(), judged.outcome))
        .collect();
    assert_eq!(
        outcomes,
        BTreeMap::from([
            ("appendix-reviewed", RequirementOutcome::Failed),
            ("cited", RequirementOutcome::Passed),
            ("escalation-done", RequirementOutcome::Inconclusive),
            ("summarized", RequirementOutcome::Passed),
        ])
    );
    assert_eq!(
        serde_json::to_value(&requirement(&result, "cited").citations).expect("citations encode"),
        json!([{ "slot": "document", "locator": { "json_pointer": "/title" } }])
    );
    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);

    let clean = desk
        .verify_document(
            "document-pipeline",
            "cards/verifier_orchestration/samples/clean-report.json",
        )
        .await;
    let escalate = step(&desk.deployment, &clean, "escalate").await;
    assert!(
        escalate.status == StepStatus::Skipped
            && escalate
                .reason
                .as_deref()
                .is_some_and(|text| text.contains("`steps.read.output.value.flagged` was false")),
        "an unflagged document never escalates: {escalate:?}"
    );
    assert!(desk.requests("ESCALATE: Clean report").await.is_empty());
    let summaries = desk.requests("SUMMARIZE: Clean report").await;
    let escalation: Value =
        serde_json::from_str(&lines(&results(&summaries[1])["m-escalation"])).unwrap_or_default();
    assert_eq!(
        (escalation["status"].as_str(), escalation["entries"].clone()),
        (Some("skipped"), json!([])),
        "the skipped escalation is mounted as an honest empty collection"
    );
    assert_eq!(
        requirement(&clean, "escalation-done").outcome,
        RequirementOutcome::NotApplicable,
        "an intentional skip leaves its requirement not applicable"
    );
    desk.deployment.shutdown().await;
}

/// A queued execution crashes while its second refinement iteration's model
/// call is held: its frozen items, its speculative decision, and its
/// settled work are already accepted in Bifrost. The restarted server
/// reclaims it and resumes from that logical state: no section reviewer,
/// comparison, extra review, or settled iteration runs again; the
/// interrupted iteration spends its attempt and runs as its retry, attempt
/// 2; and the refinement stops
/// within its original bound with every accepted publication kept.
///
/// # Panics
/// Panics when the submission is refused or does not settle, accepted
/// state is missing, or settled work runs again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn logical_control_state_resumes_from_bifrost() {
    const MARKER: &str = "RESUME-ADMIT";
    let desk = Desk::start().await;
    Mock::given(method("POST"))
        .and(body_string_contains(MARKER))
        .and(body_string_contains("index=1"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(600)))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&desk.upstream)
        .await;
    let Submission::Queued(queued) = desk
        .state
        .run()
        .observe()
        .submit(
            "resume-pipeline",
            json!({ "marker": MARKER, "sections": ["alpha", "beta"] }),
        )
        .await
        .expect("the pipeline is submitted")
    else {
        panic!("Run evidence queues an execution");
    };
    let execution = *queued.id();
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while !iterations(&desk.asked("REFINE:", MARKER).await).contains(&1) {
        assert!(
            std::time::Instant::now() < deadline,
            "the second iteration never started"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let server = desk.deployment.server();
    let map = TaskId::new("map").expect("step");
    let extra = TaskId::new("extra").expect("step");
    let accepted = desk
        .accepted(execution, |state| {
            state.speculation(&extra) != Speculation::None
        })
        .await;
    assert!(
        accepted.controls.contains(&ControlRecord::Items {
            step: map.clone(),
            items: vec![json!("alpha"), json!("beta")],
        }),
        "{:?}",
        accepted.controls
    );
    assert_eq!(accepted.speculation(&extra), Speculation::Admitted);
    let before = (
        desk.requests("SECTION:").await.len(),
        desk.asked("COMPARE:", MARKER).await.len(),
        desk.asked("EXTRA:", MARKER).await.len(),
    );
    assert_eq!(before.0, 4, "two turns for each of two sections");
    server.flush_bifrost().await.expect("Bifrost flushes");

    let upstream = desk.upstream;
    let deployment = desk.deployment.restart(builder(&upstream)).await;
    let server = deployment.server();
    VerificationFixture::provision(server.state(), server.data_tenant_id())
        .await
        .expect("the tenant provisions")
        .expire_deadlines(execution)
        .await
        .expect("the crashed process's lease expires");
    let result = Execution::connect_with(execution, &deployment.admin())
        .await
        .expect("the execution handle connects")
        .wait(Duration::from_secs(90))
        .await
        .expect("the recovered execution settles");
    let desk = Desk {
        deployment,
        upstream,
        state: desk.state,
    };

    assert_eq!(
        requirement(&result, "refined").outcome,
        RequirementOutcome::Passed,
        "{result:?}"
    );
    assert_eq!(
        (
            desk.requests("SECTION:").await.len(),
            desk.asked("COMPARE:", MARKER).await.len(),
            desk.asked("EXTRA:", MARKER).await.len(),
        ),
        before,
        "settled items, the decision, and the admitted review are restored"
    );
    assert_eq!(
        step(&desk.deployment, &result, "map").await.output,
        Some(json!([{ "done": true }, { "done": true }]))
    );
    assert_eq!(
        iterations(&desk.asked("REFINE:", MARKER).await),
        [0, 0, 1, 1, 1, 2, 2],
        "iteration 0 is restored and the held iteration 1 runs once more"
    );
    let refine = step(&desk.deployment, &result, "refine").await;
    assert_eq!(
        (refine.status, refine.output.clone()),
        (
            StepStatus::Succeeded,
            Some(json!({ "score": 2, "text": "v2" }))
        )
    );
    let attempts: Vec<(Option<u32>, u32)> = desk
        .publications(&result)
        .await
        .iter()
        .filter(|publication| publication.producer.step_id.as_str() == "refine")
        .map(|publication| {
            (
                publication.producer.iteration_index,
                publication.producer.attempt,
            )
        })
        .collect();
    assert_eq!(
        attempts,
        [(Some(0), 1), (Some(1), 2), (Some(2), 1)],
        "the interrupted iteration publishes as its second attempt"
    );
    let mut published: Vec<(String, Option<u32>, Option<u32>, Speculation)> = desk
        .publications(&result)
        .await
        .iter()
        .map(|publication| {
            (
                publication.producer.step_id.as_str().to_owned(),
                publication.producer.item_index,
                publication.producer.iteration_index,
                publication.speculation,
            )
        })
        .collect();
    published.sort_by(|a, b| (&a.0, a.1, a.2).cmp(&(&b.0, b.1, b.2)));
    assert_eq!(
        published,
        [
            ("extra".to_owned(), None, None, Speculation::Admitted),
            ("map".to_owned(), Some(0), None, Speculation::None),
            ("map".to_owned(), Some(1), None, Speculation::None),
            ("refine".to_owned(), None, Some(0), Speculation::None),
            ("refine".to_owned(), None, Some(1), Speculation::None),
            ("refine".to_owned(), None, Some(2), Speculation::None),
        ]
    );
    desk.deployment.shutdown().await;
}

/// How long a stalled worker's reply waits: longer than the whole journey,
/// so the worker is still inside its call throughout the takeover.
const STALLED: Duration = Duration::from_secs(600);

/// Two queued refinements, one that stops and one that exhausts, are each
/// stalled in iteration 1's first model call when their worker stops
/// claiming and their leases expire. A second replica takes both over while
/// the first worker still holds the stalled calls: settled iteration 0 is
/// restored, the interrupted iteration runs as its retry, and each
/// refinement stops or exhausts within its original `max_iterations`. When
/// the first worker stops, still inside its stalled calls, nothing it held
/// has started anything: across both workers, every iteration's calls
/// precede the next iteration's, and no iteration runs twice.
///
/// # Panics
/// Panics when a submission is refused or does not settle, or an iteration
/// overlaps or runs again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn repeat_stops_or_exhausts_without_overlapping() {
    const STOPS: &str = "REFINE-TAKEOVER";
    const EXHAUSTS: &str = "REFINE-EXHAUSTS-TAKEOVER";
    let desk = Desk::start().await;
    let mut executions = Vec::new();
    for question in [STOPS, EXHAUSTS] {
        Mock::given(method("POST"))
            .and(body_string_contains(question))
            .and(body_string_contains("index=1"))
            .respond_with(answer(&json!({ "late": true })).set_delay(STALLED))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&desk.upstream)
            .await;
        executions.push((
            question,
            desk.submit("repeat-takeover", json!(question)).await,
        ));
    }
    for (question, _) in &executions {
        desk.stalled_in("index=1", question).await;
    }
    let ids: Vec<ExecutionId> = executions.iter().map(|(_, execution)| *execution).collect();
    let (stalled, desk) = desk.take_over(&ids).await;
    for (question, execution) in &executions {
        let result = desk.settled(*execution).await;
        let attempts: Vec<(Option<u32>, u32)> = desk
            .publications(&result)
            .await
            .iter()
            .map(|publication| {
                (
                    publication.producer.iteration_index,
                    publication.producer.attempt,
                )
            })
            .collect();
        assert_eq!(
            attempts,
            [(Some(0), 1), (Some(1), 2), (Some(2), 1)],
            "{question}: the interrupted iteration publishes as its retry"
        );
        let draft = step(&desk.deployment, &result, "draft").await;
        if *question == STOPS {
            assert_eq!(
                (draft.status, draft.output.clone()),
                (
                    StepStatus::Succeeded,
                    Some(json!({ "score": 2, "text": "v2" }))
                ),
                "{result:?}"
            );
            assert_eq!(
                requirement(&result, "final").outcome,
                RequirementOutcome::Passed
            );
        } else {
            assert_eq!(
                (draft.status, draft.output.clone()),
                (StepStatus::Failed, None)
            );
            assert!(
                draft
                    .reason
                    .as_deref()
                    .is_some_and(|text| text.contains("max_iterations 3")),
                "{draft:?}"
            );
            assert_ne!(
                requirement(&result, "final").outcome,
                RequirementOutcome::Passed
            );
        }
    }

    stalled.shutdown().await;
    for (question, _) in &executions {
        assert_eq!(
            iterations(&desk.asked("REFINE:", question).await),
            [0, 0, 1, 1, 1, 2, 2],
            "{question}: iteration 1 runs once more and nothing overlaps"
        );
    }
    desk.deployment.shutdown().await;
}

/// Hold all but `leave` slots of `desk`'s reserved Scribe share, counting
/// the `already` slots the caller holds, and return the new reservations with
/// the total now held.
///
/// Staged work is flushed between attempts, so slots that in-flight writes
/// free are taken as they return; it finishes only when the caller holds
/// exactly the share less `leave`, so no other reservation remains beyond
/// those `leave` slots.
///
/// # Panics
/// Panics when Bifrost cannot flush, a free slot cannot be reserved, or the
/// share is not held within 30 seconds.
async fn hold_reserved_share(
    desk: &Desk,
    already: usize,
    leave: usize,
) -> (Vec<ScribeReservation>, usize) {
    let server = desk.deployment.server();
    let outbox = server.state().bifrost.scribe_outbox();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let (mut share, mut held) = (Vec::new(), already);
    loop {
        server.flush_bifrost().await.expect("staged work drains");
        let full = outbox
            .reserve(usize::MAX)
            .expect_err("no reservation holds every slot");
        let target = full.limit - leave;
        let take = (full.limit - full.admitted).min(target - held);
        if take > 0 {
            share.push(
                outbox
                    .reserve(take)
                    .expect("the free reservable share is held"),
            );
            held += take;
        }
        if held == target {
            return (share, held);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{} reserved slots stay held elsewhere",
            full.limit - leave - held
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Refinement runs from memory while background persistence is held.
///
/// A direct verify of a refinement that publishes nothing, admitted with
/// its own reserved slots, stalls inside iteration 0's model call; the
/// server's remaining reserved Scribe share is then held and the model
/// released. Every dependent iteration runs and the caller receives its
/// selected report. The direct execution keeps no execution state and no
/// queue record; once its records drain, its status and details read
/// back from them.
///
/// A queued execution is then submitted while all but the one slot its
/// admission stages through is held; once it stalls in its first model call
/// that slot is held too, so none of its later revisions or its result can
/// be staged. Its stalled model call is released and every iteration runs from memory while the
/// execution stays `running`, its claim kept for the result it cannot yet
/// persist. Once the share is released the execution settles with the same
/// final report and no further model request.
///
/// # Panics
/// Panics when a verification is refused, an iteration does not run while
/// persistence is held, the queued execution settles before persistence
/// returns, or settling calls the model again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn refinement_progresses_while_persistence_is_held() {
    const DIRECT: &str = "REFINE-HELD-DIRECT";
    const QUEUED: &str = "REFINE-HELD-QUEUED";
    let final_report = (
        StepStatus::Succeeded,
        Some(json!({ "score": 2, "text": "v2" })),
    );
    let upstream = upstream().await;
    let held = HeldUpstream::start(&upstream)
        .await
        .expect("the held upstream binds");
    let builder = builder(&upstream).with_gateway_provider_root_for_test(
        held.uri().parse().expect("the held upstream's URL parses"),
    );
    let desk = Desk::start_with(upstream, builder).await;

    held.hold(&[DIRECT]);
    let direct = {
        let state = desk.state.clone();
        tokio::spawn(async move {
            state
                .run()
                .observe()
                .verify("refine-direct", json!(DIRECT))
                .await
        })
    };
    desk.stalled_in("SCORE:", DIRECT).await;
    let (share, _) = hold_reserved_share(&desk, 0, 2).await;
    held.release(&[DIRECT]);
    let direct = direct
        .await
        .expect("the direct verify joins")
        .expect("the direct verify completes while persistence is held");
    assert_eq!(
        requirement(&direct, "final").outcome,
        RequirementOutcome::Passed,
        "{direct:?}"
    );
    assert_eq!(
        iterations(&desk.asked("SCORE:", DIRECT).await),
        [0, 1, 2],
        "every direct iteration ran while persistence was held"
    );
    drop(share);
    desk.deployment
        .server()
        .flush_bifrost()
        .await
        .expect("the direct records drain");
    let fixture = desk.fixture().await;
    assert!(
        fixture
            .execution_state(desk.deployment.server().state(), direct.execution_id)
            .await
            .expect("the state reads")
            .is_none(),
        "a direct execution keeps no execution state"
    );
    assert!(
        fixture
            .executions()
            .await
            .expect("executions read")
            .is_empty(),
        "a direct execution admits no queue record"
    );
    let execution = Execution::connect_with(direct.execution_id, &desk.deployment.admin())
        .await
        .expect("the execution handle connects");
    let status = execution.status().await.expect("the direct status reads");
    let recorded = status.result.expect("the direct status carries its result");
    assert_eq!(
        (
            status.status,
            &recorded.verifier,
            &recorded.subject,
            recorded.outcome.verdict
        ),
        (
            VerificationExecutionStatus::Completed,
            &direct.verifier,
            &direct.subject,
            direct.outcome.verdict
        ),
        "the direct status reads its recorded result"
    );
    let draft = step(&desk.deployment, &direct, "draft").await;
    assert_eq!(
        (draft.status, draft.output.clone()),
        final_report.clone(),
        "the direct details read the recorded step: {direct:?}"
    );
    assert_eq!(
        execution
            .invocations(&ExecutionPageQuery::default())
            .await
            .expect_err("a direct execution has no invocation history")
            .code(),
        "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"
    );

    held.hold(&[QUEUED]);
    let (admitting, already) = hold_reserved_share(&desk, 0, 1).await;
    let queued = desk.submit("repeat-takeover", json!(QUEUED)).await;
    desk.stalled_in("REFINE:", QUEUED).await;
    let (share, _) = hold_reserved_share(&desk, already, 0).await;
    held.release(&[QUEUED]);
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while iterations(&desk.asked("REFINE:", QUEUED).await).len() < 6 {
        assert!(
            std::time::Instant::now() < deadline,
            "the queued refinement stopped while persistence was held"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let execution = Execution::connect_with(queued, &desk.deployment.admin())
        .await
        .expect("the execution handle connects");
    assert_eq!(
        execution.status().await.expect("the status reads").status,
        VerificationExecutionStatus::Running,
        "the queued execution waits for its result to be accepted"
    );
    assert_eq!(
        iterations(&desk.asked("REFINE:", QUEUED).await),
        [0, 0, 1, 1, 2, 2],
        "every queued iteration ran from memory while persistence was held"
    );

    drop((admitting, share));
    let result = desk.settled(queued).await;
    let draft = step(&desk.deployment, &result, "draft").await;
    assert_eq!(
        (draft.status, draft.output.clone()),
        final_report,
        "{result:?}"
    );
    assert_eq!(
        iterations(&desk.asked("REFINE:", QUEUED).await),
        [0, 0, 1, 1, 2, 2],
        "settling once persistence returns calls the model no more"
    );
    desk.deployment.shutdown().await;
}

/// A takeover after a speculative decision is recorded honours it and
/// never runs the extra review again. Each comparison decides late, held
/// at a [`HeldUpstream`] until the public surface shows the extra review's
/// milestone: an admitted extra review is accepted as succeeded with no
/// disposition recorded, while a discarded or blocked one has published
/// and is inside its second call when the comparison is released and the
/// decision cancels it. Every
/// consolidator is stalled when its worker stops and the leases expire. The
/// replica restores each recorded disposition: it calls neither the
/// comparer nor the extra reviewer, mounts only admitted content into the
/// consolidator's retry, and keeps excluded content retained with its
/// disposition.
///
/// # Panics
/// Panics when a submission is refused or does not settle, a disposition
/// is not recorded before the takeover, or recovery calls a decided step
/// again or mounts excluded content.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn speculative_recovery_cannot_resurrect_content() {
    let upstream = upstream().await;
    let held = HeldUpstream::start(&upstream)
        .await
        .expect("the held upstream binds");
    let builder = builder(&upstream).with_gateway_provider_root_for_test(
        held.uri().parse().expect("the held upstream's URL parses"),
    );
    let desk = Desk::start_with(upstream, builder).await;
    let cases = [
        (
            "SPEC-ADMIT-LATE-DECISION-RECOVER",
            Speculation::Admitted,
            StepStatus::Succeeded,
            &["account"][..],
        ),
        (
            "SPEC-DISCARD-LATE-DECISION-RECOVER",
            Speculation::Discarded,
            StepStatus::Skipped,
            &[][..],
        ),
        (
            "SPEC-BLOCK-LATE-DECISION-RECOVER",
            Speculation::Blocked,
            StepStatus::Failed,
            &[][..],
        ),
    ];
    let mut executions = Vec::new();
    for (question, speculation, ..) in cases {
        Mock::given(method("POST"))
            .and(body_string_contains("CONSOLIDATE:"))
            .and(body_string_contains(question))
            .respond_with(done().set_delay(STALLED))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&desk.upstream)
            .await;
        if speculation != Speculation::Admitted {
            Mock::given(method("POST"))
                .and(body_string_contains("EXTRA:"))
                .and(body_string_contains("e-publish"))
                .and(body_string_contains(question))
                .respond_with(done().set_delay(STALLED))
                .with_priority(1)
                .mount(&desk.upstream)
                .await;
        }
        let compare = ["COMPARE:", question];
        held.hold(&compare);
        let execution = desk.submit("speculative-review", json!(question)).await;
        if speculation == Speculation::Admitted {
            let settled = reached(&desk, execution, "extra", |invocation| {
                matches!(invocation.progress, InvocationProgress::Action { .. })
            })
            .await;
            assert_eq!(
                (settled.progress, settled.speculation),
                (
                    InvocationProgress::Action {
                        status: StepStatus::Succeeded
                    },
                    Speculation::None
                ),
                "{question}: the extra review succeeds before the comparison decides"
            );
        } else {
            let deadline = std::time::Instant::now() + Duration::from_secs(90);
            while !desk
                .asked("EXTRA:", question)
                .await
                .iter()
                .any(|messages| turn(messages) == 1)
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "{question}: the extra review never reached its second call"
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let running = reached(&desk, execution, "extra", |_| true).await;
            assert_eq!(
                (running.progress, running.speculation),
                (InvocationProgress::Started, Speculation::None),
                "{question}: the extra review has published and still runs"
            );
        }
        held.release(&compare);
        executions.push(execution);
    }
    let extra = TaskId::new("extra").expect("step");
    let mut before = Vec::new();
    for ((question, speculation, ..), execution) in cases.iter().zip(&executions) {
        desk.stalled_in("CONSOLIDATE:", question).await;
        let accepted = desk
            .accepted(*execution, |state| {
                state.speculation(&extra) != Speculation::None
            })
            .await;
        assert_eq!(accepted.speculation(&extra), *speculation, "{question}");
        before.push((
            desk.asked("COMPARE:", question).await.len(),
            desk.asked("EXTRA:", question).await.len(),
        ));
    }
    let (stalled, desk) = desk.take_over(&executions).await;

    for (((question, speculation, status, entries), execution), called) in
        cases.iter().zip(&executions).zip(&before)
    {
        let result = desk.settled(*execution).await;
        assert_eq!(
            requirement(&result, "consolidated").outcome,
            RequirementOutcome::Passed,
            "{question}: {result:?}"
        );
        assert_eq!(
            step(&desk.deployment, &result, "extra").await.status,
            *status,
            "{question}"
        );
        assert_eq!(
            (
                desk.asked("COMPARE:", question).await.len(),
                desk.asked("EXTRA:", question).await.len(),
            ),
            *called,
            "{question}: the recorded decision is restored, not made again"
        );
        let consolidator = desk.asked("CONSOLIDATE:", question).await;
        assert_eq!(
            consolidator.len(),
            3,
            "{question}: the stalled turn, then a retry"
        );
        let read = results(
            consolidator
                .last()
                .expect("the consolidator called the model"),
        );
        let manifest: Value = serde_json::from_str(&lines(&read["c-extra"]))
            .unwrap_or_else(|error| panic!("{question}: manifest: {error}"));
        let mounted: Vec<&str> = manifest["entries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry["name"].as_str())
            .collect();
        assert_eq!(
            (
                manifest["speculation"].as_str(),
                mounted.as_slice(),
                manifest["invocations"][0]["attempt"].as_u64(),
            ),
            (
                serde_json::to_value(speculation)
                    .ok()
                    .as_ref()
                    .and_then(Value::as_str),
                *entries,
                Some(1),
            ),
            "{question}: {manifest}"
        );
        let retained: Vec<Speculation> = desk
            .publications(&result)
            .await
            .iter()
            .filter(|publication| publication.producer.step_id == extra)
            .map(|publication| publication.speculation)
            .collect();
        assert_eq!(retained, [*speculation], "{question}");
    }

    stalled.shutdown().await;
    for ((question, ..), called) in cases.iter().zip(&before) {
        assert_eq!(
            desk.asked("EXTRA:", question).await.len(),
            called.1,
            "{question}: the stalled worker resurrects nothing"
        );
    }
    desk.deployment.shutdown().await;
}

/// A mock that answers every section-reviewer turn for a section starting
/// with `prefix` as the script does, after `delay`.
async fn hold_sections(upstream: &MockServer, prefix: &str, delay: Duration) {
    let script = Arc::new(Script::default());
    Mock::given(method("POST"))
        .and(body_string_contains(format!("SECTION: {prefix}")))
        .respond_with(move |request: &Request| reply(request, &script).set_delay(delay))
        .with_priority(1)
        .mount(upstream)
        .await;
}

/// The arrival position of every section-reviewer request for each
/// section, in arrival order.
///
/// # Panics
/// Panics when the upstream does not record requests.
async fn section_turns(desk: &Desk) -> BTreeMap<String, Vec<usize>> {
    let mut turns: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (at, messages) in desk.requests("SECTION:").await.iter().enumerate() {
        let prompt = text(messages);
        if let Some((section, _)) = prompt
            .split_once("SECTION: ")
            .and_then(|(_, rest)| rest.split_once(" at="))
        {
            turns.entry(section.to_owned()).or_default().push(at);
        }
    }
    turns
}

/// The `(item_index, attempt)` of every `map` publication of `result`, in
/// item order; publications list in commit order, which concurrent items
/// do not fix.
///
/// # Panics
/// Panics when the execution or a page of its publications cannot be read.
async fn mapped(desk: &Desk, result: &VerificationResult) -> Vec<(Option<u32>, u32)> {
    let mut mapped: Vec<_> = desk
        .publications(result)
        .await
        .iter()
        .filter(|publication| publication.producer.step_id.as_str() == "map")
        .map(|publication| {
            (
                publication.producer.item_index,
                publication.producer.attempt,
            )
        })
        .collect();
    mapped.sort_unstable();
    mapped
}

/// The deployment admits two Agent workspaces across every execution and
/// tenant. Tenant A's three section reviewers, each slow, take both; the
/// third item is refused capacity, so the execution defers once the two
/// running items settle and resumes with only the third, which never runs
/// beside them. Tenant B's single item, submitted while A holds both
/// workspaces, is deferred the same way and completes once capacity frees,
/// and neither tenant spends an attempt or reads the other's work. An
/// execution whose items hold both workspaces indefinitely is cancelled through the CLI
/// and releases them at once to tenant B, and a Verifier's
/// `timeout_seconds` ends a stalled execution long before the deployment's
/// five-minute deadline.
///
/// # Panics
/// Panics when a submission is refused or does not settle, an item runs
/// beside the items holding the ceiling, an attempt is spent or work leaks
/// across tenants, cancellation keeps capacity, or the Verifier timeout is
/// not applied.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn admission_bounds_fanout_across_tenants() {
    let upstream = upstream().await;
    hold_sections(&upstream, "a", HELD).await;
    hold_sections(&upstream, "c", STALLED).await;
    hold_sections(&upstream, "d", STALLED).await;
    let builder = workspaces(&upstream, 2);
    let desk = Desk::start_with(upstream, builder).await;
    let other = desk.tenant("admission-other").await;
    let other_admin = other.client().expect("tenant B has a client").clone();

    let a = desk
        .submit("admission-sweep", json!(["a0", "a1", "a2"]))
        .await;
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while section_turns(&desk).await.len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "two sections never started"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let Submission::Queued(b) = other
        .run()
        .observe()
        .submit("admission-sweep", json!(["b0"]))
        .await
        .expect("tenant B submits")
    else {
        panic!("Run evidence queues an execution");
    };
    let a = desk.settled(a).await;
    let b = Execution::connect_with(*b.id(), &other_admin)
        .await
        .expect("tenant B's execution handle connects")
        .wait(Duration::from_secs(90))
        .await
        .expect("tenant B's execution settles");

    let turns = section_turns(&desk).await;
    assert_eq!(
        turns.values().map(Vec::len).collect::<Vec<_>>(),
        [2, 2, 2, 2],
        "every section runs its two turns exactly once: {turns:?}"
    );
    let (held, deferred): (Vec<_>, Vec<_>) = ["a0", "a1", "a2"]
        .iter()
        .map(|section| &turns[*section])
        .partition(|at| at[0] < 2);
    let released = held.iter().map(|at| at[1]).max().expect("two held items");
    assert_eq!(deferred.len(), 1, "{turns:?}");
    assert!(
        deferred[0][0] > released && turns["b0"][0] > released,
        "nothing starts beside the two items holding the ceiling: {turns:?}"
    );
    for result in [&a, &b] {
        assert_eq!(
            requirement(result, "mapped").outcome,
            RequirementOutcome::Passed,
            "{result:?}"
        );
    }
    assert_eq!(
        step(&desk.deployment, &a, "map").await.output,
        Some(json!([{ "done": true }, { "done": true }, { "done": true }]))
    );
    assert_eq!(
        mapped(&desk, &a).await,
        [(Some(0), 1), (Some(1), 1), (Some(2), 1)],
        "a refused item spends no attempt"
    );
    let b_publications = Execution::connect_with(b.execution_id, &other_admin)
        .await
        .expect("tenant B's execution handle connects")
        .publications(&ExecutionPageQuery::default())
        .await
        .expect("tenant B reads its publications")
        .items;
    assert_eq!(
        b_publications
            .iter()
            .map(|publication| (
                publication.producer.item_index,
                publication.producer.attempt
            ))
            .collect::<Vec<_>>(),
        [(Some(0), 1)]
    );
    assert!(
        Execution::connect_with(b.execution_id, &desk.deployment.admin())
            .await
            .expect("the execution handle connects")
            .status()
            .await
            .is_err(),
        "tenant A cannot read tenant B's execution"
    );

    let stalled = desk.submit("admission-sweep", json!(["c0", "c1"])).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    loop {
        let turns = section_turns(&desk).await;
        if turns.contains_key("c0") && turns.contains_key("c1") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the stalled sections never started"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let cancelled = cli(
        ExecutionVerb::Cancel(ExecutionArg {
            execution_id: stalled,
        }),
        &desk,
    )
    .await;
    assert_eq!(
        cancelled["status"], "cancelled",
        "the CLI cancels the stalled execution"
    );
    let Submission::Queued(freed) = other
        .run()
        .observe()
        .submit("admission-sweep", json!(["b1"]))
        .await
        .expect("tenant B submits")
    else {
        panic!("Run evidence queues an execution");
    };
    let freed = Execution::connect_with(*freed.id(), &other_admin)
        .await
        .expect("tenant B's execution handle connects")
        .wait(Duration::from_secs(60))
        .await
        .expect("cancellation released the workspaces");
    assert_eq!(
        requirement(&freed, "mapped").outcome,
        RequirementOutcome::Passed
    );

    let started = std::time::Instant::now();
    let limited = Execution::connect_with(
        desk.submit("admission-deadline", json!(["d0"])).await,
        &desk.deployment.admin(),
    )
    .await
    .expect("the execution handle connects");
    assert!(
        limited.wait(Duration::from_secs(90)).await.is_err(),
        "a stalled execution past its timeout has no result"
    );
    assert_eq!(
        limited.status().await.expect("the status reads").status,
        VerificationExecutionStatus::TimedOut
    );
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "the Verifier timeout lowers the deployment's deadline"
    );
    desk.deployment.shutdown().await;
}
