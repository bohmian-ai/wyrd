//! The `card_authoring` story, authored in Rust: Service `finance-release`
//! composes Agents `report-reviewer` and `release-controls`, each verified by
//! its own two-step Verifier (`report-policy`, `release-access`), and the
//! report judge reads the extracted `company-sharing-policy` Artifact. The
//! graph is built in code through `wyrd_ai::authoring` and declared through
//! `Cards::declare` or `WyrdState::declare_with`, which publish a local
//! Artifact first and then declare the graph.
//!
//! The local upstream stands in for the model provider: the policy judge
//! reads `/inputs/policy.json` with `fs.read`, then passes citing the report
//! text and the policy line it read, fails a report naming
//! `BANK-ACCOUNT-DEMO-001`, and returns an inconclusive verdict for a report
//! naming `PROVIDER-UNCERTAIN`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use wyrd_ai::authoring::{
    Action, Agent, Artifact, ArtifactFile, Audit, Binding, CardDefinition, CardRef, Check,
    CodeSource, Component, Data, Experiment, InputSlot, LocalArtifact, Mcp, Model, Operator,
    Prompt, Requirement, Service, Step, Trigger, Verifier,
};
use wyrd_ai::cards::{CardKind, CardRef as RegisteredRef, CardSelector, Cards};
use wyrd_ai::state::{DeclareOptions, WyrdState};
use wyrd_ai::verify::Inputs;
use wyrd_ai::{Locator, RequirementOutcome, StepStatus, VerificationResult, VerificationVerdict};
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::DeclarationOutcomeKind;
use wyrd_testing::server::WyrdTestServer;

use crate::local_development::configure_gateway;
use crate::support::{Deployment, declare, fixture};
use crate::verifier_building_blocks::{completion, requirement, tool_call};

/// The extracted policy file every report judge mounts.
const POLICY_FILE: &str = "extracted/company-sharing-policy.pdf.json";

/// The story's checked-in file at `relative`.
fn story(relative: &str) -> PathBuf {
    fixture(&format!("cards/card_authoring/{relative}"))
}

/// The policy judge's reply to `request`: a `fs.read` of the mounted policy
/// first, then a verdict on the rendered release, citing the report text and
/// the policy line that permits external audits.
fn judge_policy(request: &wiremock::Request) -> ResponseTemplate {
    let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let release = messages
        .iter()
        .find(|message| message["role"] == "user")
        .map(|message| message["content"].to_string())
        .unwrap_or_default();
    let Some(read) = messages.iter().find(|message| message["role"] == "tool") else {
        let call = tool_call(
            "call-policy",
            "fs.read",
            &json!({ "path": "/inputs/policy.json" }),
        );
        return completion(json!({ "role": "assistant", "content": null, "tool_calls": [call] }));
    };
    let output: Value = read["content"]
        .as_str()
        .and_then(|content| serde_json::from_str(content).ok())
        .unwrap_or_default();
    let Some(line) = output["units"].as_array().and_then(|units| {
        units.iter().find(|unit| {
            unit["text"]
                .as_str()
                .is_some_and(|text| text.contains("external-audit"))
        })
    }) else {
        return completion(json!({ "role": "assistant", "content": "the policy did not read" }));
    };
    let citations = json!([
        { "slot": "report", "locator": { "json_pointer": "/text" } },
        { "slot": output["sources"][0], "locator": line["locator"] },
    ]);
    let verdict = if release.contains("PROVIDER-UNCERTAIN") {
        json!({ "outcome": "inconclusive", "reason": "the policy does not settle this release", "citations": [] })
    } else if release.contains("BANK-ACCOUNT-DEMO-001") {
        json!({ "outcome": "failed", "reason": "the report names a bank account identifier", "citations": citations })
    } else {
        json!({ "outcome": "passed", "reason": "an external audit may receive this report", "citations": citations })
    };
    completion(json!({ "role": "assistant", "content": verdict.to_string() }))
}

/// One story deployment: a verifying test server whose gateway calls the
/// scripted policy judge.
struct Story {
    /// The verifying server and its Postgres-backed clients.
    deployment: Deployment,
    /// The scripted model provider the gateway forwards policy-judge calls
    /// to; it records every request.
    upstream: MockServer,
}

impl Story {
    /// Start the server and the scripted judge, and configure the gateway.
    ///
    /// # Panics
    /// Panics when the upstream URL does not parse, the server does not
    /// start, or the gateway cannot be configured.
    async fn start() -> Self {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains("POLICY JUDGE"))
            .respond_with(judge_policy)
            .mount(&upstream)
            .await;
        let deployment = Deployment::start_with(
            WyrdTestServer::builder()
                .with_verification_runtime_for_test()
                .with_gateway_provider_root_for_test(
                    upstream.uri().parse().expect("upstream URL parses"),
                ),
        )
        .await;
        configure_gateway(&deployment.admin()).await;
        Self {
            deployment,
            upstream,
        }
    }

    /// Declare `service` through `WyrdState` as the admin and return its
    /// ready state.
    ///
    /// # Panics
    /// Panics when the graph is refused or its state does not hydrate.
    async fn state(&self, service: &Service) -> WyrdState {
        WyrdState::declare_with(
            service,
            DeclareOptions {
                client: Some(self.deployment.admin()),
                ..DeclareOptions::default()
            },
        )
        .await
        .expect("the authored graph declares and hydrates")
    }

    /// The number of policy-judge requests the upstream has received.
    ///
    /// # Panics
    /// Panics when the upstream is not recording requests.
    async fn judge_calls(&self) -> usize {
        self.upstream
            .received_requests()
            .await
            .expect("the upstream records requests")
            .len()
    }
}

/// A policy-judge Agent `name` at `version`, as the story's Agent files
/// declare it.
fn policy_judge(name: &str, version: &str) -> Agent {
    Agent::new(name, version)
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "openai"}))
        .tool_names(["fs.read"])
        .run_config(json!({"max_iterations": 3}))
        .prompt(json!({
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{
                "role": "user",
                "content": "POLICY JUDGE: read /inputs/policy.json with fs.read. Determine whether the proposed release satisfies the company policy. Cite original request and policy evidence. Report: {{report}} Recipient: {{recipient}} Purpose: {{purpose}} Destination: {{destination}}",
            }],
            "response_format": "verdict",
        }))
}

/// A code step running `entrypoint` of the story's checks over the slots
/// `inputs`, judged by one `steps.<id>.output.<path> == expected`
/// requirement.
fn fact_step(
    id: &str,
    entrypoint: &str,
    inputs: &[&str],
    (requirement, statement, path, expected): (&str, &str, &str, Value),
) -> Step {
    let action = Action::code(CodeSource::file(story("release_checks.py")), entrypoint)
        .inputs(inputs.iter().map(|name| (*name, format!("inputs.{name}"))));
    Step::new(id, action).requirements([Requirement::new(
        requirement,
        statement,
        Check::assert(format!("steps.{id}.output.{path}"), "equals", expected),
    )])
}

/// The recipient, purpose, and destination slots.
fn release_inputs() -> [(&'static str, InputSlot); 3] {
    [
        (
            "recipient",
            InputSlot::json(json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["email", "authorized", "permitted_purposes"],
                "properties": {
                    "email": {"type": "string"},
                    "authorized": {"type": "boolean"},
                    "permitted_purposes": {"type": "array", "items": {"type": "string"}},
                },
            })),
        ),
        ("purpose", InputSlot::json(json!({"type": "string"}))),
        (
            "destination",
            InputSlot::json(json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["url", "encrypted", "restricted_access", "public_sharing"],
                "properties": {
                    "url": {"type": "string"},
                    "encrypted": {"type": "boolean"},
                    "restricted_access": {"type": "boolean"},
                    "public_sharing": {"type": "boolean"},
                },
            })),
        ),
    ]
}

