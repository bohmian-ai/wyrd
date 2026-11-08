//! The `card_authoring` story, authored in Rust: Service `finance-release`
//! composes Agents `report-reviewer` and `release-controls`, each verified by
//! its own two-step Verifier (`report-policy`, `release-access`), and the
//! report judge reads the extracted `company-sharing-policy` Artifact. The
//! graph is built in code through `wyrd_ai::authoring` and declared through
//! `Cards::declare_authored` or `WyrdState::declare_authored_with`, which
//! publish a local Artifact first and then declare the graph.
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
    Action, Agent, AgentCall, Artifact, ArtifactFile, BuiltinTool, Check, CodeCall, Prompt,
    Reference, Requirement, Service, Step, Verifier,
};
use wyrd_ai::cards::{CardKind, CardRef, CardSelector, Cards};
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
    deployment: Deployment,
    upstream: MockServer,
}

impl Story {
    /// Start the server and the scripted judge, and configure the gateway.
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
    async fn state(&self, service: &Service) -> WyrdState {
        WyrdState::declare_authored_with(
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
    async fn judge_calls(&self) -> usize {
        self.upstream
            .received_requests()
            .await
            .expect("the upstream records requests")
            .len()
    }
}

/// A policy-judge Agent `name` at `version`.
fn policy_judge(name: &str, version: &str) -> Agent {
    let mut prompt = Prompt::new(
        "openai",
        "gpt-4o",
        "POLICY JUDGE: read /inputs/policy.json with fs.read. Determine whether the proposed \
         release satisfies the company policy. Cite original request and policy evidence. \
         Report: {{report}} Recipient: {{recipient}} Purpose: {{purpose}} Destination: \
         {{destination}}",
    )
    .expect("prompt compiles");
    prompt
        .set_response_format(json!("verdict"))
        .expect("verdict is a named format");
    let mut agent = Agent::new(name, version, prompt).expect("valid identity");
    agent.set_gateway_route("openai").expect("valid deployment");
    agent.set_tools([BuiltinTool::FsRead.as_str()]);
    agent.set_max_iterations(3);
    agent
}

/// A code step running `entrypoint` of the story's checks over `inputs`,
/// judged by one `path == expected` requirement.
fn fact_step(
    id: &str,
    entrypoint: &str,
    inputs: &[(&str, &Reference)],
    requirement: (&str, &str, &str, Value),
) -> Step {
    let mut call =
        CodeCall::from_file(story("release_checks.py"), entrypoint).expect("source reads");
    for (name, value) in inputs {
        call.add_input(name, (*value).clone())
            .expect("valid parameter");
    }
    let mut step = Step::new(id, Action::code(call)).expect("valid id");
    let (requirement_id, statement, path, expected) = requirement;
    let check = Check::equals(step.output(path).expect("path"), expected);
    step.add_requirement(Requirement::new(requirement_id, statement, check).expect("valid id"))
        .expect("new id");
    step
}

/// Declare the recipient, purpose, and destination slots on `verifier`.
fn release_inputs(verifier: &mut Verifier) -> [Reference; 3] {
    let recipient = verifier
        .add_input(
            "recipient",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["email", "authorized", "permitted_purposes"],
                "properties": {
                    "email": {"type": "string"},
                    "authorized": {"type": "boolean"},
                    "permitted_purposes": {"type": "array", "items": {"type": "string"}},
                },
            }),
        )
        .expect("new slot");
    let purpose = verifier
        .add_input("purpose", json!({"type": "string"}))
        .expect("new slot");
    let destination = verifier
        .add_input(
            "destination",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["url", "encrypted", "restricted_access", "public_sharing"],
                "properties": {
                    "url": {"type": "string"},
                    "encrypted": {"type": "boolean"},
                    "restricted_access": {"type": "boolean"},
                    "public_sharing": {"type": "boolean"},
                },
            }),
        )
        .expect("new slot");
    [recipient, purpose, destination]
}

