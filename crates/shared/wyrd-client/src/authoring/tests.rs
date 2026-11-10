//! Offline authoring: canonical parity, value capture, deferred validation,
//! shared diagnostics, and file-authored parity, with no server.

use serde_json::{Value, json};
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::service::ServiceSpec;
use wyrd_spec::card::verifier::VerifierSpec;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::TaskId;

use super::*;

/// Each diagnostic of a refused graph as `(code, message)`, empty for any
/// error that carries none.
fn diagnostics(error: &WyrdError) -> Vec<(String, String)> {
    let WyrdError::LoaderInvalidEnvelope { details, .. } = error else {
        return Vec::new();
    };
    details["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|diagnostic| {
            (
                diagnostic["code"].as_str().unwrap_or_default().to_owned(),
                diagnostic["message"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

/// Whether some diagnostic of `error` has a message containing `needle`.
fn refuses(error: &WyrdError, needle: &str) -> bool {
    diagnostics(error)
        .iter()
        .any(|(_, message)| message.contains(needle))
}

/// The `report-reviewer` Agent: a gateway-routed inline verdict Prompt
/// reading `/inputs/policy.json` with `fs.read` for at most three turns.
fn reviewer() -> Agent {
    Agent::new("report-reviewer", "1.0.0")
        .prompt(json!({
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Review {{report}} against /inputs/policy.json"}],
            "response_format": "verdict",
        }))
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "openai"}))
        .tool_names(["fs.read"])
        .run_config(json!({"max_iterations": 3}))
}

/// The local `company-sharing-policy` Artifact, extracted on publication;
/// its source path is never read offline.
fn policy() -> Artifact {
    Artifact::from_path("company-sharing-policy", "1.0.0", "policy.pdf").extract(true)
}

/// The exact reference publishing the Artifact `name` at `version` returns.
///
/// # Panics
/// Panics when the reference does not decode.
fn published_ref(name: &str, version: &str) -> wyrd_spec::reference::CardRef {
    serde_json::from_value(json!({"kind": "Artifact", "name": name, "version": version}))
        .expect("an exact reference decodes")
}

/// The spec `card` decodes to, as its YAML file's would.
///
/// # Panics
/// Panics when the spec does not decode as a `T`.
fn decoded<T: serde::de::DeserializeOwned>(spec: Value) -> T {
    serde_json::from_value(spec).expect("the authored spec decodes")
}

/// Decode `text` as YAML into `T` through its JSON form, as the loader
/// does.
///
/// # Panics
/// Panics when `text` is not YAML or not a `T`.
fn yaml<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_value(serde_yaml::from_str::<Value>(text).expect("YAML parses"))
        .expect("canonical value decodes")
}

/// A Verifier exercising every Action and Check variant, a bare JSON Schema
/// input, an explicit dependency, and both editing styles.
fn every_primitive() -> Verifier {
    let mut verifier = Verifier::new("release-access", "1.0.0")
        .description("Every authoring primitive")
        .inputs([("report", InputSlot::json(json!({"type": "object"})))]);
    verifier
        .add_input("purpose", json!({"type": "string"}))
        .add_input("document", InputSlot::media(["pdf"]))
        .add_input("trace", InputSlot::trace());

    let mut review = Action::agent(reviewer()).inputs([("report", "inputs.report")]);
    review
        .add_workspace("/inputs/report.json", "inputs.report")
        .add_workspace(
            "/inputs/policy.json",
            policy().file("extracted/company-sharing-policy.pdf.json"),
        );
    let policy_step = Step::new("policy", review).requirements([Requirement::new(
        "findings-cited",
        "Every finding cites the policy",
        Check::each("steps.policy.output.findings[*].cited", "equals", true),
    )]);

    let mut context = Action::code(
        CodeSource::inline("def policy_context(report, purpose):\n    return {}\n"),
        "policy_context",
    );
    context
        .bind_input("report", "inputs.report")
        .bind_input("purpose", "inputs.purpose");
    let mut context_step = Step::new("policy-context", context)
        .when(json!({"path": "steps.policy.output.verdict", "operator": "equals", "expected": "pass"}))
        .timeout_seconds(30)
        .retries(2);
    context_step
        .add_depends_on("policy")
        .add_requirement(Requirement::new(
            "has-report",
            "The context has a report",
            Check::assert("steps.policy-context.output.has_report", "equals", true),
        ));

    verifier
        .add_step(Step::new("extract", Action::extract("inputs.document")))
        .add_step(policy_step)
        .add_step(context_step);
    verifier
        .add_requirement(Requirement::new(
            "trace-order",
            "Judged",
            Check::trace("inputs.trace", "$.spans[0].name", "equals", "lookup"),
        ))
        .add_requirement(Requirement::new(
            "judged",
            "Judged",
            Check::agent(CardRef::agent("judge", "1.0.0")),
        ))
        .add_requirement(Requirement::new(
            "code-judged",
            "Judged",
            Check::code(
                CardRef::artifact("release-checks", "1.0.0").file("release_checks.py"),
                "summarize_findings",
            ),
        ))
        .add_requirement(Requirement::new(
            "code-formatted",
            "Judged",
            Check::code(CodeSource::inline("def judge():\n    return {}\n"), "judge")
                .response_format("verdict"),
        ));
    verifier.decide(json!({"pass_rate": {"threshold": 0.5}}))
}

/// The stages `verifier` plans, as step IDs.
///
/// # Errors
/// Returns [`Verifier::plan`]'s refusal.
fn stages(verifier: &Verifier) -> Result<Vec<Vec<String>>, WyrdError> {
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
}

