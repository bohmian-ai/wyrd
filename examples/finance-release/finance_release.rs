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
//! verifies a permitted release and one that names a bank account. No Card
//! names a version: the server selects each one at declaration.

use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Agent, AgentRunConfigSpec, Artifact, Check, ComparisonOperator, Component, Inputs,
    OpenAiChatOptions, Requirement, ResponseFormat, Service, Verifier, openai_chat,
};
use wyrd_ai::{ExecutionRoute, RequirementOutcome, VerificationResult};

/// Build the finance release in code, declare it with
/// [`WyrdState::declare`], which uploads the policy first, and print the
/// verdict of a permitted release and of one naming a bank account.
///
/// # Errors
/// Returns the ambient client's configuration or credential error, the
/// declaration's refusal (an invalid graph, an unreadable policy file, or
/// the registry's refusal), and a verification run's error.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The company sharing policy, uploaded from this directory, with the
    // PDF's text extracted so the judge can read it.
    let policy = Artifact::from_path("company-sharing-policy", None, "company-sharing-policy.pdf")
        .extract(true);

    // The Service: the Agent that writes the reports, verified by
    // `release-check`.
    let writer = Agent::new("report-writer", None)
        .prompt(openai_chat(
            "gpt-4o",
            OpenAiChatOptions {
                messages: vec!["Write the customer report for {{customer}}.".into()],
                ..Default::default()
            },
        )?)
        .execution_route(gateway()?);
    let service = Service::new("finance-release", None)
        .components([Component::new("report-writer", writer)])
        .verified_by([release_check(&policy)?]);

    // Upload the policy, declare every Card, and get a ready state.
    let state = WyrdState::declare(&service).await?;

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

/// The `openai` gateway deployment every Agent's model calls go through.
///
/// # Errors
/// Returns the refusal of an invalid deployment name.
fn gateway() -> anyhow::Result<ExecutionRoute> {
    Ok(ExecutionRoute::WyrdGateway {
        deployment: "openai".parse()?,
    })
}

/// `release-check`: two assertions and one Agent check, which reads `policy`.
///
/// # Errors
/// Returns the policy judge's Prompt or route refusal.
fn release_check(policy: &Artifact) -> anyhow::Result<Verifier> {
    // An Agent check: `policy-judge` sees the report and the recipient, and
    // reads the policy from `/inputs/policy.json`.
    let mut follows_policy = Check::agent(policy_judge()?).inputs([
        ("report", "inputs.report"),
        ("recipient", "inputs.recipient"),
    ]);
    follows_policy.add_workspace(
        "/inputs/policy.json",
        policy.file("extracted/company-sharing-policy.pdf.json"),
    );

    Ok(Verifier::new("release-check", None)
        // The evidence the app sends, and the JSON Schema each must match.
        .inputs([
            (
                "report",
                json!({
                    "type": "object",
                    "required": ["text"],
                    "properties": { "text": { "type": "string" } },
                }),
            ),
            (
                "recipient",
                json!({
                    "type": "object",
                    "required": ["email", "authorized"],
                    "properties": {
                        "email": { "type": "string" },
                        "authorized": { "type": "boolean" },
                    },
                }),
            ),
        ])
        .requirements([
            // Assertions: compare a value, no model runs.
            Requirement::new(
                "no-bank-account",
                "The report names no bank account.",
                Check::assert(
                    "inputs.report.text",
                    ComparisonOperator::NotContains,
                    "BANK-ACCOUNT",
                ),
            ),
            Requirement::new(
                "recipient-authorized",
                "The recipient is authorized.",
                Check::assert(
                    "inputs.recipient.authorized",
                    ComparisonOperator::Equals,
                    true,
                ),
            ),
            Requirement::new(
                "follows-policy",
                "The release follows the company sharing policy.",
                follows_policy,
            ),
        ]))
}

/// `policy-judge`: a model that reads the policy with `fs.read` and returns
/// a verdict on the release.
///
/// # Errors
/// Returns the Prompt builder's or the route's refusal.
fn policy_judge() -> anyhow::Result<Agent> {
    let prompt = openai_chat(
        "gpt-4o",
        OpenAiChatOptions {
            messages: vec![
                "Read /inputs/policy.json with fs.read. Decide whether this release follows the \
                 company sharing policy, citing the report and the policy. \
                 Report: {{report}} Recipient: {{recipient}}"
                    .into(),
            ],
            response_format: Some(ResponseFormat::verdict(None)),
            ..Default::default()
        },
    )?;
    Ok(Agent::new("policy-judge", None)
        .prompt(prompt)
        .execution_route(gateway()?)
        .tool_names(["fs.read"])
        .run_config(AgentRunConfigSpec {
            max_iterations: Some(3),
            ..Default::default()
        }))
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