/// `report-policy` at `version`: the identifier scan with the policy judge
/// reading `policy`, then the report context.
fn report_policy(version: &str, judge: &Agent, policy: ArtifactFile) -> Verifier {
    let mut verifier = Verifier::new("report-policy", version)
        .description("The report is fit to share under the company sharing policy.")
        .inputs(release_inputs());
    verifier.add_input(
        "report",
        InputSlot::json(json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["text"],
            "properties": {"text": {"type": "string"}},
        })),
    );
    let mut call = Check::agent(judge.clone()).inputs(
        ["report", "recipient", "purpose", "destination"]
            .map(|name| (name, format!("inputs.{name}"))),
    );
    call.add_workspace("/inputs/policy.json", policy);
    let mut scan = fact_step(
        "policy",
        "report_facts",
        &["report"],
        (
            "no-bank-account-identifier",
            "The report contains no prohibited bank identifier.",
            "contains_bank_identifier",
            json!(false),
        ),
    );
    scan.add_requirement(Requirement::new(
        "sharing-policy-satisfied",
        "The release satisfies the company sharing policy.",
        call,
    ));
    verifier.steps([
        scan,
        fact_step(
            "policy-context",
            "policy_context",
            &["report", "purpose"],
            (
                "usable-report",
                "The proposed release contains report text.",
                "has_report",
                json!(true),
            ),
        ),
    ])
}

/// `release-access` at `version`: independent recipient and destination
/// steps, and an optional `notes` slot.
fn release_access(version: &str) -> Verifier {
    access_verifier(version, false)
}

/// `release-access` at `version` with a `summary` step that depends on and
/// consumes both fact steps' outputs.
fn summarized_access(version: &str) -> Verifier {
    access_verifier(version, true)
}

/// `release-access` at `version`, with the dependent `summary` step when
/// `summarize` is set.
fn access_verifier(version: &str, summarize: bool) -> Verifier {
    let mut verifier = Verifier::new("release-access", version)
        .description("The recipient and destination may receive the report.")
        .inputs(release_inputs());
    verifier
        .add_input(
            "notes",
            InputSlot::json(json!({"type": "string"})).required(false),
        )
        .add_step(fact_step(
            "recipient",
            "recipient_facts",
            &["recipient", "purpose"],
            (
                "recipient-authorized",
                "The recipient is authorized for this purpose.",
                "authorized",
                json!(true),
            ),
        ))
        .add_step(fact_step(
            "destination",
            "destination_facts",
            &["destination"],
            (
                "destination-controls-satisfied",
                "The destination has appropriate access controls.",
                "appropriate",
                json!(true),
            ),
        ));
    if summarize {
        let summary = Action::code(
            CodeSource::file(story("release_checks.py")),
            "summarize_findings",
        )
        .inputs([
            ("recipient", "steps.recipient.output"),
            ("destination", "steps.destination.output"),
        ]);
        verifier.add_step(
            Step::new("summary", summary)
                .depends_on(["recipient", "destination"])
                .requirements([Requirement::new(
                    "release-permitted",
                    "Both the recipient and the destination permit the release.",
                    Check::assert("steps.summary.output.releasable", "equals", true),
                )]),
        );
    }
    verifier
}

/// `finance-release`, with every Card at `version`, verifying
/// `report-reviewer` by `report-policy` reading `policy` and
/// `release-controls` by `access`.
fn finance_release(version: &str, policy: ArtifactFile, access: Verifier) -> Service {
    let reviewer = policy_judge("report-reviewer", version);
    let report = report_policy(version, &reviewer, policy);
    Service::new("finance-release", version)
        .service_type("api")
        .components([
            Component::new("report-reviewer", reviewer).verified_by([report]),
            Component::new(
                "release-controls",
                policy_judge("release-controls", version),
            )
            .verified_by([access]),
        ])
}

/// The local policy Artifact `version`, published from `source` with its
/// documents extracted.
fn local_policy(version: &str, source: &Path) -> Artifact {
    Artifact::from_path("company-sharing-policy", version, source).extract(true)
}

/// The registered policy Artifact `version`'s extracted file, used without
/// uploading.
fn registered_policy(version: &str) -> ArtifactFile {
    CardRef::artifact("company-sharing-policy", version).file(POLICY_FILE)
}

/// The exact registered reference of `kind` `name` at `version`.
///
/// # Panics
/// Panics when `name` or `version` is not a valid reference part.
fn card_ref(kind: CardKind, name: &str, version: &str) -> RegisteredRef {
    serde_json::from_value(json!({ "kind": kind, "name": name, "version": version }))
        .expect("card reference parses")
}

/// The baseline story graph over `policy`.
fn baseline(policy: ArtifactFile) -> Service {
    finance_release("1.0.0", policy, release_access("1.0.0"))
}

/// The one local Artifact `service` publishes first.
///
/// # Panics
/// Panics when the graph is invalid or does not publish exactly one local
/// Artifact.
fn published_artifact(service: &Service) -> LocalArtifact {
    let mut artifacts = CardDefinition::from(service)
        .declaration()
        .expect("valid graph")
        .artifacts;
    assert_eq!(artifacts.len(), 1, "one local Artifact publishes first");
    artifacts.remove(0)
}

/// The permitted release: an external audit of a clean report to an
/// authorized auditor over a controlled destination.
fn permitted() -> Value {
    json!({
        "report": { "text": "Synthetic customer report: quarterly total $120." },
        "recipient": {
            "email": "auditor@example.test",
            "authorized": true,
            "permitted_purposes": ["external-audit"],
        },
        "purpose": "external-audit",
        "destination": {
            "url": "https://auditor.example.test/reports",
            "encrypted": true,
            "restricted_access": true,
            "public_sharing": false,
        },
    })
}

/// The slots of `request` named by `slots`, as named inputs.
fn inputs(request: &Value, slots: &[&str]) -> Inputs {
    Inputs::named(
        slots
            .iter()
            .filter_map(|slot| request.get(*slot).map(|value| (*slot, value.clone()))),
    )
}

/// Verify `request` against both components, each through its own Run
/// subject and binding.
///
/// # Errors
/// Each result carries its own verification's refusal, such as invalid
/// input or an unbound Verifier; one refusal does not stop the other
/// component's verification.
///
/// # Panics
/// Panics when either component's Run cannot open.
async fn verify_both(
    state: &WyrdState,
    request: &Value,
) -> (
    Result<VerificationResult, WyrdError>,
    Result<VerificationResult, WyrdError>,
) {
    let report = state
        .run_for_card("report-reviewer")
        .expect("report-reviewer run opens")
        .observe()
        .verify(
            "report-policy",
            inputs(request, &["report", "recipient", "purpose", "destination"]),
        )
        .await;
    let access = state
        .run_for_card("release-controls")
        .expect("release-controls run opens")
        .observe()
        .verify(
            "release-access",
            inputs(request, &["recipient", "purpose", "destination", "notes"]),
        )
        .await;
    (report, access)
}

/// Whether the simulated release may proceed: only both passed verdicts
/// permit it.
fn releases(report: &VerificationResult, access: &VerificationResult) -> bool {
    report.outcome.verdict == VerificationVerdict::Passed
        && access.outcome.verdict == VerificationVerdict::Passed
}

/// The outcome of requirement `id` in `result`.
///
/// # Panics
/// Panics when `result` has no requirement `id`, as `requirement` does.
fn outcome(result: &VerificationResult, id: &str) -> RequirementOutcome {
    requirement(result, id).outcome
}