/// Every authored primitive builds the canonical spec its file form decodes
/// to, passes the shared validation, and stages by explicit and inferred
/// edges; the composed Service declares its local Artifact once, its root
/// last, and no local path or derived status.
///
/// # Panics
/// Panics when an authored spec, plan, or declaration differs from its
/// canonical form.
#[test]
fn core_primitives_match_canonical_contracts() {
    let verifier = every_primitive();
    let canonical: VerifierSpec = yaml(
        r#"
description: Every authoring primitive
inputs:
  report: {type: json, schema: {type: object}}
  purpose: {type: json, schema: {type: string}}
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
            card: {sibling: {kind: Artifact, name: company-sharing-policy, version: 1.0.0}}
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
    );
    assert_eq!(decoded::<VerifierSpec>(verifier.spec()), canonical);
    verifier
        .validate()
        .expect("shared validation accepts the authored Verifier");

    let drift = Verifier::new("latency-drift", "1.0.0")
        .inputs([("latency", InputSlot::samples(["latency"]))])
        .requirements([Requirement::new(
            "latency-stable",
            "Latency is in control",
            Check::drift(
                "inputs.latency",
                json!({
                    "method": "Spc",
                    "signal": {"kind": "Distribution", "baseline_ref": {"kind": "Data", "name": "latency-baseline", "version": "1.0.0"}, "features": ["latency"]},
                    "condition": {"kind": "Statistical"},
                    "profile": {"kind": "Spc", "sample_size": 5},
                }),
            ),
        )]);
    let canonical_drift: VerifierSpec = yaml(
        "
inputs: {latency: {type: samples, features: [latency]}}
requirements:
  - id: latency-stable
    statement: Latency is in control
    check:
      kind: drift
      on: inputs.latency
      method: Spc
      signal: {kind: Distribution, baseline_ref: {kind: Data, name: latency-baseline, version: 1.0.0}, features: [latency]}
      condition: {kind: Statistical}
      profile: {kind: Spc, sample_size: 5}
",
    );
    assert_eq!(decoded::<VerifierSpec>(drift.spec()), canonical_drift);
    drift
        .validate()
        .expect("shared validation accepts the drift Verifier");

    let plan = verifier.plan().expect("plans");
    let staged: Vec<Vec<&str>> = plan
        .stages
        .iter()
        .map(|stage| stage.tasks.iter().map(TaskId::as_str).collect())
        .collect();
    assert_eq!(
        staged,
        [vec!["extract", "policy"], vec!["policy-context"]],
        "the policy-context dependency stages it after policy",
    );

    let ordering = Verifier::new("ordering", "1.0.0")
        .inputs([("value", InputSlot::json(Value::Null))])
        .steps([
            Step::new("first", Action::extract("inputs.value")),
            Step::new("explicit", Action::extract("inputs.value")).depends_on(["first"]),
            Step::new(
                "inferred",
                Action::code(CodeSource::inline("def f(x):\n    return x\n"), "f")
                    .inputs([("x", "steps.first.output")]),
            ),
        ]);
    assert_eq!(
        stages(&ordering).expect("plans"),
        [vec!["first"], vec!["explicit", "inferred"]],
        "an explicit-only depends_on edge stages like an inferred output reference",
    );
    let mut cyclic = ordering.clone();
    cyclic.add_step(Step::new("cyclic", Action::extract("inputs.value")).depends_on(["cyclic"]));
    assert!(stages(&cyclic).is_err(), "a self dependency is refused");
    let mut dangling = ordering;
    dangling.add_step(
        Step::new("dangling", Action::extract("steps.unknown.output")).depends_on(["unknown"]),
    );
    assert!(
        stages(&dangling).is_err(),
        "an unknown dependency is refused"
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
    );
    assert_eq!(decoded::<AgentSpec>(reviewer().spec()), canonical_agent);
    let native = reviewer().execution_route(json!({"kind": "native"}));
    assert_eq!(
        decoded::<AgentSpec>(native.spec()).execution_route,
        wyrd_spec::ExecutionRoute::Native {}
    );

    let mut service = Service::new("finance-release", "1.0.0").service_type("api");
    service
        .add_component(Component::new("reviewer", reviewer()).verified_by([verifier.clone()]))
        .add_component(
            Component::new("controls", CardRef::agent("release-controls", "1.0.0"))
                .verified_by([CardRef::verifier("report-policy", "1.0.0")]),
        );
    let canonical_service: ServiceSpec = yaml(
        "
service_type: api
components:
  - alias: reviewer
    ref: {sibling: {kind: Agent, name: report-reviewer, version: 1.0.0}}
    verified_by: [{verifier: {sibling: {kind: Verifier, name: release-access, version: 1.0.0}}}]
  - alias: controls
    ref: {kind: Agent, name: release-controls, version: 1.0.0}
    verified_by: [{verifier: {kind: Verifier, name: report-policy, version: 1.0.0}}]
",
    );
    assert_eq!(decoded::<ServiceSpec>(service.spec()), canonical_service);

    let declaration = CardDefinition::from(&service)
        .declaration()
        .expect("valid graph");
    assert_eq!(
        CardDefinition::from(&service)
            .declaration()
            .expect("valid graph"),
        declaration,
        "repeated declaration is stable",
    );
    assert_eq!(
        declaration.artifacts.len(),
        1,
        "the local Artifact publishes first, once"
    );
    let local = &declaration.artifacts[0];
    assert_eq!(local.metadata.name.as_str(), "company-sharing-policy");
    assert_eq!(local.spec.artifact_kind, "file");
    assert_eq!(local.source, std::path::Path::new("policy.pdf"));
    assert!(local.extract);
    let kinds: Vec<CardKind> = declaration
        .input
        .as_ref()
        .expect("an ordinary graph")
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
    assert!(
        declaration
            .input
            .as_ref()
            .expect("an ordinary graph")
            .artifact_sources
            .is_empty()
    );
    let wire = serde_json::to_string(
        &declaration
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions,
    )
    .expect("serializes");
    assert!(
        !wire.contains("policy.pdf\""),
        "no local path reaches a submission: {wire}"
    );
    assert!(
        !wire.contains("\"status\""),
        "no derived status reaches a submission: {wire}"
    );

    let selected = |file: ArtifactFile| {
        CardDefinition::from(
            Verifier::new("coded", "1.0.0")
                .inputs([("report", InputSlot::json(Value::Null))])
                .steps([Step::new("facts", Action::code(file, "facts"))]),
        )
        .declaration()
    };
    let descriptor = selected(
        Artifact::new("checks", "1.0.0")
            .artifact_kind("code")
            .file("checks.py"),
    )
    .expect("a descriptor selection is a sibling of one Verifier root");
    assert!(descriptor.artifacts.is_empty(), "nothing is published");
    let input = descriptor.input.expect("an ordinary graph");
    assert_eq!(
        input
            .submissions
            .iter()
            .map(|submission| submission.kind.clone())
            .collect::<Vec<_>>(),
        [CardKind::Artifact, CardKind::Verifier]
    );
    assert_eq!(
        input.submissions[1].spec["steps"][0]["action"]["source"],
        json!({"artifact": {"card": {"sibling": {"kind": "Artifact", "name": "checks", "version": "1.0.0"}}, "path": "checks.py"}})
    );
    let external =
        json!({"kind": "Artifact", "name": "company-sharing-policy", "version": "1.0.0"});
    for (file, publishes) in [
        (policy().file("checks.py"), 1),
        (
            CardRef::artifact("company-sharing-policy", "1.0.0").file("checks.py"),
            0,
        ),
    ] {
        let mut declaration = selected(file).expect("valid graph");
        assert_eq!(declaration.artifacts.len(), publishes);
        declaration
            .bind_published(&published_ref("company-sharing-policy", "1.0.0"))
            .expect("the published policy binds");
        assert_eq!(
            declaration.input.expect("an ordinary graph").submissions[0].spec["steps"][0]["action"]
                ["source"]["artifact"]["card"],
            external,
            "local selections bind to their published reference; registered ones stay external"
        );
    }

    policy()
        .validate()
        .expect("a local Artifact validates as its own root");
    let root = CardDefinition::from(policy().labels([("team", "finance")]))
        .declaration()
        .expect("valid local root");
    assert_eq!(root.input, None, "a local root declares no graph");
    let [local] = root.artifacts.as_slice() else {
        panic!("the root is its own publication: {root:?}");
    };
    assert_eq!(local.source, std::path::Path::new("policy.pdf"));
    assert!(local.extract);
    assert_eq!(
        serde_json::to_value(&local.metadata.labels).expect("serializes"),
        json!({"team": "finance"})
    );
    let refused = policy()
        .artifact_kind("")
        .validate()
        .expect_err("a blank artifact_kind");
    assert!(refuses(&refused, "artifact_kind"), "{refused:?}");
}