/// `report-policy` at `version`: the identifier scan with the policy judge
/// reading `policy`, then the report context.
fn report_policy(version: &str, judge: &Agent, policy: &ArtifactFile) -> Verifier {
    let mut verifier = Verifier::new("report-policy", version).expect("valid identity");
    verifier.set_description("The report is fit to share under the company sharing policy.");
    let report = verifier
        .add_input(
            "report",
            json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["text"],
                "properties": {"text": {"type": "string"}},
            }),
        )
        .expect("new slot");
    let [recipient, purpose, destination] = release_inputs(&mut verifier);
    let mut scan = fact_step(
        "policy",
        "report_facts",
        &[("report", &report)],
        (
            "no-bank-account-identifier",
            "The report contains no prohibited bank identifier.",
            "contains_bank_identifier",
            json!(false),
        ),
    );
    let mut call = AgentCall::new(judge.clone());
    for (name, value) in [
        ("report", &report),
        ("recipient", &recipient),
        ("purpose", &purpose),
        ("destination", &destination),
    ] {
        call.add_input(name, value.clone())
            .expect("valid parameter");
    }
    call.add_workspace_file("/inputs/policy.json", policy);
    scan.add_requirement(
        Requirement::new(
            "sharing-policy-satisfied",
            "The release satisfies the company sharing policy.",
            Check::agent(call),
        )
        .expect("valid id"),
    )
    .expect("new id");
    let context = fact_step(
        "policy-context",
        "policy_context",
        &[("report", &report), ("purpose", &purpose)],
        (
            "usable-report",
            "The proposed release contains report text.",
            "has_report",
            json!(true),
        ),
    );
    verifier.add_step(scan).expect("new step");
    verifier.add_step(context).expect("new step");
    verifier
}

/// `release-access` at `version`: independent recipient and destination
/// steps, and an optional `notes` slot.
fn release_access(version: &str) -> Verifier {
    let mut verifier = Verifier::new("release-access", version).expect("valid identity");
    verifier.set_description("The recipient and destination may receive the report.");
    let [recipient, purpose, destination] = release_inputs(&mut verifier);
    verifier
        .add_input_with_required("notes", json!({"type": "string"}), false)
        .expect("new slot");
    verifier
        .add_step(fact_step(
            "recipient",
            "recipient_facts",
            &[("recipient", &recipient), ("purpose", &purpose)],
            (
                "recipient-authorized",
                "The recipient is authorized for this purpose.",
                "authorized",
                json!(true),
            ),
        ))
        .expect("new step");
    verifier
        .add_step(fact_step(
            "destination",
            "destination_facts",
            &[("destination", &destination)],
            (
                "destination-controls-satisfied",
                "The destination has appropriate access controls.",
                "appropriate",
                json!(true),
            ),
        ))
        .expect("new step");
    verifier
}

/// `finance-release`, with every Card at `version`, verifying
/// `report-reviewer` by `report-policy` reading `policy` and
/// `release-controls` by `access`.
fn finance_release(version: &str, policy: &ArtifactFile, access: Verifier) -> Service {
    let reviewer = policy_judge("report-reviewer", version);
    let report = report_policy(version, &reviewer, policy);
    let mut service = Service::new("finance-release", version).expect("valid identity");
    service.set_service_type("api");
    service
        .add_component("report-reviewer", reviewer, [report])
        .expect("new alias");
    service
        .add_component(
            "release-controls",
            policy_judge("release-controls", version),
            [access],
        )
        .expect("new alias");
    service
}

/// The local policy Artifact `version`, published from `source`.
fn local_policy(version: &str, source: &Path) -> Artifact {
    Artifact::from_path("company-sharing-policy", version, source, true).expect("valid identity")
}

/// The registered policy Artifact `version`, used without uploading.
fn registered_policy(version: &str) -> ArtifactFile {
    ArtifactFile::new(
        card_ref(CardKind::Artifact, "company-sharing-policy", version),
        POLICY_FILE,
    )
    .expect("Artifact reference")
}

/// The exact reference of `kind` `name` at `version`.
fn card_ref(kind: CardKind, name: &str, version: &str) -> CardRef {
    serde_json::from_value(json!({ "kind": kind, "name": name, "version": version }))
        .expect("card reference parses")
}

/// The baseline story graph over `policy`.
fn baseline(policy: &ArtifactFile) -> Service {
    finance_release("1.0.0", policy, release_access("1.0.0"))
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
fn outcome(result: &VerificationResult, id: &str) -> RequirementOutcome {
    requirement(result, id).outcome.clone()
}

/// Assert `result` judged the component `alias` by `verifier` and executed
/// exactly the steps `steps`, in any stage order.
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
    let service = baseline(&policy.file(POLICY_FILE).expect("relative path"));
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
        .declare_authored(&service)
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
