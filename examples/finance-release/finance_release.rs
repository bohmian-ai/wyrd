//! The finance release, authored in Rust.
//!
//! A finance team sends customer reports to outside auditors. Before a report
//! leaves, two things must hold:
//!
//! - `report-policy`: the report names no bank account, and a policy judge
//!   that reads the company sharing policy approves the release.
//! - `release-access`: the recipient is authorized for the purpose, and the
//!   destination is encrypted, restricted, and not public.
//!
//! The program builds every Card in code, declares them in one call, and
//! verifies a permitted release and one that leaks a bank account.

use wyrd_ai::authoring::{
    Agent, AgentCall, Artifact, BuiltinTool, Check, ComparisonOperator, Prompt, Requirement,
    Service, Verifier,
};
use wyrd_ai::serde_json::{Value, json};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::Inputs;
use wyrd_ai::{RequirementOutcome, VerificationResult};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The company sharing policy, uploaded from this directory.
    let policy = Artifact::from_path(
        "company-sharing-policy",
        "1.0.0",
        "company-sharing-policy.pdf",
        true, // extract the PDF's text so the judge can read it
    )?;

    // The Service and its two Agents.
    let reviewer = policy_judge("report-reviewer")?;
    let controls = policy_judge("release-controls")?;
    let mut service = Service::new("finance-release", "1.0.0")?;
    service.add_component(
        "report-reviewer",
        reviewer.clone(),
        [report_policy(&reviewer, &policy)?],
    )?;
    service.add_component("release-controls", controls, [release_access()?])?;

    // Upload the policy, declare every Card, and get a ready state.
    let state = WyrdState::declare_authored(&service).await?;

    for (label, report) in [
        ("permitted release", "Quarterly total $120."),
        (
            "release naming a bank account",
            "Quarterly total $120. Paid from BANK-ACCOUNT-DEMO-001.",
        ),
    ] {
        let recipient = json!({
            "email": "auditor@example.test",
            "authorized": true,
            "permitted_purposes": ["external-audit"],
        });
        let destination = json!({
            "url": "https://auditor.example.test/reports",
            "encrypted": true,
            "restricted_access": true,
            "public_sharing": false,
        });

        // Each Agent is verified in its own Run.
        let report_result = state
            .run_for_card("report-reviewer")?
            .observe()
            .verify(
                "report-policy",
                Inputs::named([
                    ("report", json!({ "text": report })),
                    ("recipient", recipient.clone()),
                    ("purpose", json!("external-audit")),
                    ("destination", destination.clone()),
                ]),
            )
            .await?;
        let access_result = state
            .run_for_card("release-controls")?
            .observe()
            .verify(
                "release-access",
                Inputs::named([("recipient", recipient), ("destination", destination)]),
            )
            .await?;

        println!("{label}");
        print_result(&report_result);
        print_result(&access_result);
    }
    Ok(())
}

/// A policy judge Agent: a model that reads `/inputs/policy.json` and returns
/// a verdict on the proposed release.
fn policy_judge(name: &str) -> anyhow::Result<Agent> {
    let mut prompt = Prompt::new(
        "openai",
        "gpt-4o",
        "Read /inputs/policy.json with fs.read. Decide whether this release \
         follows the company sharing policy, citing the report and the policy. \
         Report: {{report}} Recipient: {{recipient}} Purpose: {{purpose}} \
         Destination: {{destination}}",
    )?;
    prompt.set_response_format(json!("verdict"))?;

    let mut agent = Agent::new(name, "1.0.0", prompt)?;
    agent.set_gateway_route("openai")?;
    agent.set_tools([BuiltinTool::FsRead.as_str()]);
    agent.set_max_iterations(3);
    Ok(agent)
}