/// Adding a child captures its value, attempted duplicates are kept and
/// refused at validation, a whole-field setter resets that field alone, and
/// two different definitions of one identity refuse the declaration.
///
/// # Panics
/// Panics when a later edit reaches a captured child, a duplicate is
/// admitted or lost, a reset reaches another field, or the conflict is not
/// refused.
#[test]
fn offline_edits_preserve_snapshots() {
    let mut verifier =
        Verifier::new("report-policy", "1.0.0").inputs([("report", InputSlot::json(Value::Null))]);
    let mut step = Step::new("policy", Action::agent(reviewer()));
    verifier.add_step(step.clone());
    step = step.timeout_seconds(5);
    assert!(
        verifier.spec()["steps"][0].get("timeout_seconds").is_none(),
        "a step is captured when added: {step:?}"
    );

    let agent = reviewer();
    let mut service = Service::new("finance-release", "1.0.0");
    service
        .add_component(Component::new("reviewer", agent.clone()).verified_by([verifier.clone()]));
    let agent = agent.run_config(json!({"max_iterations": 9}));
    verifier.add_requirement(Requirement::new(
        "has-report",
        "Has a report",
        Check::assert("inputs.report", "equals", true),
    ));
    let declaration = CardDefinition::from(&service)
        .declaration()
        .expect("valid graph");
    for submission in &declaration
        .input
        .as_ref()
        .expect("an ordinary graph")
        .submissions
    {
        match submission.kind {
            CardKind::Agent => assert_eq!(submission.spec["run_config"]["max_iterations"], 3),
            CardKind::Verifier => assert!(submission.spec.get("requirements").is_none()),
            _ => {}
        }
    }

    let mut duplicated = verifier.clone();
    duplicated
        .add_step(Step::new("has-report", Action::extract("inputs.report")))
        .add_input("report", InputSlot::trace());
    assert_eq!(
        duplicated.spec()["steps"].as_array().map(Vec::len),
        Some(2),
        "a duplicate step ID is retained"
    );
    let refused = duplicated.validate().expect_err("duplicate IDs and slots");
    assert!(
        refuses(&refused, ": inputs.report: duplicate entry"),
        "{refused:?}"
    );
    let mut duplicated = verifier.clone();
    duplicated.add_step(Step::new("has-report", Action::extract("inputs.report")));
    let refused = duplicated
        .validate()
        .expect_err("a step reusing a requirement ID");
    assert!(refuses(&refused, "has-report"), "{refused:?}");

    let reset = duplicated
        .clone()
        .inputs([("report", InputSlot::json(Value::Null))]);
    assert_eq!(
        reset.spec()["steps"],
        duplicated.spec()["steps"],
        "replacing inputs leaves steps"
    );
    let mut aliases = service.clone();
    aliases.add_component(Component::new("reviewer", reviewer()));
    let refused = aliases.validate().expect_err("a reused alias");
    assert!(refuses(&refused, "reviewer"), "{refused:?}");
    let reset = aliases.components([Component::new("reviewer", reviewer())]);
    reset
        .validate()
        .expect("replacing components drops the duplicate attempt");

    let mut duplicate_key = Verifier::new("keys", "1.0.0");
    duplicate_key
        .add_input("report", InputSlot::json(Value::Null))
        .add_input("report", InputSlot::trace());
    assert!(
        duplicate_key.validate().is_err(),
        "a repeated key is refused"
    );
    let duplicate_key = duplicate_key.inputs([("report", InputSlot::json(Value::Null))]);
    duplicate_key
        .validate()
        .expect("replacing inputs drops the earlier duplicate attempt");

    service.add_component(Component::new("controls", agent));
    let conflict = CardDefinition::from(service)
        .declaration()
        .expect_err("two different report-reviewer 1.0.0 definitions");
    assert!(
        matches!(conflict, WyrdError::SpecDuplicateSubmission { .. }),
        "{conflict:?}"
    );

    let child = || Service::new("shared-child", "1.0.0").service_type("api");
    let composed = |second: Service| {
        CardDefinition::from(
            Service::new("composite", "1.0.0")
                .service_type("api")
                .components([
                    Component::new("first", child()),
                    Component::new("second", second),
                ]),
        )
        .declaration()
    };
    let explicit = composed(child().components([] as [Component; 0]))
        .expect("an omitted and an explicitly empty list decode alike");
    assert_eq!(
        explicit,
        composed(child()).expect("valid graph"),
        "the explicit default declares as the omitted one"
    );
    assert_eq!(
        explicit
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions
            .iter()
            .filter(|submission| submission.metadata.name.as_str() == "shared-child")
            .count(),
        1,
        "canonically equal definitions are one submission"
    );
    assert!(
        matches!(
            composed(child().description("different")),
            Err(WyrdError::SpecDuplicateSubmission { .. })
        ),
        "a canonically different definition still conflicts"
    );

    let mut audit = Audit::new("release-audit", "1.0.0");
    audit
        .add_evidence_ref(Artifact::new("company-sharing-policy", "1.0.0").artifact_kind("file"))
        .add_evidence_ref(policy());
    assert!(
        matches!(
            CardDefinition::from(audit).declaration(),
            Err(WyrdError::SpecDuplicateSubmission { .. })
        ),
        "a local and a spec-only Artifact of one identity conflict"
    );
}

