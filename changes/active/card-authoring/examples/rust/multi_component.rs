//! Approved revision-5 interface sketch.
//! Named fields follow YAML; constructors and edits are infallible local operations.

use std::path::Path;

use wyrd_ai::authoring::{
    Action, Agent, Artifact, ArtifactFile, CardRef, Check, CodeSource, Component, InputSlot,
    Prompt, Requirement, Service, Step, Verifier,
};
use wyrd_ai::cards::Cards;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::Inputs;
use wyrd_ai::{WyrdError, serde_json::json};

/// Constructs a distinct component and its named Prompt without network IO.
fn agent(name: &str) -> Agent {
    let prompt = Prompt::new(&format!("{name}-prompt"), "1.0.0")
        .provider("openai")
        .model("gpt-4o")
        .messages([json!({
            "role": "user",
            "content": "Read /inputs/policy.json with fs.read. Determine whether the proposed release satisfies the company policy. Cite original request and policy evidence. Report: {{report}} Recipient: {{recipient}} Purpose: {{purpose}} Destination: {{destination}}"
        })])
        .response_format("verdict");
    Agent::new(name, "1.0.0")
        .prompt(prompt)
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "openai"}))
        .tool_names(["fs.read"])
        .run_config(json!({"max_iterations": 3}))
}

/// Constructs a data-producing step; references retain their YAML strings.
fn checked_step(
    checks: &Path,
    id: &str,
    entrypoint: &str,
    inputs: &[(&str, &str)],
    requirement: (&str, &str, &str, bool),
) -> Step {
    let (requirement_id, statement, field, expected) = requirement;
    Step::new(
        id,
        Action::code(CodeSource::file(checks), entrypoint).inputs(inputs.iter().copied()),
    )
    .requirements([Requirement::new(
        requirement_id,
        statement,
        Check::assert(
            &format!("steps.{id}.output.{field}"),
            "equals",
            json!(expected),
        ),
    )])
}

/// Authors both two-step component bindings using field setters and add methods.
pub fn make_service(policy_file: ArtifactFile, checks: &Path) -> Service {
    let reviewer = agent("report-reviewer");
    let controls = agent("release-controls");
    let mut scan = checked_step(
        checks,
        "policy",
        "report_facts",
        &[("report", "inputs.report")],
        (
            "no-bank-account-identifier",
            "The report contains no prohibited bank identifier.",
            "contains_bank_identifier",
            false,
        ),
    );
    scan.add_requirement(Requirement::new(
        "sharing-policy-satisfied",
        "The release satisfies the company sharing policy.",
        Check::agent(reviewer.clone())
            .inputs([
                ("report", "inputs.report"),
                ("recipient", "inputs.recipient"),
                ("purpose", "inputs.purpose"),
                ("destination", "inputs.destination"),
            ])
            .workspace([("/inputs/policy.json", policy_file)]),
    ));
    let context = checked_step(
        checks,
        "policy-context",
        "policy_context",
        &[("report", "inputs.report"), ("purpose", "inputs.purpose")],
        (
            "usable-report",
            "The proposed release contains report text.",
            "has_report",
            true,
        ),
    );
    let report_schema = json!({
        "type": "object", "properties": {"text": {"type": "string"}},
        "required": ["text"], "additionalProperties": false
    });
    let recipient_schema = json!({
        "type": "object", "properties": {
            "email": {"type": "string"}, "authorized": {"type": "boolean"},
            "permitted_purposes": {"type": "array", "items": {"type": "string"}}
        }, "required": ["email", "authorized", "permitted_purposes"], "additionalProperties": false
    });
    let destination_schema = json!({
        "type": "object", "properties": {
            "url": {"type": "string"}, "encrypted": {"type": "boolean"},
            "restricted_access": {"type": "boolean"}, "public_sharing": {"type": "boolean"}
        }, "required": ["url", "encrypted", "restricted_access", "public_sharing"],
        "additionalProperties": false
    });
    let report_verifier = Verifier::new("report-policy", "1.0.0")
        .inputs([
            ("report", InputSlot::json(report_schema)),
            ("recipient", InputSlot::json(recipient_schema.clone())),
            ("purpose", InputSlot::json(json!({"type": "string"}))),
            ("destination", InputSlot::json(destination_schema.clone())),
        ])
        .steps([scan, context]);
    let recipient = checked_step(
        checks,
        "recipient",
        "recipient_facts",
        &[
            ("recipient", "inputs.recipient"),
            ("purpose", "inputs.purpose"),
        ],
        (
            "recipient-authorized",
            "The recipient is authorized for this purpose.",
            "authorized",
            true,
        ),
    );
    let destination = checked_step(
        checks,
        "destination",
        "destination_facts",
        &[("destination", "inputs.destination")],
        (
            "destination-controls-satisfied",
            "The destination has appropriate access controls.",
            "appropriate",
            true,
        ),
    );
    let mut access = Verifier::new("release-access", "1.0.0").inputs([
        ("recipient", InputSlot::json(recipient_schema)),
        ("purpose", InputSlot::json(json!({"type": "string"}))),
        ("destination", InputSlot::json(destination_schema)),
    ]);
    access.add_input(
        "notes",
        InputSlot::json(json!({"type": "string"})).required(false),
    );
    access.add_step(recipient).add_step(destination);
    Service::new("finance-release", "1.0.0").components([
        Component::new("report-reviewer", reviewer).verified_by([report_verifier]),
        Component::new("release-controls", controls).verified_by([access]),
    ])
}

