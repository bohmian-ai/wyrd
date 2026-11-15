use std::error::Error;

use schemars::JsonSchema;
use serde::Serialize;
use wyrd_ai::ExecutionRoute;
use wyrd_ai::verify::{
    Agent, AgentRunConfigSpec, Artifact, Check, ComparisonOperator, InputSlot, OpenAiChatOptions,
    Requirement, ResponseFormat, Service, Verifier, openai_chat,
};

/// The report to send.
#[derive(Serialize, JsonSchema)]
pub struct Report {
    /// The synthetic report text the policy judge reads.
    pub text: String,
}

/// Who receives the report.
#[derive(Serialize, JsonSchema)]
pub struct Recipient {
    /// The recipient's address, shown to the policy judge.
    pub email: String,
    /// Whether the application's records authorize this recipient;
    /// `recipient-authorized` requires `true`.
    pub authorized: bool,
}

/// Where the report goes.
#[derive(Serialize, JsonSchema)]
pub struct Destination {
    /// The destination's address.
    pub url: String,
    /// Whether the destination restricts access; `destination-restricted`
    /// requires `true`.
    pub restricted: bool,
}

/// One proposed report release: the `release-check` input. Each field is
/// one input slot.
#[derive(Serialize, JsonSchema)]
pub struct Release {
    /// Why the report is shared; the policy judge checks it against the
    /// policy's permitted purposes.
    pub purpose: String,
    /// The report to send.
    pub report: Report,
    /// Who receives it.
    pub recipient: Recipient,
    /// Where it goes.
    pub destination: Destination,
}

/// The `finance-release` Service, verified by `release-check`.
///
/// # Errors
/// Returns the policy judge's Prompt or route refusal.
pub fn service() -> Result<Service, Box<dyn Error>> {
    // The company sharing policy, uploaded from this directory with its
    // text extracted so the judge can read it.
    let policy = Artifact::from_path("company-sharing-policy", None, "company-sharing-policy.pdf")
        .extract(true);
    Ok(Service::new("finance-release", None).verified_by([release_check(&policy)?]))
}

/// `release-check`: the recipient and destination assertions, and the
/// policy judgment, which reads `policy`.
///
/// # Errors
/// Returns the policy judge's Prompt or route refusal.
fn release_check(policy: &Artifact) -> Result<Verifier, Box<dyn Error>> {
    let mut follows_policy = Check::agent(policy_judge()?).inputs([
        ("purpose", "inputs.purpose"),
        ("report", "inputs.report"),
        ("recipient", "inputs.recipient"),
    ]);
    follows_policy.add_workspace(
        "/inputs/company-sharing-policy.pdf.json",
        policy.file("extracted/company-sharing-policy.pdf.json"),
    );

    Ok(Verifier::new("release-check", None)
        .inputs(InputSlot::fields_of::<Release>())
        .requirements([
            Requirement::new(
                "recipient-authorized",
                "The recipient is authorized to receive the report.",
                Check::assert(
                    "inputs.recipient.authorized",
                    ComparisonOperator::Equals,
                    true,
                ),
            ),
            Requirement::new(
                "follows-policy",
                "The company sharing policy permits this release.",
                follows_policy,
            ),
            Requirement::new(
                "destination-restricted",
                "The destination restricts access to the report.",
                Check::assert(
                    "inputs.destination.restricted",
                    ComparisonOperator::Equals,
                    true,
                ),
            ),
        ]))
}

/// `policy-judge`: a model that reads the policy with `fs.read` and returns
/// a verdict on the report. Its model calls go through the `openai` gateway
/// deployment.
///
/// # Errors
/// Returns the Prompt builder's or the route's refusal.
fn policy_judge() -> Result<Agent, Box<dyn Error>> {
    let prompt = openai_chat(
        "gpt-4o",
        OpenAiChatOptions {
            messages: vec![
                "Read /inputs/company-sharing-policy.pdf.json with fs.read. Decide whether the company sharing \
                 policy permits sharing this report with this recipient for this purpose, \
                 citing the report and the policy line you apply. Purpose: {{purpose}} \
                 Report: {{report}} Recipient: {{recipient}}"
                    .into(),
            ],
            response_format: Some(ResponseFormat::verdict(None)),
            ..Default::default()
        },
    )?;
    Ok(Agent::new("policy-judge", None)
        .prompt(prompt)
        .execution_route(ExecutionRoute::WyrdGateway {
            deployment: "openai".parse()?,
        })
        .tool_names(["fs.read"])
        .run_config(AgentRunConfigSpec {
            max_iterations: Some(3),
            ..Default::default()
        }))
}