/// Construction and edits accept anything typed correctly; validation and
/// declaration refuse each complete invalid snapshot by its YAML path,
/// revalidate later edits, and read code files only then.
///
/// # Panics
/// Panics when an edit refuses early, a refusal is missing or misattributed,
/// a file is read before validation, or a later edit escapes validation.
#[test]
fn field_edits_defer_validation_and_preserve_snapshots() {
    let invalid = Verifier::new("Not A Name!", "^1.0");
    let refused = invalid.validate().expect_err("invalid identity");
    assert!(!diagnostics(&refused).is_empty(), "{refused:?}");

    let mut verifier = Verifier::new("report-policy", "1.0.0");
    verifier.add_requirement(Requirement::new(
        "uses-later-input",
        "Reads a slot added later",
        Check::assert("inputs.report", "approximately", true),
    ));
    let refused = verifier.validate().expect_err("invalid operator");
    assert!(
        refuses(&refused, ": requirements[0].check: "),
        "the operator is refused at its YAML path: {refused:?}"
    );
    let verifier = verifier.requirements([Requirement::new(
        "uses-later-input",
        "Reads a slot added later",
        Check::assert("inputs.report", "equals", true),
    )]);
    let refused = verifier.validate().expect_err("unknown input");
    assert!(refuses(&refused, "report"), "{refused:?}");
    let mut verifier = verifier;
    verifier.add_input("report", InputSlot::json(Value::Null));
    verifier
        .validate()
        .expect("the forward reference is valid once the slot exists");

    let directory = tempfile::tempdir().expect("scratch directory");
    let source = directory.path().join("checks.py");
    let mut coded = verifier.clone();
    coded.add_step(Step::new(
        "facts",
        Action::code(CodeSource::file(&source), "facts").inputs([("report", "inputs.report")]),
    ));
    assert!(
        coded.spec()["steps"][0]["action"]["source"].is_null(),
        "nothing is read on edit"
    );
    let refused = coded.validate().expect_err("the source does not exist yet");
    assert_eq!(
        diagnostics(&refused)
            .into_iter()
            .map(|(code, _)| code)
            .collect::<Vec<_>>(),
        ["WYRD_LOADER_400_IO"],
        "{refused:?}"
    );
    std::fs::write(&source, "def facts(report):\n    return {}\n").expect("source writes");
    let first = CardDefinition::from(&coded)
        .declaration()
        .expect("the source reads");
    std::fs::write(&source, "def facts(report):\n    return {'v': 2}\n").expect("source writes");
    let second = CardDefinition::from(&coded)
        .declaration()
        .expect("the source reads");
    assert_ne!(first, second, "every snapshot reads the file afresh");
    assert_eq!(
        second
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions[0]
            .spec["steps"][0]["action"]["source"],
        json!({"inline": "def facts(report):\n    return {'v': 2}\n"})
    );

    coded.validate().expect("valid");
    coded.add_input("broken", InputSlot::json(Value::Null).max_bytes("yes"));
    let refused = coded
        .validate()
        .expect_err("an edit after validate is revalidated");
    assert!(refuses(&refused, ": inputs.broken"), "{refused:?}");
}

/// The same invalid Card refuses with the same diagnostic code and message,
/// apart from its source label, whether authored as YAML or code.
///
/// # Panics
/// Panics when either form is admitted or their diagnostics differ.
#[test]
fn yaml_and_code_share_diagnostics() {
    let directory = tempfile::tempdir().expect("scratch directory");
    let cases: [(&str, Verifier); 3] = [
        (
            "inputs: {report: {type: json}}\nrequirements:\n  - {id: r, statement: s, check: {kind: assert, path: inputs.report, operator: approximately, expected: true}}\n",
            Verifier::new("shared", "1.0.0")
                .inputs([("report", InputSlot::json(Value::Null))])
                .requirements([Requirement::new(
                    "r",
                    "s",
                    Check::assert("inputs.report", "approximately", true),
                )]),
        ),
        (
            "requirements:\n  - {id: r, statement: s, check: {kind: assert, path: inputs.missing, operator: equals, expected: true}}\n",
            Verifier::new("shared", "1.0.0").requirements([Requirement::new(
                "r",
                "s",
                Check::assert("inputs.missing", "equals", true),
            )]),
        ),
        (
            "inputs: {report: {type: trace, max_bytes: 1}}\n",
            Verifier::new("shared", "1.0.0")
                .inputs([("report", InputSlot::json(Value::Null))])
                .inputs([("report", InputSlot::trace().max_bytes(1))]),
        ),
    ];
    for (index, (spec, code)) in cases.into_iter().enumerate() {
        let file = directory.path().join(format!("case-{index}.yaml"));
        std::fs::write(
            &file,
            format!(
                "apiVersion: wyrd/v1\nkind: Verifier\nmetadata: {{name: shared, version: 1.0.0}}\nspec:\n{}",
                spec.lines().flat_map(|line| ["  ", line, "\n"]).collect::<String>()
            ),
        )
        .expect("case writes");
        let from_file = wyrd_loader::load(&file)
            .and_then(wyrd_loader::build_declaration_input)
            .map_err(WyrdError::from)
            .expect_err("the YAML case is invalid");
        let from_code = code.validate().expect_err("the code case is invalid");
        let strip = |error: &WyrdError, label: &str| -> Vec<(String, String)> {
            diagnostics(error)
                .into_iter()
                .map(|(code, message)| (code, message.replace(label, "<card>")))
                .collect()
        };
        let file_label = file.display().to_string();
        let file_diagnostics = strip(&from_file, &file_label);
        assert!(!file_diagnostics.is_empty(), "case {index}: {from_file:?}");
        assert_eq!(
            strip(&from_code, "Verifier/shared@1.0.0"),
            file_diagnostics,
            "case {index} refuses alike"
        );
    }

    for (index, (yaml, raw)) in [("malformed", json!("malformed")), ("null", Value::Null)]
        .into_iter()
        .enumerate()
    {
        let file = directory.path().join(format!("raw-{index}.yaml"));
        std::fs::write(
            &file,
            format!(
                "apiVersion: wyrd/v1\nkind: Service\nmetadata: {{name: shared, version: 1.0.0}}\nspec: {yaml}\n"
            ),
        )
        .expect("case writes");
        let from_file = wyrd_loader::load(&file)
            .and_then(wyrd_loader::build_declaration_input)
            .map_err(WyrdError::from)
            .expect_err("the YAML spec is not an object");
        let from_code = CardDefinition(Card::with_spec(
            CardKind::Service,
            "shared",
            Some("1.0.0"),
            Body::authored(raw),
        ))
        .declaration()
        .expect_err("the raw spec is not an object");
        let file_label = file.display().to_string();
        let file_diagnostics: Vec<_> = diagnostics(&from_file)
            .into_iter()
            .map(|(code, message)| (code, message.replace(&file_label, "<card>")))
            .collect();
        assert!(!file_diagnostics.is_empty(), "raw {yaml}: {from_file:?}");
        assert_eq!(
            diagnostics(&from_code)
                .into_iter()
                .map(|(code, message)| (code, message.replace("Service/shared@1.0.0", "<card>")))
                .collect::<Vec<_>>(),
            file_diagnostics,
            "raw {yaml} refuses alike"
        );
    }
    CardDefinition(Card::with_spec(
        CardKind::Service,
        "shared",
        Some("1.0.0"),
        Body::authored(json!({})),
    ))
    .declaration()
    .expect("an empty object is a valid Service spec");
}

/// The `card_authoring` story's checked-in file at `relative`.
fn story(relative: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/cards/card_authoring")
        .join(relative)
}

/// A policy-judge Agent named `name`, as the story's Agent files declare it.
fn policy_judge(name: &str) -> Agent {
    Agent::new(name, "1.0.0")
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "openai"}))
        .tool_names(["fs.read"])
        .run_config(json!({"max_iterations": 3}))
        .prompt(json!({
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{
                "role": "user",
                "content": "POLICY JUDGE: read /inputs/policy.json with fs.read. Determine whether the proposed release satisfies the company policy. Cite original request and policy evidence. Report: {{report}} Recipient: {{recipient}} Purpose: {{purpose}} Destination: {{destination}}",
            }],
            "response_format": "verdict",
        }))
}

