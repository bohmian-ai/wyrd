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
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Execution, ExecutionPageQuery, File, PublicationContent, PublicationName, PublicationSelector,
    PublicationSummary, Speculation, TaskId,
};
use wyrd_ai::{RequirementOutcome, StepStatus, VerificationResult, VerificationVerdict};

use crate::hosted_agent_tools::{builder, calls, done, lines, messages, results, text, turn};
use crate::local_development::configure_gateway;
use crate::support::{Deployment, declare, fixture, pull};
use crate::verifier_building_blocks::{completion, grants, requirement, step};

/// A final answer carrying `value`.
fn answer(value: &Value) -> ResponseTemplate {
    completion(json!({ "role": "assistant", "content": value.to_string() }))
}

/// How long a held reply waits, so the other side of a race settles first.
const HELD: Duration = Duration::from_secs(3);

/// How long an early decision waits, so the bootstrap it races settles
/// first while a held extra review is still running.
const BRIEF: Duration = Duration::from_secs(1);

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
/// stylist publishes a partial draft, then breaks. A `LATE-DECISION`
/// comparison and a `KNOWN` bootstrap hold their replies; an
/// `EARLY-DECISION` comparison waits briefly while its extra review holds
/// its first turn longer. The refiner publishes its report, then scores it
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
    if prompt.contains("BOOTSTRAP:") {
        let reply = answer(&json!({ "done": true }));
        return if prompt.contains("KNOWN") {
            reply.set_delay(HELD)
        } else {
            reply
        };
    }
    if prompt.contains("COMPARE:") {
        let agree = if prompt.contains("ADMIT") {
            json!(false)
        } else if prompt.contains("BLOCK") {
            Value::Null
        } else {
            json!(true)
        };
        let reply = answer(&json!({ "agree": agree }));
        return if prompt.contains("LATE-DECISION") {
            reply.set_delay(HELD)
        } else if prompt.contains("EARLY-DECISION") {
            reply.set_delay(BRIEF)
        } else {
            reply
        };
    }
    if prompt.contains("EXTRA:") {
        return match turn {
            0 => {
                let reply = calls(&[(
                    "e-publish",
                    "publish",
                    json!({ "name": "account", "value": { "extra": true } }),
                )]);
                if prompt.contains("EARLY-DECISION") {
                    reply.set_delay(HELD)
                } else {
                    reply
                }
            }
            _ => done(),
        };
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

    /// Verify the fixture document at `relative` with `verifier` now.
    ///
    /// # Panics
    /// Panics when the upload or the verify is refused.
    async fn verify_document(&self, verifier: &str, relative: &str) -> VerificationResult {
        self.state
            .run()
            .observe()
            .verify(verifier, File::path(fixture(relative)))
            .await
            .expect("the document is uploaded and verified")
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

/// One `speculative-review` question's expected observations.
struct Speculated {
    /// The question, carrying the comparison's and timing's markers.
    question: &'static str,
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

/// After the bootstrap succeeds, the extra review runs while the comparison
/// is pending. A disagreement admits it whether it finished before or after
/// the decision; an agreement discards completed work or cancels running
/// work; a `null` comparison blocks it; a decision known before the extra
/// review is eligible makes no call. The consolidator only ever reads the
/// recorded disposition, never a pending one, and receives the account only
/// when it was admitted, while an authorized reader still inspects the
/// retained excluded account with its disposition.
///
/// # Panics
/// Panics when a verification is refused or a call, status, disposition,
/// mounted entry, or retained publication differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn speculative_disposition_controls_consumer_admission() {
    use StepStatus::{Failed, Skipped, Succeeded};
    let desk = Desk::start().await;
    let cases = [
        Speculated {
            question: "SPEC-ADMIT-LATE-DECISION",
            called: true,
            status: Succeeded,
            speculation: "admitted",
            entries: &["account"],
            retained: Some(Speculation::Admitted),
        },
        Speculated {
            question: "SPEC-ADMIT-EARLY-DECISION",
            called: true,
            status: Succeeded,
            speculation: "admitted",
            entries: &["account"],
            retained: Some(Speculation::Admitted),
        },
        Speculated {
            question: "SPEC-DISCARD-LATE-DECISION",
            called: true,
            status: Skipped,
            speculation: "discarded",
            entries: &[],
            retained: Some(Speculation::Discarded),
        },
        Speculated {
            question: "SPEC-DISCARD-EARLY-DECISION",
            called: true,
            status: Skipped,
            speculation: "discarded",
            entries: &[],
            retained: None,
        },
        Speculated {
            question: "SPEC-BLOCK-LATE-DECISION",
            called: true,
            status: Failed,
            speculation: "blocked",
            entries: &[],
            retained: Some(Speculation::Blocked),
        },
        Speculated {
            question: "SPEC-KNOWN",
            called: false,
            status: Skipped,
            speculation: "discarded",
            entries: &[],
            retained: None,
        },
    ];
    let extra = TaskId::new("extra").expect("step");
    let account = PublicationName::new("account").expect("name");
    for case in cases {
        let question = case.question;
        let result = desk.verify("speculative-review", question).await;
        assert_eq!(
            requirement(&result, "consolidated").outcome,
            RequirementOutcome::Passed,
            "{question}: {result:?}"
        );
        assert_eq!(
            step(&result, "extra").status,
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
            let execution = Execution::connect_with(result.execution_id, &desk.deployment.admin())
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
                .expect("an authorized reader inspects the retained account");
            assert_eq!(
                inspected.content,
                PublicationContent::Json {
                    value: json!({ "extra": true })
                },
                "{question}"
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
    let draft = step(&result, "draft");
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
        let draft = step(&result, "draft");
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
            step(&result, "read").status,
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
/// duplicate section keeping its own index; the empty appendix runs no
/// reviewer and is honestly empty; the flagged document escalates and its
/// failed escalation keeps its partial note; the summarizer refines one
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

    let sections = step(&result, "sections");
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
    let appendix = step(&result, "appendix");
    assert_eq!(
        (appendix.status, appendix.output.clone()),
        (StepStatus::Succeeded, Some(json!([])))
    );
    assert_eq!(step(&result, "escalate").status, StepStatus::Failed);
    let summarize = step(&result, "summarize");
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
    desk.deployment.shutdown().await;
}
