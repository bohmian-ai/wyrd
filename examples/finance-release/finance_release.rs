//! The finance release, authored in Rust.
//!
//! A finance team's `report-writer` Agent prepares customer reports for
//! outside auditors. Before a report is sent, the `release-check` Verifier
//! judges three requirements, all at once:
//!
//! - `no-bank-account`: the report names no bank account (an assertion).
//! - `recipient-authorized`: the recipient is authorized (an assertion).
//! - `follows-policy`: `policy-judge` reads the company sharing policy and
//!   approves the release (an Agent check).
//!
//! The program builds every Card in code, declares them in one call, and
//! verifies a permitted release and one that names a bank account.

use wyrd_ai::authoring::{
    Agent, AgentCall, Artifact, BuiltinTool, Check, ComparisonOperator, Prompt, Requirement,
    Service, Verifier,
};
use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::Inputs;
use wyrd_ai::{RequirementOutcome, VerificationResult};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The company sharing policy, uploaded from this directory. `true`
    // extracts the PDF's text so the judge can read it.
    let policy = Artifact::from_path(
        "company-sharing-policy",
        "1.0.0",
        "company-sharing-policy.pdf",
        true,
    )?;

    // The Agent that writes the reports.
    let mut writer = Agent::new(
        "report-writer",
        "1.0.0",
        Prompt::new(
            "openai",
            "gpt-4o",
            "Write the customer report for {{customer}}.",
        )?,
    )?;
    writer.set_gateway_route("openai")?;

    // The Service: one Agent, verified by `release-check`.
    let mut service = Service::new("finance-release", "1.0.0")?;
    service.add_component("report-writer", writer, Vec::<Verifier>::new())?;
    service.add_verifier(release_check(&policy)?);

    // Upload the policy, declare every Card, and get a ready state.
    let state = WyrdState::declare_authored(&service).await?;

    for report in [
        "Quarterly total $120.",
        "Quarterly total $120. Paid from BANK-ACCOUNT-DEMO-001.",
    ] {
        let result = state
            .run()
            .observe()
            .verify(
                "release-check",
                Inputs::named([
                    ("report", json!({ "text": report })),
                    (
                        "recipient",
                        json!({ "email": "auditor@example.test", "authorized": true }),
                    ),
                ]),
            )
            .await?;
        print_result(report, &result);
    }
    Ok(())
}

/// `release-check`: two assertions and one Agent check, which reads `policy`.
fn release_check(policy: &Artifact) -> anyhow::Result<Verifier> {
    let mut verifier = Verifier::new("release-check", "1.0.0")?;

    // The evidence the app sends, and the JSON Schema each must match.
    let report = verifier.add_input(
        "report",
        json!({
            "type": "object",
            "required": ["text"],
            "properties": { "text": { "type": "string" } },
        }),
    )?;
    let recipient = verifier.add_input(
        "recipient",
        json!({
            "type": "object",
            "required": ["email", "authorized"],
            "properties": {
                "email": { "type": "string" },
                "authorized": { "type": "boolean" },
            },
        }),
    )?;

    // An assertion: compare a value, no model runs.
    verifier.add_requirement(Requirement::new(
        "no-bank-account",
        "The report names no bank account.",
        Check::assert(
            report.at("text")?,
            ComparisonOperator::NotContains,
            json!("BANK-ACCOUNT"),
        ),
    )?)?;

    // Another assertion.
    verifier.add_requirement(Requirement::new(
        "recipient-authorized",
        "The recipient is authorized.",
        Check::equals(recipient.at("authorized")?, json!(true)),
    )?)?;

    // An Agent check: `policy-judge` sees the report and the recipient, and
    // reads the policy from `/inputs/policy.json`.
    let mut judge = AgentCall::new(policy_judge()?);
    judge.add_input("report", report)?;
    judge.add_input("recipient", recipient)?;
    judge.add_workspace_file(
        "/inputs/policy.json",
        &policy.file("extracted/company-sharing-policy.pdf.json")?,
    );
    verifier.add_requirement(Requirement::new(
        "follows-policy",
        "The release follows the company sharing policy.",
        Check::agent(judge),
    )?)?;
    Ok(verifier)
}

/// `policy-judge`: a model that reads the policy and returns a verdict on
/// the release.
fn policy_judge() -> anyhow::Result<Agent> {
    let mut prompt = Prompt::new(
        "openai",
        "gpt-4o",
        "Read /inputs/policy.json with fs.read. Decide whether this release \
         follows the company sharing policy, citing the report and the policy. \
         Report: {{report}} Recipient: {{recipient}}",
    )?;
    prompt.set_response_format(json!("verdict"))?;

    let mut judge = Agent::new("policy-judge", "1.0.0", prompt)?;
    judge.set_gateway_route("openai")?;
    judge.set_tools([BuiltinTool::FsRead.as_str()]);
    judge.set_max_iterations(3);
    Ok(judge)
}

/// Print the verdict on `report`, then each requirement that did not pass.
fn print_result(report: &str, result: &VerificationResult) {
    println!("{report}");
    println!("  {}: {}", result.verifier.name, result.outcome.verdict);
    for requirement in &result.outcome.requirements {
        if requirement.outcome != RequirementOutcome::Passed {
            println!("    not passed: {}", requirement.statement);
        }
    }
}