/// A code step running `entrypoint` of the story's checks over `inputs`,
/// judged by one `steps.<id>.output.<path> == expected` requirement.
fn fact_step(
    id: &str,
    entrypoint: &str,
    inputs: &[&str],
    (requirement, statement, path, expected): (&str, &str, &str, Value),
) -> Step {
    let action = Action::code(CodeSource::file(story("release_checks.py")), entrypoint)
        .inputs(inputs.iter().map(|name| (*name, format!("inputs.{name}"))));
    Step::new(id, action).requirements([Requirement::new(
        requirement,
        statement,
        Check::assert(format!("steps.{id}.output.{path}"), "equals", expected),
    )])
}

/// The recipient, purpose, and destination slots.
fn release_inputs() -> [(&'static str, InputSlot); 3] {
    [
        (
            "recipient",
            InputSlot::json(json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["email", "authorized", "permitted_purposes"],
                "properties": {
                    "email": {"type": "string"},
                    "authorized": {"type": "boolean"},
                    "permitted_purposes": {"type": "array", "items": {"type": "string"}},
                },
            })),
        ),
        ("purpose", InputSlot::json(json!({"type": "string"}))),
        (
            "destination",
            InputSlot::json(json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["url", "encrypted", "restricted_access", "public_sharing"],
                "properties": {
                    "url": {"type": "string"},
                    "encrypted": {"type": "boolean"},
                    "restricted_access": {"type": "boolean"},
                    "public_sharing": {"type": "boolean"},
                },
            })),
        ),
    ]
}

/// The `card_authoring` story's Service, authored in code, with the report
/// judge mounting `policy`.
fn finance_release(policy: ArtifactFile) -> Service {
    let reviewer = policy_judge("report-reviewer");
    let mut report_policy = Verifier::new("report-policy", "1.0.0")
        .description("The report is fit to share under the company sharing policy.")
        .inputs(release_inputs());
    report_policy.add_input(
        "report",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["text"],
            "properties": {"text": {"type": "string"}},
        }),
    );
    let mut judge = Check::agent(reviewer.clone())
        .inputs(["report", "recipient", "purpose"].map(|name| (name, format!("inputs.{name}"))));
    judge
        .bind_input("destination", "inputs.destination")
        .add_workspace("/inputs/policy.json", policy);
    let mut scan = fact_step(
        "policy",
        "report_facts",
        &["report"],
        (
            "no-bank-account-identifier",
            "The report contains no prohibited bank identifier.",
            "contains_bank_identifier",
            json!(false),
        ),
    );
    scan.add_requirement(Requirement::new(
        "sharing-policy-satisfied",
        "The release satisfies the company sharing policy.",
        judge,
    ));
    let report_policy = report_policy.steps([
        scan,
        fact_step(
            "policy-context",
            "policy_context",
            &["report", "purpose"],
            (
                "usable-report",
                "The proposed release contains report text.",
                "has_report",
                json!(true),
            ),
        ),
    ]);

    let mut release_access = Verifier::new("release-access", "1.0.0")
        .description("The recipient and destination may receive the report.")
        .inputs(release_inputs());
    release_access
        .add_input("notes", json!({"type": "string"}))
        .add_step(fact_step(
            "recipient",
            "recipient_facts",
            &["recipient", "purpose"],
            (
                "recipient-authorized",
                "The recipient is authorized for this purpose.",
                "authorized",
                json!(true),
            ),
        ))
        .add_step(fact_step(
            "destination",
            "destination_facts",
            &["destination"],
            (
                "destination-controls-satisfied",
                "The destination has appropriate access controls.",
                "appropriate",
                json!(true),
            ),
        ));

    Service::new("finance-release", "1.0.0")
        .service_type("api")
        .components([
            Component::new("report-reviewer", reviewer).verified_by([report_policy]),
            Component::new("release-controls", policy_judge("release-controls"))
                .verified_by([release_access]),
        ])
}

/// The story authored in code declares the same identities, metadata, and
/// specs as its checked-in file tree, publishes only its policy first, and
/// declares the same root last.
///
/// # Panics
/// Panics when the story does not load or any submission differs.
#[test]
fn declarations_match_file_authored_tree() {
    let policy = Artifact::from_path(
        "company-sharing-policy",
        "1.0.0",
        story("company-sharing-policy.pdf"),
    )
    .extract(true);
    let mut authored = CardDefinition::from(finance_release(
        policy.file("extracted/company-sharing-policy.pdf.json"),
    ))
    .declaration()
    .expect("valid graph");
    authored
        .bind_published(&published_ref("company-sharing-policy", "1.0.0"))
        .expect("the published policy binds");
    let files = wyrd_loader::build_declaration_input(
        wyrd_loader::load(&story("finance-release.yaml")).expect("the story loads"),
    )
    .expect("the story builds");

    assert_eq!(
        authored.artifacts.len(),
        1,
        "the policy publishes first, alone"
    );
    assert_eq!(
        authored.artifacts[0].source,
        story("company-sharing-policy.pdf")
    );
    let identity = |submission: &wyrd_spec::registry::CardSubmission| {
        (
            format!("{:?}", submission.kind),
            submission.metadata.name.to_string(),
        )
    };
    let mut authored_submissions = authored
        .input
        .as_ref()
        .expect("an ordinary graph")
        .submissions
        .clone();
    let mut file_submissions = files.submissions.clone();
    authored_submissions.sort_by_key(identity);
    file_submissions.sort_by_key(identity);
    assert_eq!(authored_submissions.len(), 5);
    for (authored, file) in authored_submissions.iter().zip(&file_submissions) {
        assert_eq!(identity(authored), identity(file));
        assert_eq!(
            authored.spec,
            file.spec,
            "{:?} matches its file",
            identity(file)
        );
        assert_eq!(authored.metadata, file.metadata);
        assert!(authored.artifacts.is_empty() && file.artifacts.is_empty());
    }
    assert_eq!(
        authored
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions
            .last()
            .map(identity),
        files.submissions.last().map(identity),
        "both declare the Service root last",
    );
}

/// A sibling `daily` 1.0.0 Trigger firing on `cron`.
fn daily(cron: &str) -> Trigger {
    Trigger::new("daily", "1.0.0").kind("schedule").cron(cron)
}

/// The declaration of a Verifier with a `report` input whose one Agent step
/// makes `call`.
///
/// # Errors
/// Returns [`CardDefinition::declaration`]'s refusal.
fn call_declaration(call: impl Into<Action>) -> Result<Declaration, WyrdError> {
    Verifier::new("workspace", "1.0.0")
        .inputs([("report", InputSlot::json(Value::Null))])
        .steps([Step::new("judge", call)])
        .card_definition()
        .declaration()
}

impl Verifier {
    /// This Verifier as a root definition.
    fn card_definition(self) -> CardDefinition {
        CardDefinition::from(self)
    }
}

