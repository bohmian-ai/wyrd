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
    Agent, Artifact, Check, Component, InputSlot, Requirement, Service, Verifier,
};
use wyrd_ai::serde_json::{Value, json};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::Inputs;
use wyrd_ai::{RequirementOutcome, VerificationResult};

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
    let policy = Artifact::from_path(
        "company-sharing-policy",
        "1.0.0",
        "company-sharing-policy.pdf",
    )
    .extract(true);

    // The Service: the Agent that writes the reports, verified by
    // `release-check`.
    let writer = Agent::new("report-writer", "1.0.0")
        .prompt(prompt("Write the customer report for {{customer}}."))
        .execution_route(json!({ "kind": "wyrd_gateway", "deployment": "openai" }));
    let service = Service::new("finance-release", "1.0.0")
        .components([Component::new("report-writer", writer)])
        .verified_by([release_check(&policy)]);

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

/// `release-check`: two assertions and one Agent check, which reads `policy`.
fn release_check(policy: &Artifact) -> Verifier {
    // An Agent check: `policy-judge` sees the report and the recipient, and
    // reads the policy from `/inputs/policy.json`.
    let mut follows_policy = Check::agent(policy_judge()).inputs([
        ("report", "inputs.report"),
        ("recipient", "inputs.recipient"),
    ]);
    follows_policy.add_workspace(
        "/inputs/policy.json",
        policy.file("extracted/company-sharing-policy.pdf.json"),
    );

    Verifier::new("release-check", "1.0.0")
        // The evidence the app sends, and the JSON Schema each must match.
        .inputs([
            (
                "report",
                InputSlot::json(json!({
                    "type": "object",
                    "required": ["text"],
                    "properties": { "text": { "type": "string" } },
                })),
            ),
            (
                "recipient",
                InputSlot::json(json!({
                    "type": "object",
                    "required": ["email", "authorized"],
                    "properties": {
                        "email": { "type": "string" },
                        "authorized": { "type": "boolean" },
                    },
                })),
            ),
        ])
        .requirements([
            // Assertions: compare a value, no model runs.
            Requirement::new(
                "no-bank-account",
                "The report names no bank account.",
                Check::assert("inputs.report.text", "not_contains", "BANK-ACCOUNT"),
            ),
            Requirement::new(
                "recipient-authorized",
                "The recipient is authorized.",
                Check::assert("inputs.recipient.authorized", "equals", true),
            ),
            Requirement::new(
                "follows-policy",
                "The release follows the company sharing policy.",
                follows_policy,
            ),
        ])
}

/// `policy-judge`: a model that reads the policy with `fs.read` and returns
/// a verdict on the release.
fn policy_judge() -> Agent {
    let mut prompt = prompt(
        "Read /inputs/policy.json with fs.read. Decide whether this release \
         follows the company sharing policy, citing the report and the policy. \
         Report: {{report}} Recipient: {{recipient}}",
    );
    prompt["response_format"] = json!("verdict");
    Agent::new("policy-judge", "1.0.0")
        .prompt(prompt)
        .execution_route(json!({ "kind": "wyrd_gateway", "deployment": "openai" }))
        .tool_names(["fs.read"])
        .run_config(json!({ "max_iterations": 3 }))
}

/// An inline `gpt-4o` Prompt whose one user message is `text`.
fn prompt(text: &str) -> Value {
    json!({
        "provider": "openai",
        "model": "gpt-4o",
        "messages": [{ "role": "user", "content": text }],
    })
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