/// Assert `result` judged the component `alias` by `verifier` and executed
/// exactly the steps `steps`, in any stage order.
///
/// # Panics
/// Panics when the subject, Verifier, or executed steps differ, or `alias`
/// is not a component of `state`.
fn assert_attributed(
    result: &VerificationResult,
    state: &WyrdState,
    alias: &str,
    verifier: &str,
    steps: &[&str],
) {
    let subject = state.card_ref(alias).expect("component alias resolves");
    assert_eq!(&result.subject.name, &subject.name, "{result:#?}");
    assert_eq!(&result.subject.version, &subject.version, "{result:#?}");
    assert_eq!(result.verifier.name.as_str(), verifier, "{result:#?}");
    let executed: std::collections::BTreeSet<&str> = result
        .outcome
        .steps
        .iter()
        .map(|step| step.id.as_str())
        .collect();
    assert_eq!(executed, steps.iter().copied().collect(), "{result:#?}");
}

/// The authored two-component Service declares through `WyrdState` and
/// each component is verified under its own subject by its own two-step
/// Verifier: the permitted release passes both and releases, each prohibited
/// variant fails only the Verifier that owns it, missing evidence and an
/// uncertain provider are inconclusive and hold, a malformed report and the
/// other component's Verifier are refused, and an unchanged rerun declares
/// the same identities.
///
/// # Panics
/// Panics when declaration fails or any case differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn multi_component_service_verifies_each_subject() {
    let story_server = Story::start().await;
    let policy = local_policy("1.0.0", &story("company-sharing-policy.pdf"));
    let service = baseline(policy.file(POLICY_FILE));
    let state = story_server.state(&service).await;

    let (report, access) = verify_both(&state, &permitted()).await;
    let (report, access) = (
        report.expect("report-policy runs"),
        access.expect("release-access runs"),
    );
    assert_attributed(
        &report,
        &state,
        "report-reviewer",
        "report-policy",
        &["policy", "policy-context"],
    );
    assert_attributed(
        &access,
        &state,
        "release-controls",
        "release-access",
        &["recipient", "destination"],
    );
    assert!(
        report
            .outcome
            .steps
            .iter()
            .chain(&access.outcome.steps)
            .all(|step| step.status == StepStatus::Succeeded),
        "{report:#?} {access:#?}"
    );
    assert!(releases(&report, &access), "{report:#?} {access:#?}");
    assert_eq!(
        story_server.judge_calls().await,
        2,
        "only report-policy's judge runs: one policy read, then its verdict"
    );
    let judged = requirement(&report, "sharing-policy-satisfied");
    assert_eq!(judged.outcome, RequirementOutcome::Passed, "{judged:#?}");
    assert!(
        judged
            .citations
            .iter()
            .any(|citation| citation.slot.as_str() == "report"
                && citation.locator == Locator::JsonPointer("/text".to_owned())),
        "{judged:#?}"
    );
    assert!(
        judged.citations.iter().any(|citation| {
            citation.slot.as_str().starts_with("attachment_")
                && matches!(citation.locator, Locator::Lines { .. })
        }),
        "the judge cites the policy line it read: {judged:#?}"
    );

    let mut prohibited = permitted();
    prohibited["report"]["text"] =
        json!("Synthetic report. Bank identifier: BANK-ACCOUNT-DEMO-001.");
    let mut unauthorized = permitted();
    unauthorized["recipient"]["authorized"] = json!(false);
    let mut public = permitted();
    public["destination"]["public_sharing"] = json!(true);
    for (request, report_verdict, access_verdict, failed) in [
        (
            &prohibited,
            VerificationVerdict::Failed,
            VerificationVerdict::Passed,
            "no-bank-account-identifier",
        ),
        (
            &unauthorized,
            VerificationVerdict::Passed,
            VerificationVerdict::Failed,
            "recipient-authorized",
        ),
        (
            &public,
            VerificationVerdict::Passed,
            VerificationVerdict::Failed,
            "destination-controls-satisfied",
        ),
    ] {
        let (report, access) = verify_both(&state, request).await;
        let (report, access) = (
            report.expect("report-policy runs"),
            access.expect("release-access runs"),
        );
        assert_eq!(report.outcome.verdict, report_verdict, "{report:#?}");
        assert_eq!(access.outcome.verdict, access_verdict, "{access:#?}");
        let owner = if report_verdict == VerificationVerdict::Failed {
            &report
        } else {
            &access
        };
        assert_eq!(
            outcome(owner, failed),
            RequirementOutcome::Failed,
            "{owner:#?}"
        );
        assert!(!releases(&report, &access), "a failure holds the release");
    }

    let mut missing = permitted();
    missing.as_object_mut().expect("object").remove("report");
    let (report, _) = verify_both(&state, &missing).await;
    let report = report.expect("a missing report is judged, not refused");
    assert_eq!(
        report.outcome.verdict,
        VerificationVerdict::Inconclusive,
        "{report:#?}"
    );
    for id in ["no-bank-account-identifier", "usable-report"] {
        assert_eq!(
            outcome(&report, id),
            RequirementOutcome::Inconclusive,
            "{report:#?}"
        );
    }
    assert!(
        report
            .outcome
            .requirements
            .iter()
            .all(|requirement| requirement.outcome != RequirementOutcome::Failed),
        "no policy failure is fabricated: {report:#?}"
    );

    let mut uncertain = permitted();
    uncertain["report"]["text"] = json!("Synthetic report flagged PROVIDER-UNCERTAIN.");
    let (report, access) = verify_both(&state, &uncertain).await;
    let (report, access) = (
        report.expect("report-policy runs"),
        access.expect("release-access runs"),
    );
    assert_eq!(
        report.outcome.verdict,
        VerificationVerdict::Inconclusive,
        "{report:#?}"
    );
    assert_eq!(
        outcome(&report, "sharing-policy-satisfied"),
        RequirementOutcome::Inconclusive
    );
    for id in ["no-bank-account-identifier", "usable-report"] {
        assert_eq!(
            outcome(&report, id),
            RequirementOutcome::Passed,
            "{report:#?}"
        );
    }
    assert!(!releases(&report, &access), "uncertainty holds the release");

    let mut malformed = permitted();
    malformed["report"]["text"] = json!(123);
    let (report, _) = verify_both(&state, &malformed).await;
    let refused = report.expect_err("a malformed report is refused, not judged");
    assert_eq!(
        refused.code(),
        "WYRD_VERIFICATION_400_INPUT_INVALID",
        "{refused:?}"
    );

    for (alias, verifier) in [
        ("report-reviewer", "release-access"),
        ("release-controls", "report-policy"),
    ] {
        let refused = state
            .run_for_card(alias)
            .expect("component run opens")
            .observe()
            .verify(
                verifier,
                inputs(&permitted(), &["recipient", "purpose", "destination"]),
            )
            .await
            .expect_err("another component's Verifier is not bound here");
        assert_eq!(
            refused.code(),
            "WYRD_SDK_404_UNKNOWN_VERIFIER",
            "{refused:?}"
        );
    }

    let rerun = story_server
        .deployment
        .cards()
        .await
        .declare(&service)
        .await
        .expect("an unchanged rerun declares");
    assert_eq!(rerun.root.name.as_str(), "finance-release");
    assert!(
        rerun
            .outcomes
            .iter()
            .all(|outcome| outcome.outcome == DeclarationOutcomeKind::IdempotentNoop),
        "an unchanged rerun declares nothing new: {rerun:#?}"
    );
    let again = story_server.state(&service).await;
    for alias in ["report-reviewer", "release-controls"] {
        assert_eq!(
            again.card_ref(alias).expect("alias resolves"),
            state.card_ref(alias).expect("alias resolves"),
        );
    }
    story_server.deployment.shutdown().await;
}

/// The kinds `receipt` declared.
fn declared_kinds(receipt: &wyrd_ai::cards::DeclarationReceipt) -> Vec<CardKind> {
    receipt
        .outcomes
        .iter()
        .map(|outcome| outcome.card_ref.kind.clone())
        .collect()
}