/// Replacing a Trigger, a component, or a workspace destination leaves
/// exactly the dependencies of the current values: an edited definition
/// declares and publishes what the same definition built directly does,
/// while two current conflicting definitions still refuse.
///
/// # Panics
/// Panics when an edited value differs from its direct construction, an
/// obsolete Trigger or Artifact is declared or published, a still-mounted
/// Artifact is dropped, or the current conflict is admitted.
#[test]
fn replacements_keep_only_current_dependencies() {
    let verifier = Verifier::new("checked", "1.0.0");
    let declare = |binding: Binding| {
        reviewer()
            .verified_by([binding])
            .card_definition()
            .declaration()
    };
    let direct = Binding::new(verifier.clone()).runs_on(daily("0 6 * * *"));
    let edited = Binding::new(verifier.clone())
        .runs_on(daily("0 0 * * *"))
        .runs_on(daily("0 6 * * *"));
    assert_eq!(edited, direct, "replacement equals direct construction");
    let declaration = declare(edited).expect("only the current Trigger is declared");
    assert_eq!(declaration, declare(direct).expect("valid graph"));
    let triggers: Vec<&Value> = declaration
        .input
        .as_ref()
        .expect("an ordinary graph")
        .submissions
        .iter()
        .filter(|submission| submission.kind == CardKind::Trigger)
        .map(|submission| &submission.spec)
        .collect();
    assert_eq!(
        triggers,
        [&json!({"kind": "schedule", "cron": "0 6 * * *"})]
    );
    let registered = Binding::new(verifier.clone())
        .runs_on(daily("0 0 * * *"))
        .runs_on(daily("0 6 * * *").card_ref().expect("a versioned Trigger"));
    assert!(
        declare(registered)
            .expect("valid graph")
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions
            .iter()
            .all(|submission| submission.kind != CardKind::Trigger),
        "a registered Trigger replaces the sibling submission",
    );

    let first = Binding::new(verifier.clone()).runs_on(daily("0 0 * * *"));
    let second = Binding::new(Verifier::new("other", "1.0.0")).runs_on(daily("0 6 * * *"));
    assert!(
        matches!(
            reviewer()
                .verified_by([first, second])
                .card_definition()
                .declaration(),
            Err(WyrdError::SpecDuplicateSubmission { .. })
        ),
        "two current definitions of daily 1.0.0 still conflict",
    );

    let mut replaced = Service::new("finance-release", "1.0.0");
    replaced.add_component(Component::new("a", policy_judge("old-judge")));
    let replaced = replaced.components([Component::new("a", reviewer())]);
    let names: Vec<String> = CardDefinition::from(replaced)
        .declaration()
        .expect("valid graph")
        .input
        .as_ref()
        .expect("an ordinary graph")
        .submissions
        .iter()
        .map(|submission| submission.metadata.name.to_string())
        .collect();
    assert!(
        !names.contains(&"old-judge".to_owned()),
        "a replaced component's Card is not declared: {names:?}"
    );

    let obsolete = Artifact::from_path("obsolete-policy", "1.0.0", "missing/obsolete.pdf");
    let mut to_scope = Action::agent(reviewer());
    to_scope
        .add_workspace("/inputs/policy.json", obsolete.file("obsolete.pdf"))
        .add_workspace("/inputs/report.json", "inputs.report");
    let to_scope = to_scope.workspace([("/inputs/policy.json", "inputs.report")]);
    let scope_only =
        Action::agent(reviewer()).workspace([("/inputs/policy.json", "inputs.report")]);
    assert_eq!(
        to_scope, scope_only,
        "replacement equals direct construction"
    );
    assert!(
        call_declaration(to_scope)
            .expect("valid graph")
            .artifacts
            .is_empty(),
        "a replaced local Artifact is not published",
    );

    let to_file = Action::agent(reviewer())
        .workspace([("/inputs/policy.json", obsolete.file("obsolete.pdf"))])
        .workspace([("/inputs/policy.json", policy().file("policy.pdf"))]);
    let file_only =
        Action::agent(reviewer()).workspace([("/inputs/policy.json", policy().file("policy.pdf"))]);
    assert_eq!(to_file, file_only, "replacement equals direct construction");
    let published = call_declaration(to_file).expect("valid graph").artifacts;
    assert_eq!(
        published.len(),
        1,
        "only the current local Artifact is published"
    );
    assert_eq!(
        published[0].metadata.name.as_str(),
        "company-sharing-policy"
    );

    let mut shared = Action::agent(reviewer());
    shared
        .add_workspace("/inputs/policy.json", policy().file("policy.pdf"))
        .add_workspace("/inputs/copy.json", policy().file("policy.pdf"));
    let shared = shared.workspace([
        ("/inputs/policy.json", FieldValue::from("inputs.report")),
        ("/inputs/copy.json", policy().file("policy.pdf").into()),
    ]);
    assert_eq!(
        call_declaration(shared)
            .expect("valid graph")
            .artifacts
            .len(),
        1,
        "an Artifact still mounted elsewhere is still published",
    );

    let schema = |name: &str| Artifact::from_path(name, "1.0.0", "schema.json");
    let payload = Artifact::from_path("payload", "1.0.0", "payload.json")
        .schema_ref(schema("old-schema"))
        .schema_ref(schema("payload-schema"));
    let mut audit = Audit::new("payload-audit", "1.0.0");
    audit
        .add_evidence_ref(payload.clone())
        .add_evidence_ref(schema("payload-schema"));
    let mut declaration = CardDefinition::from(&audit)
        .declaration()
        .expect("valid graph");
    let order: Vec<&str> = declaration
        .artifacts
        .iter()
        .map(|artifact| artifact.metadata.name.as_str())
        .collect();
    assert_eq!(
        order,
        ["payload-schema", "payload"],
        "a nested local schema publishes first, once, and a replaced one not at all"
    );
    let schema_ref = |declaration: &Declaration| {
        serde_json::to_value(&declaration.artifacts[1].spec).expect("serializes")["schema_ref"]
            .clone()
    };
    assert_eq!(
        schema_ref(&declaration),
        json!({"sibling": {"kind": "Artifact", "name": "payload-schema", "version": "1.0.0"}}),
        "the payload names its schema as a sibling until the schema is published"
    );
    declaration
        .bind_published(&published_ref("payload-schema", "1.0.0"))
        .expect("the published schema binds");
    assert_eq!(
        schema_ref(&declaration),
        json!({"kind": "Artifact", "name": "payload-schema", "version": "1.0.0"}),
        "the payload references its published schema by exact identity"
    );
    declaration
        .bind_published(&published_ref("payload", "1.0.0"))
        .expect("the published payload binds");
    assert!(
        declaration
            .input
            .as_ref()
            .expect("an ordinary graph")
            .submissions[0]
            .spec["evidence_refs"]
            .as_array()
            .expect("evidence")
            .iter()
            .all(|evidence| evidence.get("sibling").is_none()),
        "the graph references its published Artifacts by exact identity"
    );
    let unresolved = Audit::new("payload-audit", "1.0.0")
        .evidence_refs([payload.clone().schema_ref("./missing-schema.yaml")]);
    let refused = unresolved
        .validate()
        .expect_err("a local Artifact's path reference is unresolved");
    assert!(
        refuses(&refused, "unresolved path reference"),
        "{refused:?}"
    );
}