/// Declares through both existing owners, then verifies each component view.
///
/// # Errors
/// Returns native validation/preparation, registry, hydration or verification errors.
pub async fn declare_and_verify(policy_source: &Path, checks: &Path) -> Result<(), WyrdError> {
    let policy =
        Artifact::from_path("company-sharing-policy", "1.0.0", policy_source).extract(true);
    let service = make_service(
        policy.file("extracted/company-sharing-policy.pdf.json"),
        checks,
    );
    service.validate()?;
    let cards = Cards::connect().await?;
    println!("{:?}", cards.declare(&service).await?);
    let state = reuse_registered_policy(checks).await?;
    let request = json!({
        "report": {"text": "Synthetic customer report: quarterly total $120."},
        "recipient": {"email": "auditor@example.test", "authorized": true,
            "permitted_purposes": ["external-audit"]},
        "purpose": "external-audit",
        "destination": {"url": "https://auditor.example.test/reports", "encrypted": true,
            "restricted_access": true, "public_sharing": false}
    });
    let run = state.run();
    let report = run
        .for_card("report-reviewer")?
        .observe()
        .verify(
            "report-policy",
            Inputs::named([
                ("report", request["report"].clone()),
                ("recipient", request["recipient"].clone()),
                ("purpose", request["purpose"].clone()),
                ("destination", request["destination"].clone()),
            ]),
        )
        .await?;
    let access = run
        .for_card("release-controls")?
        .observe()
        .verify(
            "release-access",
            Inputs::named([
                ("recipient", request["recipient"].clone()),
                ("purpose", request["purpose"].clone()),
                ("destination", request["destination"].clone()),
            ]),
        )
        .await?;
    println!("{:?} {:?}", report.outcome.verdict, access.outcome.verdict);
    Ok(())
}

/// Uses an exact registered Artifact reference without preparation or upload.
///
/// # Errors
/// Returns native validation, registry or hydration errors.
pub async fn reuse_registered_policy(checks: &Path) -> Result<WyrdState, WyrdError> {
    let policy = CardRef::artifact("company-sharing-policy", "1.0.0");
    let service = make_service(
        policy.file("extracted/company-sharing-policy.pdf.json"),
        checks,
    );
    WyrdState::declare(&service).await
}
