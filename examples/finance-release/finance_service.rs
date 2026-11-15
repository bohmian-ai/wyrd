use std::error::Error;
use std::path::Path;

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
    pub text: String,
}

/// Who receives the report.
#[derive(Serialize, JsonSchema)]
pub struct Recipient {
    pub email: String,
    pub authorized: bool,
}

/// Where the report goes.
#[derive(Serialize, JsonSchema)]
pub struct Destination {
    pub url: String,
    pub restricted: bool,
}

/// One proposed report release: the `release-check` input.
#[derive(Serialize, JsonSchema)]
pub struct Release {
    pub report: Report,
    pub recipient: Recipient,
    pub destination: Destination,
}

/// The company sharing policy the judge applies.
const POLICY: &str = "# Company sharing policy

Reports may be shared outside the company only for external-audit purposes.
A shared report must never contain a bank account identifier such as BANK-ACCOUNT-DEMO-001.
";

/// The `finance-release` Service, verified by `release-check`.
///
/// # Errors
/// Returns a refused policy write, or the policy judge's Prompt or route
/// refusal.
pub fn service() -> Result<Service, Box<dyn Error>> {
    // The company sharing policy. The program saves it beside itself the
    // first time; edit the file and run again to publish a new policy
    // version.
    let policy_file = Path::new("company-sharing-policy.md");
    if !policy_file.exists() {
        std::fs::write(policy_file, POLICY)?;
    }
    let policy = Artifact::from_path("company-sharing-policy", None, policy_file);
    Ok(Service::new("finance-release", None).verified_by([release_check(&policy)?]))
}

/// `release-check`: the recipient and destination assertions, and the
/// policy judgment, which reads `policy`.
///
/// # Errors
/// Returns the policy judge's Prompt or route refusal.
fn release_check(policy: &Artifact) -> Result<Verifier, Box<dyn Error>> {
    let mut follows_policy = Check::agent(policy_judge()?).inputs([
        ("report", "inputs.report"),
        ("recipient", "inputs.recipient"),
    ]);
    follows_policy.add_workspace(
        "/inputs/policy.md",
        policy.file("company-sharing-policy.md"),
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
                "Read /inputs/policy.md with fs.read. Decide whether the company sharing \
                 policy permits sharing this report with this recipient, citing the report \
                 and the policy line you apply. Report: {{report}} Recipient: {{recipient}}"
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
