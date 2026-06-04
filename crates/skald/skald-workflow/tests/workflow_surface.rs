use skald_agent::Agent;
use skald_prompt::{OpenAiChatOptions, openai_chat};
use skald_workflow::Workflow;
use wyrd_spec::card::workflow::WorkflowAction;
use wyrd_spec::reference::AgentRef;

fn build_agent(name: &str) -> Agent {
    let prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec!["plan a thing".to_owned()],
            ..Default::default()
        },
    )
    .expect("static prompt is valid");
    Agent::new(prompt).name(name).version("0.1.0")
}

fn anonymous_agent() -> Agent {
    let prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec!["plan".to_owned()],
            ..Default::default()
        },
    )
    .expect("static prompt is valid");
    Agent::new(prompt)
}

#[test]
fn workflow_sequential_chains_named_agents_with_card_ref_cascade() {
    let wf = Workflow::sequential("research", [build_agent("planner"), build_agent("writer")])
        .expect("valid sequential workflow");

    assert_eq!(wf.step_ids(), vec!["planner", "writer"]);
    assert_eq!(wf.spec().steps[0].depends_on, Vec::<String>::new());
    assert_eq!(wf.spec().steps[1].depends_on, vec!["planner".to_owned()]);

    let names: Vec<&str> = wf
        .cascade_children()
        .iter()
        .map(|card_ref| card_ref.name.as_str())
        .collect();
    assert!(names.contains(&"planner"));
    assert!(names.contains(&"writer"));
    for step in &wf.spec().steps {
        match &step.action {
            WorkflowAction::Agent(AgentRef::Card(_)) => {}
            other => panic!("expected Agent::Card variant, got {other:?}"),
        }
    }
}

#[test]
fn workflow_sequential_anonymous_agents_get_inline_action() {
    let wf = Workflow::sequential("research", [anonymous_agent(), anonymous_agent()])
        .expect("valid sequential workflow");
    assert!(wf.cascade_children().is_empty());
    for step in &wf.spec().steps {
        match &step.action {
            WorkflowAction::Agent(AgentRef::Inline(_)) => {}
            other => panic!("expected Agent::Inline variant, got {other:?}"),
        }
    }
}

#[test]
fn workflow_parallel_zero_deps() {
    let wf = Workflow::parallel(
        "fanout",
        [
            build_agent("alpha"),
            build_agent("bravo"),
            build_agent("charlie"),
        ],
    )
    .expect("valid parallel workflow");
    for step in &wf.spec().steps {
        assert!(step.depends_on.is_empty());
    }
}

#[test]
fn workflow_builder_add_after_string_dep() {
    let wf = Workflow::builder("research")
        .add(build_agent("planner"))
        .expect("add planner")
        .add_after(build_agent("writer"), ["planner".to_owned()])
        .expect("add writer after planner")
        .build()
        .expect("valid DAG");
    assert_eq!(wf.spec().steps[1].depends_on, vec!["planner".to_owned()]);
}

#[test]
fn workflow_to_yaml_round_trip_preserves_steps() {
    let wf = Workflow::sequential("research", [build_agent("planner"), build_agent("writer")])
        .expect("valid sequential workflow")
        .with_version("0.1.0");
    let yaml = wf.to_yaml_string().expect("yaml");
    assert!(yaml.contains("research"));
    assert!(yaml.contains("planner"));
    assert!(yaml.contains("writer"));
}