/// Whether Artifact `version` of the policy is registered and holds its
/// extracted policy file.
///
/// # Panics
/// Panics when `version` is not an exact version, as `card_ref` does,
/// before the server is asked.
async fn policy_published(cards: &Cards, version: &str) -> bool {
    cards
        .list_artifacts(CardSelector::exact(card_ref(
            CardKind::Artifact,
            "company-sharing-policy",
            version,
        )))
        .await
        .is_ok_and(|files| {
            files
                .artifacts
                .iter()
                .any(|file| file.relative_path.as_str() == POLICY_FILE)
        })
}

/// The policy version the registered `report-policy` `version` mounts.
///
/// # Panics
/// Panics when `report-policy` `version` cannot be read.
async fn mounted_policy(cards: &Cards, version: &str) -> Value {
    let card = cards
        .get(CardSelector::exact(card_ref(
            CardKind::Verifier,
            "report-policy",
            version,
        )))
        .await
        .expect("report-policy reads");
    let spec = serde_json::to_value(&card.spec).expect("spec serializes");
    spec["steps"][0]["requirements"][1]["check"]["workspace"]["/inputs/policy.json"]["artifact"]
        ["card"]["version"]
        .clone()
}

/// A local policy publishes first, alone, through `Cards`; a registered
/// policy reference declares through `WyrdState` with nothing uploaded and
/// is mounted by the judge; a new local policy version through `WyrdState`
/// leaves the old graph on the old policy; changed bytes under a published
/// version, an unreadable source, and an unregistered reference are
/// refused; and a graph refused after its policy published keeps that
/// policy, which a later graph reuses by reference.
///
/// # Panics
/// Panics when a declaration, verification, or read differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn local_artifacts_reuse_and_keep_versions() {
    let story_server = Story::start().await;
    let cards = story_server.deployment.cards().await;
    let source = story("company-sharing-policy.pdf");
    let local = local_policy("1.0.0", &source);
    let authored = baseline(local.file(POLICY_FILE));
    let declaration = CardDefinition::from(&authored)
        .declaration()
        .expect("valid graph");
    let [published] = declaration.artifacts.as_slice() else {
        panic!("one local Artifact publishes first: {declaration:#?}");
    };
    assert_eq!(published.source, source);
    assert!(published.extract);
    assert_eq!(published.metadata.name.as_str(), local.card_ref().name());
    assert!(
        declaration
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions
            .iter()
            .all(|submission| submission.kind != CardKind::Artifact
                && submission.artifacts.is_empty()),
        "the graph request carries no heavy submission"
    );

    let receipt = cards
        .declare(&authored)
        .await
        .expect("the local policy publishes and the graph declares");
    assert_eq!(receipt.root.name.as_str(), "finance-release");
    assert!(
        !declared_kinds(&receipt).contains(&CardKind::Artifact),
        "{receipt:#?}"
    );
    assert!(policy_published(&cards, "1.0.0").await);
    assert_eq!(mounted_policy(&cards, "1.0.0").await, json!("1.0.0"));

    let reused = baseline(registered_policy("1.0.0"));
    assert!(
        CardDefinition::from(&reused)
            .declaration()
            .expect("valid graph")
            .artifacts
            .is_empty(),
        "a registered reference prepares and uploads nothing"
    );
    let state = story_server.state(&reused).await;
    for _ in 0..2 {
        let (report, access) = verify_both(&state, &permitted()).await;
        let (report, access) = (
            report.expect("report-policy runs"),
            access.expect("release-access runs"),
        );
        assert!(releases(&report, &access), "{report:#?} {access:#?}");
        assert_eq!(report.verifier.version.as_str(), "1.0.0");
    }

    let unregistered = story_server
        .deployment
        .cards()
        .await
        .declare(&finance_release(
            "9.0.0",
            registered_policy("9.0.0"),
            release_access("9.0.0"),
        ))
        .await
        .expect_err("an unregistered policy reference is not uploaded");
    assert_eq!(
        unregistered.code(),
        "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY",
        "{unregistered:?}"
    );
    assert!(!policy_published(&cards, "9.0.0").await, "{unregistered:?}");

    let revised = tempfile::tempdir().expect("revision directory creates");
    let revised_source = revised.path().join("company-sharing-policy.pdf");
    let mut bytes = std::fs::read(&source).expect("policy reads");
    bytes.extend_from_slice(b"% revision 2\n");
    std::fs::write(&revised_source, bytes).expect("revised policy writes");
    let revised_state = story_server
        .state(&finance_release(
            "2.0.0",
            local_policy("2.0.0", &revised_source).file(POLICY_FILE),
            release_access("2.0.0"),
        ))
        .await;
    assert!(policy_published(&cards, "2.0.0").await);
    assert_eq!(mounted_policy(&cards, "2.0.0").await, json!("2.0.0"));
    assert_eq!(mounted_policy(&cards, "1.0.0").await, json!("1.0.0"));
    let (revised_report, _) = verify_both(&revised_state, &permitted()).await;
    assert_eq!(
        revised_report
            .expect("report-policy runs")
            .verifier
            .version
            .as_str(),
        "2.0.0"
    );
    let (old_report, _) = verify_both(&state, &permitted()).await;
    assert_eq!(
        old_report
            .expect("report-policy runs")
            .verifier
            .version
            .as_str(),
        "1.0.0",
        "the old state keeps the old policy"
    );

    let drifted = cards
        .declare(&baseline(
            local_policy("1.0.0", &revised_source).file(POLICY_FILE),
        ))
        .await
        .expect_err("changed bytes under a published version are refused");
    assert_eq!(
        drifted.code(),
        "WYRD_REGISTRY_409_SPEC_DRIFT",
        "{drifted:?}"
    );

    let unreadable = cards
        .declare(&finance_release(
            "4.0.0",
            local_policy("4.0.0", &revised.path().join("missing.pdf")).file(POLICY_FILE),
            release_access("4.0.0"),
        ))
        .await
        .expect_err("an unreadable source is refused");
    assert_eq!(
        unreadable.code(),
        "WYRD_LOADER_400_INVALID_ENVELOPE",
        "{unreadable:?}"
    );
    assert!(
        cards
            .get(CardSelector::exact(card_ref(
                CardKind::Service,
                "finance-release",
                "4.0.0"
            )))
            .await
            .is_err()
    );

    let judge = policy_judge("report-reviewer", "3.0.0");
    let policy = local_policy("3.0.0", &source);
    let orphaned = Service::new("finance-release", "3.0.0").components([
        Component::new("report-reviewer", judge.clone()).verified_by([report_policy(
            "3.0.0",
            &judge,
            policy.file(POLICY_FILE),
        )]),
        Component::new(
            "release-controls",
            CardRef::agent("unregistered-controls", "1.0.0"),
        )
        .verified_by([release_access("3.0.0")]),
    ]);
    let refused = cards
        .declare(&orphaned)
        .await
        .expect_err("the graph names an unregistered component");
    assert_eq!(
        refused.code(),
        "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY",
        "the server, not local validation, refuses the graph: {refused:?}"
    );
    assert!(
        policy_published(&cards, "3.0.0").await,
        "the published policy outlives the refused graph: {refused:?}"
    );
    let recovered = cards
        .declare(&finance_release(
            "3.0.0",
            registered_policy("3.0.0"),
            release_access("3.0.0"),
        ))
        .await
        .expect("the retained policy is reused by reference");
    assert!(
        !declared_kinds(&recovered).contains(&CardKind::Artifact),
        "{recovered:#?}"
    );
    assert_eq!(mounted_policy(&cards, "3.0.0").await, json!("3.0.0"));
    story_server.deployment.shutdown().await;
}