impl Agent {
    /// This Agent as a root definition.
    fn card_definition(self) -> CardDefinition {
        CardDefinition::from(self)
    }
}

/// Canonical object forms of sub-values are kept exactly as authored: equal
/// to their constructed values, and a malformed form stands until validation
/// refuses it by its YAML field path.
///
/// # Panics
/// Panics when a canonical form, bare schema, or source string differs from
/// its constructed value, or a malformed form is not refused at its path.
#[test]
fn canonical_objects_match_constructed_values() {
    let step = json!({
        "id": "scan",
        "action": {"kind": "extract", "from": "inputs.report"},
        "depends_on": ["fetch"],
    });
    let constructed = Step::new("scan", Action::extract("inputs.report")).depends_on(["fetch"]);
    assert_eq!(
        FieldValue::from(Step::from(step.clone())).value,
        FieldValue::from(constructed).value
    );
    let check = json!({"kind": "assert", "path": "steps.scan.output.ok", "operator": "equals", "expected": true});
    assert_eq!(
        FieldValue::from(Check::from(check.clone())).value,
        FieldValue::from(Check::assert("steps.scan.output.ok", "equals", true)).value
    );
    assert_eq!(
        FieldValue::from(InputSlot::from(json!("verifiers.a.inputs.report"))).value,
        json!("verifiers.a.inputs.report")
    );
    for slot in [
        json!({"type": "string"}),
        json!({"type": "json", "schema": {"type": "string"}}),
    ] {
        assert_eq!(
            FieldValue::from(InputSlot::from(slot)).value,
            FieldValue::from(InputSlot::json(json!({"type": "string"}))).value
        );
    }
    assert_eq!(
        FieldValue::from(InputSlot::from(json!({"type": "trace"}))).value,
        FieldValue::from(InputSlot::trace()).value
    );
    assert_eq!(
        FieldValue::from(Binding::from(json!({"verifier": "./v.yaml"}))).value,
        FieldValue::from(Binding::new("./v.yaml")).value
    );
    assert_eq!(
        FieldValue::from(Component::from(json!({"alias": "a", "ref": "./a.yaml"}))).value,
        FieldValue::from(Component::new("a", "./a.yaml")).value
    );
    assert_eq!(
        FieldValue::from(Requirement::from(
            json!({"id": "r", "statement": "s", "check": check.clone()})
        ))
        .value,
        FieldValue::from(Requirement::new("r", "s", Check::from(check))).value
    );

    let mut verifier = Verifier::new("canonical", "1.0.0");
    verifier
        .add_input("report", InputSlot::json(json!({"type": "string"})))
        .add_step(Step::from(json!("not-a-step")));
    let error = verifier
        .validate()
        .expect_err("a string step does not decode");
    assert!(refuses(&error, "steps[0]"), "{error:?}");
}

/// Objects and lists built from entries keep what each entry references at
/// its own position: a Card nested in a canonical component object joins the
/// graph, a code file nested in a step object is read at its path, and the
/// value equals the constructed one.
///
/// # Panics
/// Panics when a built value differs from its constructed value or loses a
/// nested reference.
#[test]
fn entry_built_objects_keep_nested_references() {
    let reviewer = reviewer();
    let component = Component::from_iter([
        ("alias".to_owned(), FieldValue::from("reviewer")),
        ("ref".to_owned(), FieldValue::from(reviewer.clone())),
        (
            "verified_by".to_owned(),
            [FieldValue::from_iter([(
                "verifier".to_owned(),
                FieldValue::from(CardRef::verifier("registered", "1.0.0")),
            )])]
            .into_iter()
            .collect(),
        ),
    ]);
    let constructed = Component::new("reviewer", reviewer.clone())
        .verified_by([Binding::new(CardRef::verifier("registered", "1.0.0"))]);
    assert_eq!(component, constructed);
    let service = Service::new("svc", "1.0.0").components([component]);
    let declaration = CardDefinition::from(&service)
        .declaration()
        .expect("the nested Agent joins the graph");
    let input = declaration.input.expect("a graph");
    assert!(
        input
            .submissions
            .iter()
            .any(|card| card.metadata.name.as_str() == "report-reviewer"),
        "{input:?}"
    );

    let step = Step::from_iter([
        ("id".to_owned(), FieldValue::from("scan")),
        (
            "action".to_owned(),
            FieldValue::from_iter([
                ("kind".to_owned(), FieldValue::from("code")),
                (
                    "source".to_owned(),
                    FieldValue::from(CodeSource::file("missing.py")),
                ),
                ("entrypoint".to_owned(), FieldValue::from("main")),
            ]),
        ),
    ]);
    let mut verifier = Verifier::new("built", "1.0.0");
    verifier.add_step(step);
    let error = verifier.validate().expect_err("the nested file is read");
    assert!(
        diagnostics(&error)
            .iter()
            .any(|(code, _)| code == "WYRD_LOADER_400_IO"),
        "{error:?}"
    );

    let workspace = |second: FieldValue| {
        FieldValue::from_iter([
            (
                "/inputs/policy.json".to_owned(),
                FieldValue::from(policy().file("policy.pdf")),
            ),
            ("/inputs/policy.json".to_owned(), second),
        ])
    };
    let repeated = Step::from_iter([
        ("id".to_owned(), FieldValue::from("review")),
        ("id".to_owned(), FieldValue::from("renamed")),
        (
            "action".to_owned(),
            FieldValue::from_iter([
                ("kind".to_owned(), FieldValue::from("agent")),
                ("agent".to_owned(), FieldValue::from(reviewer.clone())),
                (
                    "workspace".to_owned(),
                    workspace(FieldValue::from("inputs.report")),
                ),
            ]),
        ),
    ]);
    let repeated = Verifier::new("repeated", "1.0.0")
        .inputs([("report", InputSlot::json(json!({"type": "object"})))])
        .steps([repeated]);
    let step = &repeated.spec()["steps"][0];
    assert_eq!(step["id"], "review", "the first received field is kept");
    assert_eq!(
        step["action"]["workspace"]["/inputs/policy.json"]["artifact"]["path"], "policy.pdf",
        "the first received destination keeps its fixed source"
    );
    let refused = repeated
        .validate()
        .expect_err("repeated received keys are refused");
    for path in [
        "steps[0].id",
        "steps[0].action.workspace./inputs/policy.json",
    ] {
        assert!(
            refuses(&refused, &format!("{path}: duplicate entry")),
            "{path}: {refused:?}"
        );
    }

    let distinct = Verifier::new("distinct", "1.0.0")
        .inputs([("report", InputSlot::json(json!({"type": "object"})))])
        .steps([Step::from_iter([
            ("id".to_owned(), FieldValue::from("review")),
            (
                "action".to_owned(),
                FieldValue::from_iter([
                    ("kind".to_owned(), FieldValue::from("agent")),
                    ("agent".to_owned(), FieldValue::from(reviewer)),
                    (
                        "workspace".to_owned(),
                        FieldValue::from_iter([
                            (
                                "/inputs/policy.json".to_owned(),
                                FieldValue::from(policy().file("policy.pdf")),
                            ),
                            (
                                "/inputs/report.json".to_owned(),
                                FieldValue::from("inputs.report"),
                            ),
                        ]),
                    ),
                ]),
            ),
        ])]);
    let declaration = CardDefinition::from(&distinct)
        .declaration()
        .expect("distinct keys keep every nested capture");
    assert_eq!(
        declaration.artifacts.len(),
        1,
        "the local Artifact publishes"
    );
    assert!(
        declaration
            .input
            .expect("a graph")
            .submissions
            .iter()
            .any(|card| card.metadata.name.as_str() == "report-reviewer"),
        "the nested Agent joins the graph"
    );
}

