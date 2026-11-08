use serde_json::{Value, json};
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::service::ServiceSpec;
use wyrd_spec::card::verifier::VerifierSpec;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

use super::*;

fn rule(error: &WyrdError) -> Option<&str> {
    match error {
        WyrdError::Validation { details, .. } => details["rule"].as_str(),
        _ => None,
    }
}

fn reviewer() -> Agent {
    let mut prompt = Prompt::new(
        "openai",
        "gpt-4o",
        "Review {{report}} against /inputs/policy.json",
    )
    .expect("prompt compiles");
    prompt
        .set_response_format(json!("verdict"))
        .expect("verdict is a named format");
    let mut agent = Agent::new("report-reviewer", "1.0.0", prompt).expect("valid identity");
    agent.set_gateway_route("openai").expect("valid deployment");
    agent.set_tools([BuiltinTool::FsRead.as_str()]);
    agent.set_max_iterations(3);
    agent
}

fn policy() -> Artifact {
    Artifact::from_path("company-sharing-policy", "1.0.0", "policy.pdf", true)
        .expect("valid identity")
}

/// A Verifier exercising every Action and Check variant, both input
/// requirednesses, and an explicit dependency.
fn every_primitive() -> Verifier {
    let mut verifier = Verifier::new("release-access", "1.0.0").expect("valid identity");
    verifier.set_description("Every authoring primitive");
    let report = verifier
        .add_input("report", json!({"type": "object"}))
        .expect("new slot");
    let purpose = verifier
        .add_input_with_required("purpose", Value::Null, false)
        .expect("new slot");
    let document = verifier
        .add_slot(
            "document",
            InputSlot::Media {
                formats: vec![MediaFormat::Pdf],
                max_bytes: None,
                required: true,
            },
        )
        .expect("new slot");
    let trace = verifier
        .add_slot("trace", InputSlot::Trace { required: true })
        .expect("new slot");
    let extract = Step::new("extract", Action::extract(document)).expect("valid id");

    let mut review = AgentCall::new(reviewer());
    review
        .add_input("report", report.clone())
        .expect("valid parameter");
    review.add_workspace_input("/inputs/report.json", report.clone());
    review.add_workspace_file(
        "/inputs/policy.json",
        &policy()
            .file("extracted/company-sharing-policy.pdf.json")
            .expect("relative path"),
    );
    let mut policy_step = Step::new("policy", Action::agent(review)).expect("valid id");

    let mut context = CodeCall::inline(
        "def policy_context(report, purpose):\n    return {}\n",
        "policy_context",
    );
    context
        .add_input("report", report)
        .expect("valid parameter");
    context
        .add_input("purpose", purpose)
        .expect("valid parameter");
    let mut context_step = Step::new("policy-context", Action::code(context)).expect("valid id");
    context_step.add_depends_on(&policy_step);
    context_step.set_when(
        policy_step.output("verdict").expect("path"),
        ComparisonOperator::Equals,
        json!("pass"),
    );
    context_step.set_timeout_seconds(30);
    context_step.set_retries(2).expect("within limit");
    context_step
        .add_requirement(
            Requirement::new(
                "has-report",
                "The context has a report",
                Check::equals(
                    context_step.output("has_report").expect("path"),
                    json!(true),
                ),
            )
            .expect("valid id"),
        )
        .expect("new id");
    policy_step
        .add_requirement(
            Requirement::new(
                "findings-cited",
                "Every finding cites the policy",
                Check::each(
                    policy_step.output("findings[*].cited").expect("path"),
                    ComparisonOperator::Equals,
                    json!(true),
                ),
            )
            .expect("valid id"),
        )
        .expect("new id");

    for step in [extract, policy_step, context_step] {
        verifier.add_step(step).expect("new step");
    }
    let checks = [
        (
            "trace-order",
            Check::trace(
                trace,
                "$.spans[0].name",
                ComparisonOperator::Equals,
                json!("lookup"),
            )
            .expect("valid selector"),
        ),
        (
            "judged",
            Check::agent(AgentCall::new(Linked::registered(card("Agent", "judge")))),
        ),
        (
            "code-judged",
            Check::code(CodeCall::artifact(
                &Artifact::registered(card("Artifact", "release-checks"))
                    .expect("Artifact")
                    .file("release_checks.py")
                    .expect("relative path"),
                "summarize_findings",
            )),
        ),
        (
            "code-formatted",
            Check::code_with_format(
                CodeCall::inline("def judge():\n    return {}\n", "judge"),
                skald_spec::NamedResponseFormat::Name("verdict".to_owned()),
            ),
        ),
    ];
    for (id, check) in checks {
        verifier
            .add_requirement(Requirement::new(id, "Judged", check).expect("valid id"))
            .expect("new id");
    }
    verifier.set_decide(Decide::PassRate { threshold: 0.5 });
    verifier
}