/// Each declared Card of `receipt`, by name, with its resolved identity and
/// spec hash.
fn resolved(
    receipt: &wyrd_ai::cards::DeclarationReceipt,
) -> std::collections::BTreeMap<String, (RegisteredRef, String)> {
    receipt
        .outcomes
        .iter()
        .map(|outcome| {
            (
                outcome.card_ref.name.to_string(),
                (outcome.card_ref.clone(), outcome.spec_hash.clone()),
            )
        })
        .collect()
}

/// The checked-in files and the code-authored graph register the same
/// content under the same identities: the policy publishes once, the
/// authored graph is an idempotent rerun of the file graph through `Cards`,
/// and `WyrdState` hydrates the same component identities from either
/// form. Changed content under a published version and a caller without
/// the declare grant keep their existing refusals through both entry
/// points.
///
/// # Panics
/// Panics when a declaration differs or a refusal carries another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn declarations_match_files_and_keep_refusals() {
    let story_server = Story::start().await;
    let deployment = &story_server.deployment;
    let cards = deployment.cards().await;
    let source = story("company-sharing-policy.pdf");
    let local = local_policy("1.0.0", &source);
    let authored = baseline(local.file(POLICY_FILE));
    let parts = published_artifact(&authored);
    let published = cards
        .upload_artifact(
            &parts.source,
            parts.metadata.clone(),
            parts.spec.clone(),
            parts.extract,
        )
        .await
        .expect("the policy file publishes");
    let files = declare(&cards, "cards/card_authoring/finance-release.yaml").await;

    let receipt = cards
        .declare(&authored)
        .await
        .expect("the authored graph declares");
    assert_eq!(receipt.root, files.root);
    assert_eq!(
        resolved(&receipt),
        resolved(&files),
        "same identities and content"
    );
    assert!(
        receipt
            .outcomes
            .iter()
            .all(|outcome| outcome.outcome == DeclarationOutcomeKind::IdempotentNoop),
        "the authored graph is the file graph: {receipt:#?}"
    );
    let reused = cards
        .declare(&baseline(registered_policy("1.0.0")))
        .await
        .expect("the reference form declares");
    assert_eq!(resolved(&reused), resolved(&files));
    assert_eq!(
        cards
            .upload_artifact(
                &parts.source,
                parts.metadata.clone(),
                parts.spec.clone(),
                parts.extract
            )
            .await
            .expect("the unchanged policy republishes"),
        published,
        "the authored publication kept the file publication's identity"
    );

    let from_code = story_server.state(&authored).await;
    let from_file = WyrdState::declare_with(
        story("finance-release.yaml"),
        DeclareOptions {
            client: Some(deployment.admin()),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect("the path form still declares");
    for alias in ["report-reviewer", "release-controls"] {
        assert_eq!(
            from_code.card_ref(alias).expect("alias resolves"),
            from_file.card_ref(alias).expect("alias resolves"),
        );
    }

    let edited = release_access("1.0.0").description("Changed after publication.");
    let changed = finance_release("1.0.0", registered_policy("1.0.0"), edited);
    let drift = cards
        .declare(&changed)
        .await
        .expect_err("changed content under a published version is refused");
    assert_eq!(drift.code(), "WYRD_REGISTRY_409_SPEC_DRIFT", "{drift:?}");
    let drift = WyrdState::declare_with(
        &changed,
        DeclareOptions {
            client: Some(deployment.admin()),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect_err("changed content under a published version is refused");
    assert_eq!(drift.code(), "WYRD_REGISTRY_409_SPEC_DRIFT", "{drift:?}");

    let reader = deployment.client(&deployment.scoped_key("card_reader", &["cards:read"]).await);
    let refused = Cards::connect_with(&reader)
        .await
        .expect("the reader connects")
        .declare(&authored)
        .await
        .expect_err("a reader cannot declare");
    assert_eq!(
        refused.code(),
        "WYRD_PERMISSION_403_DENIED_RBAC",
        "{refused:?}"
    );
    let refused = WyrdState::declare_with(
        &authored,
        DeclareOptions {
            client: Some(reader),
            ..DeclareOptions::default()
        },
    )
    .await
    .expect_err("a reader cannot declare");
    assert_eq!(
        refused.code(),
        "WYRD_PERMISSION_403_DENIED_RBAC",
        "{refused:?}"
    );
    story_server.deployment.shutdown().await;
}

/// A dependent `summary` step added to `release-access` keeps its explicit
/// dependencies through registration and runs after both fact steps,
/// consuming their outputs: a controlled release summarizes as releasable
/// and passes, a public destination summarizes as not releasable and fails.
///
/// # Panics
/// Panics when declaration or verification fails, or an output differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn dependent_steps_consume_prior_outputs() {
    let story_server = Story::start().await;
    let cards = story_server.deployment.cards().await;
    let policy = local_policy("1.0.0", &story("company-sharing-policy.pdf"));
    let service = finance_release(
        "1.1.0",
        policy.file(POLICY_FILE),
        summarized_access("1.1.0"),
    );
    let state = story_server.state(&service).await;
    let registered = cards
        .get(CardSelector::exact(card_ref(
            CardKind::Verifier,
            "release-access",
            "1.1.0",
        )))
        .await
        .expect("release-access reads");
    let spec = serde_json::to_value(&registered.spec).expect("spec serializes");
    assert_eq!(spec["steps"][2]["id"], "summary");
    assert_eq!(
        spec["steps"][2]["depends_on"],
        json!(["recipient", "destination"])
    );
    assert_eq!(
        spec["steps"][2]["action"]["inputs"],
        json!({"recipient": "steps.recipient.output", "destination": "steps.destination.output"}),
    );

    let mut public = permitted();
    public["destination"]["public_sharing"] = json!(true);
    for (request, releasable, verdict) in [
        (permitted(), true, VerificationVerdict::Passed),
        (public, false, VerificationVerdict::Failed),
    ] {
        let (_, access) = verify_both(&state, &request).await;
        let access = access.expect("release-access runs");
        assert_attributed(
            &access,
            &state,
            "release-controls",
            "release-access",
            &["recipient", "destination", "summary"],
        );
        let summary = access
            .outcome
            .steps
            .iter()
            .find(|step| step.id.as_str() == "summary")
            .expect("summary ran");
        assert_eq!(summary.status, StepStatus::Succeeded, "{summary:#?}");
        assert_eq!(
            summary.output,
            Some(json!({
                "releasable": releasable,
                "recipient": {"authorized": true},
                "destination": {"appropriate": releasable},
            })),
            "{access:#?}"
        );
        let expected = if releasable {
            RequirementOutcome::Passed
        } else {
            RequirementOutcome::Failed
        };
        assert_eq!(
            outcome(&access, "release-permitted"),
            expected,
            "{access:#?}"
        );
        assert_eq!(access.outcome.verdict, verdict, "{access:#?}");
    }
    story_server.deployment.shutdown().await;
}

/// Every native comparison operator, in its canonical wire form.
fn operators() -> Vec<Value> {
    let named = [
        "equals",
        "not_equals",
        "greater_than",
        "greater_than_or_equals",
        "less_than",
        "less_than_or_equals",
        "is_positive",
        "is_negative",
        "is_zero",
        "contains",
        "not_contains",
        "contains_ignore_case",
        "starts_with",
        "ends_with",
        "is_email",
        "is_url",
        "is_uuid",
        "is_ipv4",
        "is_ipv6",
        "is_json",
        "in",
        "not_in",
        "is_subset",
        "is_superset",
        "is_disjoint",
        "all_of",
        "any_of",
        "none_of",
        "is_empty",
        "is_non_empty",
        "unique_values",
        "is_truthy",
        "is_falsy",
        "is_null",
        "is_not_null",
        "is_string",
        "is_number",
        "is_boolean",
        "is_object",
    ];
    let parameterized = [
        json!({"kind": "in_range", "min": 0.0, "max": 1.0, "inclusive": true}),
        json!({"kind": "not_in_range", "min": 0.0, "max": 1.0, "inclusive": false}),
        json!({"kind": "approximately_equals", "tolerance": 0.5}),
        json!({"kind": "matches_regex", "pattern": "^r"}),
        json!({"kind": "not_matches_regex", "pattern": "^r"}),
        json!({"kind": "has_min_length", "min": 1}),
        json!({"kind": "has_max_length", "max": 9}),
        json!({"kind": "length", "expected": 2}),
        json!({"kind": "length_greater_than", "min": 1}),
        json!({"kind": "length_less_than", "max": 9}),
        json!({"kind": "is_type", "expected": "object"}),
        json!({"kind": "within_abs_tolerance", "tolerance": 0.5}),
        json!({"kind": "within_pct_tolerance", "pct": 5.0}),
        json!({"kind": "within_std_dev", "sigma": 2.0, "mean": 0.0, "std_dev": 1.0}),
        json!({"kind": "between_percentiles", "lower_pct": 5.0, "upper_pct": 95.0}),
        json!({"kind": "divergence_less_than", "metric": "js", "threshold": 0.1}),
        json!({"kind": "cosine_similarity_at_least", "threshold": 0.9}),
    ];
    named
        .into_iter()
        .map(Value::from)
        .chain(parameterized)
        .collect()
}

/// `kind`'s canonical spec decoded from its authored JSON form.
///
/// # Panics
/// Panics when `value` is not a valid `kind` spec.
fn canonical(kind: &CardKind, value: Value) -> wyrd_ai::Spec {
    wyrd_ai::Spec::from_kind_and_value(kind, value).expect("canonical spec decodes")
}

/// Assert `card` authors the canonical `kind` spec `expected` and declares
/// alone as one `kind` submission.
///
/// # Panics
/// Panics when the spec differs or the one-Card graph does not declare.
fn assert_authors(card: impl Into<CardDefinition>, kind: &CardKind, expected: Value) {
    let card = card.into();
    assert_eq!(&card.card_ref().kind().clone(), kind);
    assert_eq!(
        canonical(kind, card.spec()),
        canonical(kind, expected),
        "{kind:?}"
    );
    let declaration = card.declaration().expect("one-Card graph declares");
    assert_eq!(
        declaration
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions
            .iter()
            .map(|submission| submission.kind.clone())
            .collect::<Vec<_>>(),
        std::slice::from_ref(kind),
    );
}

/// Through `wyrd_ai` paths alone, every native authoring family builds the
/// canonical spec its file form decodes to, with whole-field setters and add
/// methods alike: every named kind and the generic kinds, all 56 comparison
/// operators in assert, each, and trace, every Action and Check, every input
/// slot including group sources and requiredness independent of whether its
/// schema admits `null`, Service components, and a binding's Trigger and both
/// Operator lists in each linking form. Nothing here contacts a server.
///
/// # Panics
/// Panics when an authored value differs from its canonical form, the
/// shared validation refuses one, or a declaration omits a submission.
#[test]
fn public_paths_author_every_native_family() {
    let data = json!({
        "interface": {"kind": "Parquet", "meta": {"compression": "Snappy"}},
        "schema": {"columns": [{"name": "latency", "dtype": "float64"}]},
        "stats": {"row_count": 1, "col_count": 1, "byte_count": 1, "sha256": "c4f7700b380aff2662e88d034154d1890c33d3c12c8dda1a0ca9481f43bb9906"},
    });
    assert_authors(
        Data::new("authored", "1.0.0")
            .interface(data["interface"].clone())
            .schema(data["schema"].clone())
            .stats(data["stats"].clone()),
        &CardKind::Data,
        data,
    );
    let model = json!({
        "interface": {"kind": "Custom", "meta": {"framework_version": "0.1.0", "loader_module": "assistant", "loader_class": "LatencyModel", "extra": {}}},
        "task_type": "Other",
        "signature": {"inputs": [{"name": "latency", "dtype": "float64"}], "outputs": [{"name": "score", "dtype": "float64"}]},
    });
    assert_authors(
        Model::new("authored", "1.0.0")
            .interface(model["interface"].clone())
            .task_type("Other")
            .signature(model["signature"].clone()),
        &CardKind::Model,
        model,
    );
    let mut experiment = Experiment::new("authored", "1.0.0")
        .experiment_type("ab")
        .description("Prompt comparison");
    experiment.add_target_ref(CardRef::model("ranker", "1.0.0"));
    assert_authors(
        experiment,
        &CardKind::Experiment,
        json!({"experiment_type": "ab", "description": "Prompt comparison", "target_refs": [{"kind": "Model", "name": "ranker", "version": "1.0.0"}]}),
    );
    let mut mcp = Mcp::new("authored", "1.0.0")
        .server_name("files")
        .transport("stdio");
    mcp.add_scope("read");
    let whole = Mcp::new("authored", "1.0.0")
        .server_name("files")
        .transport("stdio")
        .scopes(["read"]);
    assert_eq!(mcp, whole, "an add method equals the whole-field setter");
    assert_authors(
        mcp,
        &CardKind::Mcp,
        json!({"server_name": "files", "transport": "stdio", "scopes": ["read"]}),
    );
    assert_authors(
        Audit::new("authored", "1.0.0").description("Quarterly review"),
        &CardKind::Audit,
        json!({"description": "Quarterly review"}),
    );
    assert_authors(
        Trigger::new("authored", "1.0.0")
            .kind("schedule")
            .cron("0 0 * * *"),
        &CardKind::Trigger,
        json!({"kind": "schedule", "cron": "0 0 * * *"}),
    );
    let pager =
        json!({"kind": "http", "method": "post", "url": "https://hooks.example.com/release"});
    assert_authors(
        Operator::new("authored", "1.0.0").action(pager.clone()),
        &CardKind::Operator,
        pager.clone(),
    );
    let mut prompt = Prompt::new("authored", "1.0.0")
        .provider("openai")
        .model("gpt-4o");
    prompt.add_message(json!({"role": "user", "content": "Judge {{report}}"}));
    assert_authors(
        prompt,
        &CardKind::Prompt,
        json!({"provider": "openai", "model": "gpt-4o", "messages": [{"role": "user", "content": "Judge {{report}}"}]}),
    );
    let mut artifact = Artifact::new("authored", "1.0.0")
        .artifact_kind("document")
        .content_type("application/pdf");
    artifact
        .add_metadata("owner", json!({"type": "str", "value": "finance"}))
        .add_label("team", "finance")
        .add_annotation("reviewed", "yes");
    assert_authors(
        &artifact,
        &CardKind::Artifact,
        json!({"artifact_kind": "document", "content_type": "application/pdf", "metadata": {"owner": {"type": "str", "value": "finance"}}}),
    );
    let declared = CardDefinition::from(artifact.clone())
        .declaration()
        .expect("a spec-only Artifact root declares")
        .input
        .expect("an ordinary graph");
    assert_eq!(
        json!({
            "labels": declared.submissions[0].metadata.labels,
            "annotations": declared.submissions[0].metadata.annotations,
        }),
        json!({"labels": {"team": "finance"}, "annotations": {"reviewed": "yes"}}),
        "root adaptation keeps the envelope metadata"
    );
    let source = story("company-sharing-policy.pdf");
    let [local] = CardDefinition::from(&local_policy("1.0.0", &source))
        .declaration()
        .expect("a local Artifact root validates")
        .artifacts
        .try_into()
        .expect("the root is its own publication");
    assert_eq!((local.source, local.extract), (source, true));
    assert!(matches!(
        wyrd_ai::cards::Declarable::from(&artifact),
        wyrd_ai::cards::Declarable::Card(_)
    ));
    assert!(matches!(
        wyrd_ai::cards::Declarable::from(artifact),
        wyrd_ai::cards::Declarable::Card(_)
    ));
    for (kind, spec) in [
        (
            CardKind::Policy,
            json!({"description": "Sharing", "enforcement": "block"}),
        ),
        (
            CardKind::Source,
            json!({"source": {"kind": "object_store", "uri": "gs://archive/runs", "format": "parquet"}}),
        ),
    ] {
        assert_authors(
            CardDefinition::new(kind.clone(), "authored", "1.0.0", spec.clone()),
            &kind,
            spec,
        );
    }

    let mut verifier = Verifier::new("inventory", "1.0.0")
        .inputs([("report", InputSlot::json(json!({"type": "object"})))]);
    for (name, schema, required) in [
        (
            "required_nullable",
            json!({"type": ["object", "null"]}),
            true,
        ),
        (
            "optional_nullable",
            json!({"type": ["object", "null"]}),
            false,
        ),
        ("optional_strict", json!({"type": "object"}), false),
    ] {
        let slot = InputSlot::json(schema);
        verifier.add_input(name, if required { slot } else { slot.required(false) });
    }
    verifier
        .add_input(
            "document",
            InputSlot::media(["pdf"]).max_bytes(1024).required(false),
        )
        .add_input("trace", InputSlot::trace().required(false));
    let mut call = Action::agent(CardRef::agent("judge", "1.0.0"));
    call.add_input("report", "inputs.report")
        .add_workspace("/inputs/report.json", "inputs.report")
        .add_workspace("/inputs/policy.json", registered_policy("1.0.0"));
    let mut check_call =
        Check::agent(CardRef::agent("judge", "1.0.0")).inputs([("report", "inputs.report")]);
    check_call
        .add_workspace("/inputs/report.json", "inputs.report")
        .add_workspace("/inputs/policy.json", registered_policy("1.0.0"));
    verifier = verifier.steps([
        Step::new("extract", Action::extract("inputs.document")),
        Step::new("judge", call).when(
            json!({"path": "steps.extract.status", "operator": "equals", "expected": "succeeded"}),
        ),
        Step::new(
            "summarize",
            Action::code(registered_policy("1.0.0"), "summarize").runtime("python"),
        ),
    ]);
    let mut requirements = vec![
        Requirement::new("judged", "Judged", check_call),
        Requirement::new(
            "coded",
            "Coded",
            Check::code(CodeSource::inline("def f():\n    return {}\n"), "f"),
        ),
        Requirement::new(
            "formatted",
            "Formatted",
            Check::code(CodeSource::inline("def f():\n    return {}\n"), "f")
                .response_format("verdict"),
        ),
    ];
    let mut canonical_requirements = vec![
        json!({"id": "judged", "statement": "Judged", "check": {"kind": "agent", "agent": {"kind": "Agent", "name": "judge", "version": "1.0.0"},
            "inputs": {"report": "inputs.report"},
            "workspace": {"/inputs/report.json": "inputs.report", "/inputs/policy.json": {"artifact": {"card": {"kind": "Artifact", "name": "company-sharing-policy", "version": "1.0.0"}, "path": POLICY_FILE}}}}}),
        json!({"id": "coded", "statement": "Coded", "check": {"kind": "code", "source": {"inline": "def f():\n    return {}\n"}, "entrypoint": "f"}}),
        json!({"id": "formatted", "statement": "Formatted", "check": {"kind": "code", "source": {"inline": "def f():\n    return {}\n"}, "entrypoint": "f", "response_format": "verdict"}}),
    ];
    let wire = operators();
    assert_eq!(wire.len(), 56, "the native operator catalog");
    for (index, operator) in wire.into_iter().enumerate() {
        let checks = [
            (
                "assert",
                Check::assert("inputs.report", operator.clone(), 1),
                json!({"kind": "assert", "path": "inputs.report", "operator": operator, "expected": 1}),
            ),
            (
                "each",
                Check::each("inputs.report.items[*]", operator.clone(), 1).into(),
                json!({"kind": "assert", "each": "inputs.report.items[*]", "operator": operator, "expected": 1}),
            ),
            (
                "trace",
                Check::trace("inputs.trace", "$.spans[0].name", operator.clone(), 1),
                json!({"kind": "trace", "on": "inputs.trace", "span_selector": "$.spans[0].name", "operator": operator, "expected": 1}),
            ),
        ];
        for (form, check, canonical_check) in checks {
            let id = format!("{form}-{index}");
            requirements.push(Requirement::new(&id, "Compared", check));
            canonical_requirements
                .push(json!({"id": id, "statement": "Compared", "check": canonical_check}));
        }
    }
    let verifier = verifier
        .requirements(requirements)
        .decide(json!({"pass_rate": {"threshold": 0.5}}))
        .context_capture("hash");
    let expected = canonical(
        &CardKind::Verifier,
        json!({
            "inputs": {
                "report": {"type": "json", "schema": {"type": "object"}},
                "required_nullable": {"type": "json", "schema": {"type": ["object", "null"]}},
                "optional_nullable": {"type": "json", "schema": {"type": ["object", "null"]}, "required": false},
                "optional_strict": {"type": "json", "schema": {"type": "object"}, "required": false},
                "document": {"type": "media", "formats": ["pdf"], "max_bytes": 1024, "required": false},
                "trace": {"type": "trace", "required": false},
            },
            "steps": [
                {"id": "extract", "action": {"kind": "extract", "from": "inputs.document"}},
                {"id": "judge", "when": {"path": "steps.extract.status", "operator": "equals", "expected": "succeeded"},
                 "action": {"kind": "agent", "agent": {"kind": "Agent", "name": "judge", "version": "1.0.0"},
                    "inputs": {"report": "inputs.report"},
                    "workspace": {"/inputs/report.json": "inputs.report", "/inputs/policy.json": {"artifact": {"card": {"kind": "Artifact", "name": "company-sharing-policy", "version": "1.0.0"}, "path": POLICY_FILE}}}}},
                {"id": "summarize", "action": {"kind": "code", "runtime": "python", "source": {"artifact": {"card": {"kind": "Artifact", "name": "company-sharing-policy", "version": "1.0.0"}, "path": POLICY_FILE}}, "entrypoint": "summarize"}},
            ],
            "requirements": canonical_requirements,
            "decide": {"pass_rate": {"threshold": 0.5}},
            "context_capture": "hash",
        }),
    );
    assert_eq!(canonical(&CardKind::Verifier, verifier.spec()), expected);
    verifier
        .validate()
        .expect("shared validation accepts every operator and family");
    let wyrd_ai::Spec::Verifier(spec) = &expected else {
        panic!("a Verifier spec decodes as one");
    };
    let required: Vec<(&str, bool)> = spec
        .inputs
        .iter()
        .map(|(name, slot)| (name.as_str(), slot.required()))
        .collect();
    assert_eq!(
        required,
        [
            ("document", false),
            ("optional_nullable", false),
            ("optional_strict", false),
            ("report", true),
            ("required_nullable", true),
            ("trace", false),
        ],
        "requiredness is independent of whether the schema admits null",
    );

    let group = Verifier::new("conversation", "1.0.0")
        .inputs([
            (
                "turns",
                InputSlot::source("verifiers.turn-check.inputs.turn"),
            ),
            (
                "answered",
                InputSlot::source("verifiers.turn-check.requirements.answered.outcome"),
            ),
        ])
        .order_by("inputs.turns.turn")
        .requirements([Requirement::new(
            "every-turn-answered",
            "Every turn was answered",
            Check::each("inputs.answered[*]", "equals", "passed"),
        )]);
    assert_eq!(
        canonical(&CardKind::Verifier, group.spec()),
        canonical(
            &CardKind::Verifier,
            json!({
                "inputs": {"turns": "verifiers.turn-check.inputs.turn", "answered": "verifiers.turn-check.requirements.answered.outcome"},
                "order_by": "inputs.turns.turn",
                "requirements": [{"id": "every-turn-answered", "statement": "Every turn was answered",
                    "check": {"kind": "assert", "each": "inputs.answered[*]", "operator": "equals", "expected": "passed"}}],
            })
        ),
    );
    group
        .validate()
        .expect("shared validation accepts the group Verifier");

    let drift_spec = json!({
        "method": "Spc",
        "signal": {"kind": "Distribution", "baseline_ref": {"kind": "Data", "name": "latency-baseline", "version": "1.0.0"}, "features": ["latency"]},
        "condition": {"kind": "Statistical"},
        "profile": {"kind": "Spc", "sample_size": 5},
    });
    let mut drift = Verifier::new("latency-drift", "1.0.0");
    drift
        .add_input("latency", InputSlot::samples(["latency"]))
        .add_requirement(Requirement::new(
            "stable",
            "Latency is in control",
            Check::drift("inputs.latency", drift_spec.clone()),
        ));
    let mut drift_check = drift_spec;
    drift_check["kind"] = json!("drift");
    drift_check["on"] = json!("inputs.latency");
    assert_eq!(
        canonical(&CardKind::Verifier, drift.spec()),
        canonical(
            &CardKind::Verifier,
            json!({
                "inputs": {"latency": {"type": "samples", "features": ["latency"]}},
                "requirements": [{"id": "stable", "statement": "Latency is in control", "check": drift_check}],
            })
        ),
    );
    drift
        .validate()
        .expect("shared validation accepts the drift Verifier");

    let mut binding = Binding::new(Verifier::new("checked", "1.0.0")).runs_on(
        Trigger::new("daily", "1.0.0")
            .kind("schedule")
            .cron("0 0 * * *"),
    );
    binding
        .add_on_failure(Operator::new("page", "1.0.0").action(pager.clone()))
        .add_on_failure(pager.clone())
        .add_on_verdict_change(CardRef::operator("notify", "1.0.0"));
    let mut agent = policy_judge("bound", "1.0.0");
    agent.add_verified_by(binding);
    let bound_binding = json!({
        "verifier": {"sibling": {"kind": "Verifier", "name": "checked", "version": "1.0.0"}},
        "runs_on": {"sibling": {"kind": "Trigger", "name": "daily", "version": "1.0.0"}},
        "on_failure": [
            {"sibling": {"kind": "Operator", "name": "page", "version": "1.0.0"}},
            pager,
        ],
        "on_verdict_change": [{"kind": "Operator", "name": "notify", "version": "1.0.0"}],
    });
    let mut expected_agent = policy_judge("bound", "1.0.0").spec();
    expected_agent["verified_by"] = json!([bound_binding]);
    assert_eq!(
        canonical(&CardKind::Agent, agent.spec()),
        canonical(&CardKind::Agent, expected_agent)
    );
    let mut declared: Vec<CardKind> = CardDefinition::from(agent)
        .declaration()
        .expect("valid graph")
        .input
        .as_ref()
        .expect("an ordinary graph")
        .submissions
        .iter()
        .map(|submission| submission.kind.clone())
        .collect();
    declared.sort_by_key(CardKind::wire_name);
    assert_eq!(
        declared,
        [
            CardKind::Agent,
            CardKind::Operator,
            CardKind::Trigger,
            CardKind::Verifier
        ]
    );

    let mut component = Component::new("judge", CardRef::agent("judge", "1.0.0"))
        .source(json!({"path": "services/judge", "kind": "python"}))
        .config([("region", json!({"type": "str", "value": "eu"}))]);
    component.add_verified_by(CardRef::verifier("checked", "1.0.0"));
    let service = Service::new("authored", "1.0.0")
        .description("Release service")
        .service_type("api")
        .entry_point("main:app")
        .components([component])
        .verified_by(["./not-a-file.yaml"]);
    let service = service.verified_by([CardRef::verifier("release", "1.0.0")]);
    assert_eq!(
        canonical(&CardKind::Service, service.spec()),
        canonical(
            &CardKind::Service,
            json!({
                "description": "Release service",
                "service_type": "api",
                "entry_point": "main:app",
                "components": [{
                    "alias": "judge",
                    "ref": {"kind": "Agent", "name": "judge", "version": "1.0.0"},
                    "source": {"path": "services/judge", "kind": "python"},
                    "config": {"region": {"type": "str", "value": "eu"}},
                    "verified_by": [{"verifier": {"kind": "Verifier", "name": "checked", "version": "1.0.0"}}],
                }],
                "verified_by": [{"verifier": {"kind": "Verifier", "name": "release", "version": "1.0.0"}}],
            })
        ),
        "a whole-field setter replaces an earlier binding"
    );

    let mounted = local_policy("1.0.0", &story("company-sharing-policy.pdf"));
    let agent_call = |judge: &str| {
        let mut call =
            Action::agent(policy_judge(judge, "1.0.0")).inputs([("report", "inputs.report")]);
        call.add_workspace("/inputs/policy.json", mounted.file(POLICY_FILE));
        call
    };
    let agent_check = |judge: &str| {
        let mut check =
            Check::agent(policy_judge(judge, "1.0.0")).inputs([("report", "inputs.report")]);
        check.add_workspace("/inputs/policy.json", mounted.file(POLICY_FILE));
        check
    };
    let edited_call = agent_call("old-judge").agent(policy_judge("new-judge", "1.0.0"));
    let edited_check = agent_check("old-judge").agent(policy_judge("new-judge", "1.0.0"));
    let edited_each = Check::each("inputs.report.old[*]", "equals", 1)
        .each("inputs.report.items[*]")
        .operator("greater_than")
        .expected(0);
    let edited_extract = Action::extract("inputs.report").from("inputs.document");
    assert_eq!(
        edited_call,
        agent_call("new-judge"),
        "Action.agent replaces"
    );
    assert_eq!(
        edited_check,
        agent_check("new-judge"),
        "Check.agent replaces"
    );
    assert_eq!(
        edited_each,
        Check::each("inputs.report.items[*]", "greater_than", 0),
        "Check.each replaces"
    );
    assert_eq!(
        edited_extract,
        Action::extract("inputs.document"),
        "Action.from replaces"
    );
    let edited = Verifier::new("edited", "1.0.0")
        .inputs([
            ("report", InputSlot::json(json!({"type": "object"}))),
            ("document", InputSlot::media(["pdf"])),
        ])
        .steps([
            Step::new("extract", edited_extract),
            Step::new("judge", edited_call),
        ])
        .requirements([
            Requirement::new("judged", "Judged", edited_check),
            Requirement::new("each", "Each", edited_each),
        ]);
    assert_eq!(
        edited.spec()["steps"][1]["action"]["kind"],
        "agent",
        "an edit keeps the discriminant"
    );
    assert_eq!(edited.spec()["requirements"][1]["check"]["kind"], "assert");
    let declaration = CardDefinition::from(&edited)
        .declaration()
        .expect("valid graph");
    let agents: Vec<String> = declaration
        .input
        .expect("an ordinary graph")
        .submissions
        .iter()
        .filter(|submission| submission.kind == CardKind::Agent)
        .map(|submission| submission.metadata.name.to_string())
        .collect();
    assert_eq!(agents, ["new-judge"], "the replaced Agent is not declared");
    assert_eq!(
        declaration.artifacts.len(),
        1,
        "the mounted policy survives the Agent replacement"
    );
}
