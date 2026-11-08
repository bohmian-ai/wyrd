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

use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Execution, ExecutionPageQuery, PublicationSummary};
use wyrd_ai::{RequirementOutcome, StepStatus, VerificationResult};

use crate::hosted_agent_tools::{builder, calls, done, lines, messages, results, text, turn};
use crate::local_development::configure_gateway;
use crate::support::{Deployment, declare, pull};
use crate::verifier_building_blocks::{completion, grants, requirement, step};

/// A final answer carrying `value`.
fn answer(value: &Value) -> ResponseTemplate {
    completion(json!({ "role": "assistant", "content": value.to_string() }))
}

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
/// stylist publishes a partial draft, then breaks.
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
        ]
        .into_iter()
        .find(|(marker, _)| prompt.contains(marker));
        return match decision {
            Some((_, decision)) => answer(&decision),
            None => ResponseTemplate::new(200).set_body_json(json!({ "garbled": true })),
        };
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
        let upstream = MockServer::start().await;
        let script = Arc::new(Script::default());
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(move |request: &Request| reply(request, &script))
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(builder(&upstream)).await;
        configure_gateway(&deployment.admin()).await;
        let cards = deployment.cards().await;
        let desk = declare(
            &cards,
            "cards/verifier_orchestration/orchestration-desk.yaml",
        )
        .await;
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
            state,
        }
    }

    /// Verify `question` with `verifier` now.
    ///
    /// # Panics
    /// Panics when the verify is refused.
    async fn verify(&self, verifier: &str, question: &str) -> VerificationResult {
        self.state
            .run()
            .observe()
            .verify(verifier, json!(question))
            .await
            .expect("the verification runs")
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
/// its own item, never as a fourth reviewer, and starts from empty scratch.
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
    let review = step(&result, "review");
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
    let manifest: Value = serde_json::from_str(&lines(&results(last)["c-manifest"]))
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
    /// Text the failed `security` step's reason carries, when it failed.
    security_reason: Option<&'static str>,
}

/// A planner's decisions select specialists: true runs one, false skips it
/// before its workspace binds and makes no call, and a `null`, missing, or
/// unreadable decision fails it as an evaluation error rather than skipping.
/// The consolidator reads every settled collection whatever its outcome,
/// the stylist's failed attempt keeping its partial draft and its failed
/// status. The quoter, requiring the security account by name, runs only
/// when that account was produced. With no specialist selected and only the
/// specialist judged, the verdict is inconclusive.
///
/// # Panics
/// Panics when a verification is refused or a call, status, collection, or
/// verdict differs.
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
            security_reason: None,
        },
        Case {
            question: "SECURITY-ONLY",
            statuses: [Succeeded, Succeeded, Skipped, Succeeded],
            called: &["GENERAL:", "SECURITY:", "CONSOLIDATE:", "QUOTE:"],
            collections: [("succeeded", &["account"]), ("skipped", &[])],
            security_reason: None,
        },
        Case {
            question: "BOTH",
            statuses: [Succeeded, Succeeded, Failed, Succeeded],
            called: &["GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"],
            collections: [("succeeded", &["account"]), ("failed", &["draft"])],
            security_reason: None,
        },
        Case {
            question: "UNDECIDED",
            statuses: [Succeeded, Failed, Skipped, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("failed", &[]), ("skipped", &[])],
            security_reason: Some("kind (null, bool)"),
        },
        Case {
            question: "FORGOTTEN",
            statuses: [Succeeded, Failed, Skipped, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("failed", &[]), ("skipped", &[])],
            security_reason: Some("security"),
        },
        Case {
            question: "UNPLANNED",
            statuses: [Failed, Failed, Failed, Skipped],
            called: &["GENERAL:", "CONSOLIDATE:"],
            collections: [("failed", &[]), ("failed", &[])],
            security_reason: None,
        },
    ];
    for case in cases {
        let question = case.question;
        let result = desk.verify("specialist-review", question).await;
        let statuses = ["plan", "security", "style", "quote"].map(|id| step(&result, id).status);
        assert_eq!(statuses, case.statuses, "{question}: {result:?}");
        assert_eq!(step(&result, "general").status, Succeeded, "{question}");
        assert_eq!(
            requirement(&result, "consolidated").outcome,
            RequirementOutcome::Passed,
            "{question}: consolidation reads failed and skipped collections"
        );
        for marker in ["GENERAL:", "SECURITY:", "STYLE:", "CONSOLIDATE:", "QUOTE:"] {
            let ran = !desk.asked(marker, question).await.is_empty();
            assert_eq!(ran, case.called.contains(&marker), "{question}: {marker}");
        }
        if let Some(cause) = case.security_reason {
            let security = step(&result, "security");
            assert!(
                security
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains(cause)),
                "{question}: {security:?}"
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
    }

    let unjudged = desk.verify("specialists-only", "NEITHER-ONLY").await;
    assert_eq!(
        (
            step(&unjudged, "security").status,
            requirement(&unjudged, "secured").outcome,
            unjudged.outcome.verdict
        ),
        (
            Skipped,
            RequirementOutcome::NotApplicable,
            wyrd_ai::VerificationVerdict::Inconclusive
        )
    );
    desk.deployment.shutdown().await;
}