/// A drift check's `signal.baseline_ref` takes a Card or exact `CardRef`
/// as any reference position does.
///
/// # Panics
/// Panics when a baseline reference loses its intent.
#[test]
fn drift_references_compose() {
    let drift = |baseline: FieldValue| {
        Verifier::new("latency-drift", "1.0.0")
            .inputs([("latency", InputSlot::samples(["latency"]))])
            .requirements([Requirement::new(
                "latency-stable",
                "Latency is in control",
                Check::drift(
                    "inputs.latency",
                    FieldValue::from_iter([
                        ("method".to_owned(), FieldValue::from("Spc")),
                        (
                            "signal".to_owned(),
                            FieldValue::from_iter([
                                ("kind".to_owned(), FieldValue::from("Distribution")),
                                ("baseline_ref".to_owned(), baseline),
                                ("features".to_owned(), FieldValue::from(json!(["latency"]))),
                            ]),
                        ),
                        (
                            "condition".to_owned(),
                            FieldValue::from(json!({"kind": "Statistical"})),
                        ),
                        (
                            "profile".to_owned(),
                            FieldValue::from(json!({"kind": "Spc", "sample_size": 5})),
                        ),
                    ]),
                ),
            )])
    };
    let reference = json!({"kind": "Data", "name": "latency-baseline", "version": "1.0.0"});
    let literal = drift(FieldValue::from(reference.clone()));
    let external = drift(FieldValue::from(CardRef::data("latency-baseline", "1.0.0")));
    assert_eq!(
        external.spec(),
        literal.spec(),
        "an exact CardRef stays external"
    );
    assert_eq!(
        external.spec()["requirements"][0]["check"]["signal"]["baseline_ref"],
        reference
    );
    external.validate().expect("an external baseline validates");
    let baseline = Data::new("latency-baseline", "1.0.0")
        .interface(json!({"kind": "Parquet", "meta": {"compression": "Snappy"}}))
        .schema(json!({"columns": [{"name": "latency", "dtype": "float64"}]}))
        .stats(json!({
            "row_count": 1,
            "col_count": 1,
            "byte_count": 1,
            "sha256": "c4f7700b380aff2662e88d034154d1890c33d3c12c8dda1a0ca9481f43bb9906",
        }));
    let sibling = drift(FieldValue::from(baseline));
    assert_eq!(
        sibling.spec()["requirements"][0]["check"]["signal"]["baseline_ref"],
        json!({ "sibling": reference }),
        "an authored Data Card is a sibling"
    );
    let input = CardDefinition::from(&sibling)
        .declaration()
        .map(|declaration| declaration.input.expect("a graph"));
    assert!(
        input
            .as_ref()
            .is_ok_and(|input| input
                .submissions
                .iter()
                .any(|card| card.metadata.name.as_str() == "latency-baseline")),
        "the authored baseline joins the graph: {input:?}"
    );
}

/// Definitions without versions declare as YAML without `metadata.version`
/// does: no envelope pin and versionless siblings, which the server resolves.
/// The original Prompt and canonical typed values fill their fields inline.
///
/// # Panics
/// Panics when a version is invented, a typed value is not canonical, or the
/// unversioned graph is refused.
#[test]
fn unversioned_definitions_take_the_original_prompt_and_typed_values() {
    let prompt = skald_prompt::openai_chat(
        "gpt-4o",
        skald_prompt::OpenAiChatOptions {
            messages: vec!["Write the customer report for {{customer}}.".into()],
            ..Default::default()
        },
    )
    .expect("the Prompt builds");
    let writer = Agent::new("report-writer", None)
        .prompt(prompt.clone())
        .execution_route(wyrd_spec::ExecutionRoute::Native {})
        .run_config(wyrd_spec::AgentRunConfigSpec {
            max_iterations: Some(3),
            ..Default::default()
        });
    assert_eq!(writer.card_ref(), None, "no version is invented");
    let checks = Verifier::new("report-checks", None)
        .inputs([("report", InputSlot::json(json!({"type": "string"})))])
        .requirements([Requirement::new(
            "no-bank-account",
            "The report names no bank account",
            Check::assert(
                "inputs.report",
                wyrd_spec::verification::ComparisonOperator::NotContains,
                "BANK-ACCOUNT",
            ),
        )]);
    let service =
        Service::new("finance-release", None)
            .components([Component::new("report-writer", writer).verified_by([checks])]);

    let declaration = CardDefinition::from(&service)
        .declaration()
        .expect("an unversioned graph is valid");
    let input = declaration.input.expect("an ordinary graph");
    assert!(
        input
            .submissions
            .iter()
            .all(|submission| submission.metadata.resolved_pin().is_none()),
        "no submission carries a version"
    );
    let service = input
        .submissions
        .iter()
        .find(|submission| submission.kind == CardKind::Service)
        .expect("the Service is submitted");
    let component = &service.spec["components"][0];
    assert_eq!(
        component["ref"],
        json!({"sibling": {"kind": "Agent", "name": "report-writer"}})
    );
    assert_eq!(
        component["verified_by"][0]["verifier"],
        json!({"sibling": {"kind": "Verifier", "name": "report-checks"}})
    );
    let agent = input
        .submissions
        .iter()
        .find(|submission| submission.kind == CardKind::Agent)
        .expect("the Agent is submitted");
    let agent: AgentSpec = decoded(agent.spec.clone());
    assert_eq!(
        agent.prompt,
        wyrd_spec::reference::InlineableRef::Inline(Box::new(prompt.into_native())),
        "the original Prompt is held inline"
    );
    assert_eq!(agent.execution_route, wyrd_spec::ExecutionRoute::Native {});
    assert_eq!(agent.run_config.max_iterations, Some(3));
    let verifier = input
        .submissions
        .iter()
        .find(|submission| submission.kind == CardKind::Verifier)
        .expect("the Verifier is submitted");
    assert_eq!(
        verifier.spec["requirements"][0]["check"]["operator"],
        json!("not_contains")
    );
}