/// `report-policy`: an assertion on the report text and an Agent check by
/// `judge`, which reads `policy`.
fn report_policy(judge: &Agent, policy: &Artifact) -> anyhow::Result<Verifier> {
    let mut verifier = Verifier::new("report-policy", "1.0.0")?;
    let report = verifier.add_input("report", report_schema())?;
    let recipient = verifier.add_input("recipient", recipient_schema())?;
    let purpose = verifier.add_input("purpose", json!({ "type": "string" }))?;
    let destination = verifier.add_input("destination", destination_schema())?;

    // An assertion: no model runs.
    verifier.add_requirement(Requirement::new(
        "no-bank-account",
        "The report names no bank account.",
        Check::assert(
            report.at("text")?,
            ComparisonOperator::NotContains,
            json!("BANK-ACCOUNT"),
        ),
    )?)?;

    // An Agent check: the judge sees the release and reads the policy file.
    let mut judge_call = AgentCall::new(judge.clone());
    judge_call.add_input("report", report)?;
    judge_call.add_input("recipient", recipient)?;
    judge_call.add_input("purpose", purpose)?;
    judge_call.add_input("destination", destination)?;
    judge_call.add_workspace_file(
        "/inputs/policy.json",
        &policy.file("extracted/company-sharing-policy.pdf.json")?,
    );
    verifier.add_requirement(Requirement::new(
        "follows-sharing-policy",
        "The release follows the company sharing policy.",
        Check::agent(judge_call),
    )?)?;
    Ok(verifier)
}

/// `release-access`: assertions on the recipient and the destination.
fn release_access() -> anyhow::Result<Verifier> {
    let mut verifier = Verifier::new("release-access", "1.0.0")?;
    let recipient = verifier.add_input("recipient", recipient_schema())?;
    let destination = verifier.add_input("destination", destination_schema())?;

    verifier.add_requirement(Requirement::new(
        "recipient-authorized",
        "The recipient is authorized.",
        Check::equals(recipient.at("authorized")?, json!(true)),
    )?)?;
    verifier.add_requirement(Requirement::new(
        "purpose-permitted",
        "The recipient may receive reports for an external audit.",
        Check::assert(
            recipient.at("permitted_purposes")?,
            ComparisonOperator::IsSuperset, // the list includes every expected item
            json!(["external-audit"]),
        ),
    )?)?;
    verifier.add_requirement(Requirement::new(
        "destination-encrypted",
        "The destination is encrypted.",
        Check::equals(destination.at("encrypted")?, json!(true)),
    )?)?;
    verifier.add_requirement(Requirement::new(
        "destination-restricted",
        "The destination restricts access.",
        Check::equals(destination.at("restricted_access")?, json!(true)),
    )?)?;
    verifier.add_requirement(Requirement::new(
        "destination-private",
        "The destination is not shared publicly.",
        Check::equals(destination.at("public_sharing")?, json!(false)),
    )?)?;
    Ok(verifier)
}

/// The report: its text.
fn report_schema() -> Value {
    json!({
        "type": "object",
        "required": ["text"],
        "properties": { "text": { "type": "string" } },
    })
}

/// Who receives the report, and the purposes they may receive it for.
fn recipient_schema() -> Value {
    json!({
        "type": "object",
        "required": ["email", "authorized", "permitted_purposes"],
        "properties": {
            "email": { "type": "string" },
            "authorized": { "type": "boolean" },
            "permitted_purposes": { "type": "array", "items": { "type": "string" } },
        },
    })
}

/// Where the report is sent, and how that place is protected.
fn destination_schema() -> Value {
    json!({
        "type": "object",
        "required": ["url", "encrypted", "restricted_access", "public_sharing"],
        "properties": {
            "url": { "type": "string" },
            "encrypted": { "type": "boolean" },
            "restricted_access": { "type": "boolean" },
            "public_sharing": { "type": "boolean" },
        },
    })
}

/// Print a Verifier's verdict, then each requirement that did not pass.
fn print_result(result: &VerificationResult) {
    println!("  {}: {}", result.verifier.name, result.outcome.verdict);
    for requirement in &result.outcome.requirements {
        if requirement.outcome != RequirementOutcome::Passed {
            println!("    not passed: {}", requirement.statement);
        }
    }
}
