use skald_agent::Agent;
use skald_prompt::{OpenAiChatOptions, openai_chat};
use skald_workflow::{AgentResolver, Workflow};
use wyrd_spec::AgentSpec;
use wyrd_spec::card::workflow::{WorkflowAction, WorkflowCard, WorkflowSpec, WorkflowStep};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, InlineableRef};

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

struct StaticAgentResolver {
    agent: Agent,
}

impl AgentResolver for StaticAgentResolver {
    fn resolve(
        &self,
        _agent_ref: &InlineableRef<AgentSpec>,
    ) -> Result<Agent, wyrd_spec::error::WyrdError> {
        Ok(self.agent.clone())
    }
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
            WorkflowAction::Agent(InlineableRef::Ref(_)) => {}
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
            WorkflowAction::Agent(InlineableRef::Inline(_)) => {}
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

#[test]
fn workflow_hydrates_sibling_agent_with_explicit_resolver() {
    let agent_ref = CardRef {
        kind: CardKind::Agent,
        name: "registered-agent".parse().expect("agent name is valid"),
        version: "0.1.0".parse().expect("agent version is valid"),
        space: Some("research".parse().expect("agent space is valid")),
        uid: None,
    };
    let card = WorkflowCard {
        space: "research".to_owned(),
        name: "registered-workflow".to_owned(),
        version: "0.1.0".to_owned(),
        uid: String::new(),
        labels: Labels::default(),
        annotations: Annotations::default(),
        spec: WorkflowSpec {
            steps: vec![WorkflowStep {
                id: "registered-agent".to_owned(),
                action: WorkflowAction::Agent(InlineableRef::Sibling {
                    sibling: agent_ref.clone(),
                }),
                inputs: Default::default(),
                depends_on: Vec::new(),
                condition: None,
                timeout_seconds: None,
                retry: None,
                display: Default::default(),
            }],
            ..Default::default()
        },
        cascade_children: vec![agent_ref],
        created_at: chrono::Utc::now(),
    };
    let tool_resolver = skald_tool::ToolRegistry::default();
    let resolver = StaticAgentResolver {
        agent: build_agent("registered-agent"),
    };

    let workflow = Workflow::from_card_with_agent_resolver(
        card,
        &tool_resolver,
        skald_agent::default_prompt_resolver(),
        Some(&resolver),
    )
    .expect("sibling agent resolves");
    assert_eq!(workflow.step_ids(), vec!["registered-agent"]);
}