fn yaml<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    serde_json::from_value(serde_yaml::from_str::<Value>(text).expect("YAML parses"))
}

fn card(kind: &str, name: &str) -> CardRef {
    serde_json::from_value(json!({"kind": kind, "name": name, "version": "1.0.0"}))
        .expect("card ref")
}

#[test]
fn core_primitives_match_canonical_contracts() {
    let verifier = every_primitive();
    let canonical: VerifierSpec = yaml(
        r#"
description: Every authoring primitive
inputs:
  report: {type: json, schema: {type: object}}
  purpose: {type: json, required: false}
  document: {type: media, formats: [pdf]}
  trace: {type: trace}
steps:
  - id: extract
    action: {kind: extract, from: inputs.document}
  - id: policy
    action:
      kind: agent
      agent: {sibling: {kind: Agent, name: report-reviewer, version: 1.0.0}}
      inputs: {report: inputs.report}
      workspace:
        /inputs/report.json: inputs.report
        /inputs/policy.json:
          artifact:
            card: {kind: Artifact, name: company-sharing-policy, version: 1.0.0}
            path: extracted/company-sharing-policy.pdf.json
    requirements:
      - id: findings-cited
        statement: Every finding cites the policy
        check: {kind: assert, each: "steps.policy.output.findings[*].cited", operator: equals, expected: true}
  - id: policy-context
    depends_on: [policy]
    when: {path: steps.policy.output.verdict, operator: equals, expected: pass}
    timeout_seconds: 30
    retries: 2
    action:
      kind: code
      source: {inline: "def policy_context(report, purpose):\n    return {}\n"}
      entrypoint: policy_context
      inputs: {report: inputs.report, purpose: inputs.purpose}
    requirements:
      - id: has-report
        statement: The context has a report
        check: {kind: assert, path: steps.policy-context.output.has_report, operator: equals, expected: true}
requirements:
  - id: trace-order
    statement: Judged
    check: {kind: trace, on: inputs.trace, span_selector: "$.spans[0].name", operator: equals, expected: lookup}
  - id: judged
    statement: Judged
    check: {kind: agent, agent: {kind: Agent, name: judge, version: 1.0.0}}
  - id: code-judged
    statement: Judged
    check:
      kind: code
      source: {artifact: {card: {kind: Artifact, name: release-checks, version: 1.0.0}, path: release_checks.py}}
      entrypoint: summarize_findings
  - id: code-formatted
    statement: Judged
    check: {kind: code, source: {inline: "def judge():\n    return {}\n"}, entrypoint: judge, response_format: verdict}
decide: {pass_rate: {threshold: 0.5}}
"#,
    )
    .expect("canonical Verifier decodes");
    assert_eq!(verifier.spec(), &canonical);
    verifier
        .validate()
        .expect("shared validation accepts the authored Verifier");

    let mut drift = Verifier::new("latency-drift", "1.0.0").expect("valid identity");
    let samples = drift
        .add_slot(
            "latency",
            InputSlot::Samples {
                features: vec!["latency".parse().expect("feature")],
            },
        )
        .expect("new slot");
    let spec = r#"
method: Spc
signal: {kind: Distribution, baseline_ref: {kind: Data, name: latency-baseline, version: 1.0.0}, features: [latency]}
condition: {kind: Statistical}
profile: {kind: Spc, sample_size: 5}
"#;
    drift
        .add_requirement(
            Requirement::new(
                "latency-stable",
                "Latency is in control",
                Check::drift(samples, yaml(spec).expect("drift spec")),
            )
            .expect("valid id"),
        )
        .expect("new id");
    let canonical_drift: VerifierSpec = yaml(&format!(
        "inputs: {{latency: {{type: samples, features: [latency]}}}}\nrequirements:\n  - id: latency-stable\n    statement: Latency is in control\n    check:\n      kind: drift\n      on: inputs.latency\n{}",
        spec.lines().filter(|line| !line.is_empty()).map(|line| format!("      {line}\n")).collect::<String>(),
    ))
    .expect("canonical drift Verifier decodes");
    assert_eq!(drift.spec(), &canonical_drift);
    drift
        .validate()
        .expect("shared validation accepts the drift Verifier");

    let plan = verifier.plan().expect("plans");
    let stages: Vec<Vec<&str>> = plan
        .stages
        .iter()
        .map(|stage| stage.tasks.iter().map(|task| task.as_str()).collect())
        .collect();
    assert_eq!(
        stages,
        [vec!["extract", "policy"], vec!["policy-context"]],
        "the policy-context dependency stages it after policy",
    );

    let stages = |verifier: &Verifier| -> Result<Vec<Vec<String>>, WyrdError> {
        Ok(verifier
            .plan()?
            .stages
            .iter()
            .map(|stage| {
                stage
                    .tasks
                    .iter()
                    .map(|task| task.as_str().to_owned())
                    .collect()
            })
            .collect())
    };
    let mut ordering = Verifier::new("ordering", "1.0.0").expect("valid identity");
    let input = ordering.add_input("value", Value::Null).expect("new slot");
    let first = Step::new("first", Action::extract(input.clone())).expect("valid id");
    let mut explicit = Step::new("explicit", Action::extract(input.clone())).expect("valid id");
    explicit.add_depends_on(&first);
    let mut inferred_call = CodeCall::inline("def f(x):\n    return x\n", "f");
    inferred_call
        .add_input("x", first.output("").expect("whole output"))
        .expect("valid parameter");
    let inferred = Step::new("inferred", Action::code(inferred_call)).expect("valid id");
    let mut cyclic = Step::new("cyclic", Action::extract(input.clone())).expect("valid id");
    cyclic.add_depends_on(&cyclic.clone());
    let unknown_target = Step::new("unknown", Action::extract(input)).expect("valid id");
    let mut dangling = Step::new(
        "dangling",
        Action::extract(unknown_target.output("").expect("path")),
    )
    .expect("valid id");
    dangling.add_depends_on(&unknown_target);
    for step in [first, explicit, inferred] {
        ordering.add_step(step).expect("new step");
    }
    assert_eq!(
        stages(&ordering).expect("plans"),
        [vec!["first"], vec!["explicit", "inferred"]],
        "an explicit-only depends_on edge stages like an inferred output reference",
    );
    let mut refused = ordering.clone();
    refused.add_step(cyclic).expect("new step");
    assert_eq!(
        rule(&stages(&refused).expect_err("self dependency")),
        Some("graph")
    );
    let mut refused = ordering;
    refused.add_step(dangling).expect("new step");
    assert_eq!(
        rule(&stages(&refused).expect_err("unknown dependency")),
        Some("graph")
    );

    let canonical_agent: AgentSpec = yaml(
        r#"
prompt:
  provider: openai
  model: gpt-4o
  messages: [{role: user, content: "Review {{report}} against /inputs/policy.json"}]
  response_format: verdict
execution_route: {kind: wyrd_gateway, deployment: openai}
tool_names: [fs.read]
run_config: {max_iterations: 3}
"#,
    )
    .expect("canonical Agent decodes");
    assert_eq!(reviewer().spec(), &canonical_agent);

    let mut native = reviewer();
    native.set_native_route();
    assert_eq!(native.spec().execution_route, ExecutionRoute::Native {});

    let mut service = Service::new("finance-release", "1.0.0").expect("valid identity");
    service.set_service_type("api");
    service
        .add_component("reviewer", reviewer(), [verifier.clone()])
        .expect("new alias");
    service
        .add_registered_component(
            "controls",
            card("Agent", "release-controls"),
            [Binding::registered(card("Verifier", "report-policy"))],
        )
        .expect("new alias");
    let canonical_service: ServiceSpec = yaml(
        r#"
service_type: api
components:
  - alias: reviewer
    ref: {sibling: {kind: Agent, name: report-reviewer, version: 1.0.0}}
    verified_by: [{verifier: {sibling: {kind: Verifier, name: release-access, version: 1.0.0}}}]
  - alias: controls
    ref: {kind: Agent, name: release-controls, version: 1.0.0}
    verified_by: [{verifier: {kind: Verifier, name: report-policy, version: 1.0.0}}]
"#,
    )
    .expect("canonical Service decodes");
    assert_eq!(service.spec(), &canonical_service);

    let declaration = CardDefinition::from(service.clone())
        .declaration()
        .expect("valid graph");
    assert_eq!(
        CardDefinition::from(service)
            .declaration()
            .expect("valid graph"),
        declaration,
        "repeated declaration is stable",
    );
    assert_eq!(
        declaration.artifacts,
        [policy()],
        "the local Artifact publishes first, once"
    );
    let kinds: Vec<CardKind> = declaration
        .input
        .submissions
        .iter()
        .map(|submission| submission.kind.clone())
        .collect();
    assert_eq!(
        kinds.last(),
        Some(&CardKind::Service),
        "the root declares last"
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == CardKind::Agent)
            .count(),
        1
    );
    assert!(declaration.input.artifact_sources.is_empty());
    let wire = serde_json::to_string(&declaration.input.submissions).expect("serializes");
    assert!(
        !wire.contains("policy.pdf\""),
        "no local path reaches a submission: {wire}"
    );
    assert!(
        !wire.contains("\"status\""),
        "no derived status reaches a submission: {wire}"
    );
}

