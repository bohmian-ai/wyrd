//! Verify each proposed report release before the application sends it.
//!
//! The `finance-release` Service shares synthetic customer reports with
//! outside auditors. Before it releases one, it calls the `release-check`
//! Verifier once, and the Verifier judges three requirements together:
//!
//! - `recipient-authorized`: the supplied recipient record says the
//!   recipient is authorized (an assertion).
//! - `follows-policy`: `policy-judge` reads the company sharing policy PDF
//!   and judges the report against it (an Agent check, through the Wyrd
//!   gateway).
//! - `destination-restricted`: the supplied destination record says access
//!   is restricted (an assertion).
//!
//! Wyrd returns the verdict; the application decides. Only `passed` releases
//! the report; anything else holds it for review. Nothing is sent anywhere.

use std::error::Error;

use wyrd_ai::serde_json::{self, Map, Value, json};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Agent, AgentRunConfigSpec, Artifact, Check, ComparisonOperator, Inputs, OpenAiChatOptions,
    Requirement, ResponseFormat, Service, Verifier, openai_chat,
};
use wyrd_ai::{ExecutionRoute, RequirementOutcome, VerificationVerdict};

/// Declare the finance release, then verify each proposed release in
/// `releases.json` and print the application's decision.
///
/// # Errors
/// Returns the ambient client's configuration or credential error, the
/// declaration's refusal (an invalid graph, an unreadable policy file, or
/// the registry's refusal), and an unreadable `releases.json`. A refused
/// verification holds its release instead.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // The company sharing policy, uploaded once from this directory with its
    // text extracted so the judge can read it.
    let policy = Artifact::from_path("company-sharing-policy", None, "company-sharing-policy.pdf")
        .extract(true);
    let service = Service::new("finance-release", None).verified_by([release_check(&policy)?]);

    // Upload the policy, declare every Card, and get a ready state.
    let state = WyrdState::declare(&service).await?;

    let releases: Vec<Map<String, Value>> =
        serde_json::from_str(&std::fs::read_to_string("releases.json")?)?;
    for mut release in releases {
        let name = release.remove("name").unwrap_or_default();
        let name = name.as_str().unwrap_or_default();
        // One synchronous call judges all three requirements.
        let result = match state
            .run()
            .observe()
            .verify("release-check", Inputs::named(release))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                println!("{name}: {} -> held for review", error.code());
                continue;
            }
        };
        let verdict = result.outcome.verdict;
        // Only a passed verdict releases the report.
        let decision = if verdict == VerificationVerdict::Passed {
            "released"
        } else {
            "held for review"
        };
        let verifier = &result.verifier;
        println!(
            "{name}: {verdict} ({}@{}) -> {decision}",
            verifier.name, verifier.version
        );
        for requirement in &result.outcome.requirements {
            if requirement.outcome != RequirementOutcome::Passed {
                let outcome = json!(requirement.outcome);
                let outcome = outcome.as_str().unwrap_or_default();
                println!(
                    "  {} {outcome}: {}",
                    requirement.id.as_str(),
                    requirement.reason
                );
            }
        }
    }
    Ok(())
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
        "/inputs/policy.json",
        policy.file("extracted/company-sharing-policy.pdf.json"),
    );

    Ok(Verifier::new("release-check", None)
        // The evidence the application sends, and the JSON Schema each must
        // match.
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
            (
                "destination",
                json!({
                    "type": "object",
                    "required": ["url", "restricted"],
                    "properties": {
                        "url": { "type": "string" },
                        "restricted": { "type": "boolean" },
                    },
                }),
            ),
        ])
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
                "Read /inputs/policy.json with fs.read. Decide whether the company sharing \
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