#[test]
fn offline_edits_preserve_snapshots() {
    let mut verifier = Verifier::new("report-policy", "1.0.0").expect("valid identity");
    let report = verifier.add_input("report", Value::Null).expect("new slot");
    let mut step =
        Step::new("policy", Action::agent(AgentCall::new(reviewer()))).expect("valid id");
    verifier.add_step(step.clone()).expect("new step");
    step.set_timeout_seconds(5);
    assert_eq!(
        verifier.spec().steps[0].timeout_seconds,
        None,
        "a step is captured when added"
    );

    let mut agent = reviewer();
    let mut service = Service::new("finance-release", "1.0.0").expect("valid identity");
    service
        .add_component("reviewer", agent.clone(), [verifier.clone()])
        .expect("new alias");
    agent.set_max_iterations(9);
    verifier
        .add_requirement(
            Requirement::new(
                "has-report",
                "Has a report",
                Check::equals(report.clone(), json!(true)),
            )
            .expect("valid id"),
        )
        .expect("new id");
    let declaration = CardDefinition::from(service.clone())
        .declaration()
        .expect("valid graph");
    for submission in &declaration.input.submissions {
        match submission.kind {
            CardKind::Agent => assert_eq!(submission.spec["run_config"]["max_iterations"], 3),
            CardKind::Verifier => assert!(submission.spec.get("requirements").is_none()),
            _ => {}
        }
    }

    let duplicate = verifier
        .add_step(Step::new("has-report", Action::extract(report.clone())).expect("valid id"))
        .expect_err("a step reusing a requirement ID");
    assert_eq!(rule(&duplicate), Some("duplicate_id"));
    let duplicate = verifier
        .add_requirement(
            Requirement::new("policy", "Reused", Check::equals(report, json!(true)))
                .expect("valid id"),
        )
        .expect_err("a requirement reusing a step ID");
    assert_eq!(rule(&duplicate), Some("duplicate_id"));
    let duplicate = verifier
        .add_input("report", Value::Null)
        .expect_err("a reused slot");
    assert_eq!(rule(&duplicate), Some("duplicate_input"));
    let duplicate = service
        .add_component("reviewer", reviewer(), Vec::<Binding>::new())
        .expect_err("a reused alias");
    assert_eq!(rule(&duplicate), Some("duplicate_alias"));
    assert_eq!(
        verifier.spec().steps.len(),
        1,
        "a refused edit changes nothing"
    );
    assert_eq!(
        service.spec().components.len(),
        1,
        "a refused edit changes nothing"
    );

    service
        .add_component("controls", agent, Vec::<Binding>::new())
        .expect("new alias");
    let conflict = CardDefinition::from(service)
        .declaration()
        .expect_err("two different report-reviewer 1.0.0 definitions");
    assert!(
        matches!(conflict, WyrdError::SpecDuplicateSubmission { .. }),
        "{conflict:?}"
    );

    assert!(matches!(
        Agent::new(
            "report-reviewer",
            "^1.0",
            Prompt::new("openai", "gpt-4o", "Review").expect("prompt compiles"),
        ),
        Err(WyrdError::RegistryInvalidVersionBlock { .. })
    ));
}
