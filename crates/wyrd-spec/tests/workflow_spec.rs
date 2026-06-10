use std::collections::BTreeMap;

use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec};
use wyrd_spec::card::workflow::{
    WorkflowAction, WorkflowCard, WorkflowSpec, WorkflowStep, WorkflowValidationError,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{AgentRef, CardRef, PromptRef};

fn prompt() -> skald_spec::Prompt {
    skald_spec::Prompt::new(
        skald_spec::ProviderRequest::OpenAiChatCompletion(skald_spec::OpenAiChatRequest {
            model: "gpt-4o-mini".to_owned(),
            messages: vec![skald_spec::OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                    "plan".to_owned(),
                )),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                refusal: None,
                annotations: Vec::new(),
                audio: None,
            }],
            response_format: None,
            stream: None,
            stream_options: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            settings: skald_spec::OpenAiChatSettings::default(),
        }),
        "gpt-4o-mini",
        None,
        skald_spec::ResponseType::Text,
    )
    .expect("static prompt is valid")
}

fn inline_agent_step(id: &str) -> WorkflowStep {
    WorkflowStep {
        id: id.to_owned(),
        action: WorkflowAction::Agent(AgentRef::from(AgentSpec {
            prompt: PromptRef::from(prompt()),
            tool_names: vec![],
            run_config: AgentRunConfigSpec::default(),
        })),
        depends_on: vec![],
        inputs: BTreeMap::new(),
        condition: None,
        timeout_seconds: None,
        retry: None,
        display: BTreeMap::new(),
    }
}

fn card_ref_agent_step(id: &str, agent_name: &str) -> WorkflowStep {
    WorkflowStep {
        id: id.to_owned(),
        action: WorkflowAction::Agent(AgentRef::from(CardRef {
            kind: CardKind::Agent,
            name: agent_name.parse().expect("valid card name"),
            version: "0.1.0".parse().expect("valid version"),
            space: None,
            uid: None,
        })),
        depends_on: vec![],
        inputs: BTreeMap::new(),
        condition: None,
        timeout_seconds: None,
        retry: None,
        display: BTreeMap::new(),
    }
}

#[test]
fn workflow_spec_inline_agent_roundtrips() {
    let spec = WorkflowSpec {
        steps: vec![inline_agent_step("planner")],
        ..WorkflowSpec::default()
    };

    let yaml = serde_yaml::to_string(&spec).expect("serialize");
    let decoded: WorkflowSpec = serde_yaml::from_str(&yaml).expect("deserialize");
    assert_eq!(decoded, spec);
}

#[test]
fn workflow_spec_card_ref_agent_roundtrips() {
    let spec = WorkflowSpec {
        steps: vec![card_ref_agent_step("planner", "research-planner")],
        ..WorkflowSpec::default()
    };

    let yaml = serde_yaml::to_string(&spec).expect("serialize");
    let decoded: WorkflowSpec = serde_yaml::from_str(&yaml).expect("deserialize");
    assert_eq!(decoded, spec);
    assert!(
        yaml.contains("research-planner"),
        "expected card ref name in YAML, got: {yaml}"
    );
}

#[test]
fn workflow_card_envelope_roundtrips() {
    let card = WorkflowCard {
        space: "default".to_owned(),
        name: "research".to_owned(),
        version: "0.1.0".to_owned(),
        uid: String::new(),
        labels: Labels::default(),
        annotations: Annotations::default(),
        spec: WorkflowSpec {
            steps: vec![card_ref_agent_step("planner", "research-planner")],
            ..WorkflowSpec::default()
        },
        cascade_children: vec![],
        created_at: chrono::Utc::now(),
    };

    let envelope = card.to_envelope().expect("to_envelope");
    let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
    assert_eq!(reloaded.spec, card.spec);
    assert_eq!(reloaded.name, card.name);
    assert_eq!(reloaded.version, card.version);
    assert_eq!(reloaded.cascade_children.len(), 1);
    assert_eq!(reloaded.cascade_children[0].kind, CardKind::Agent);
}

#[test]
fn workflow_card_cascade_inline_agent_with_card_prompt() {
    let inline_with_prompt_ref = AgentSpec {
        prompt: PromptRef::from(CardRef {
            kind: CardKind::Prompt,
            name: "planner-prompt".parse().expect("valid card name"),
            version: "0.3.0".parse().expect("valid version"),
            space: None,
            uid: None,
        }),
        tool_names: vec![],
        run_config: AgentRunConfigSpec::default(),
    };
    let step = WorkflowStep {
        id: "planner".to_owned(),
        action: WorkflowAction::Agent(AgentRef::from(inline_with_prompt_ref)),
        depends_on: vec![],
        inputs: BTreeMap::new(),
        condition: None,
        timeout_seconds: None,
        retry: None,
        display: BTreeMap::new(),
    };
    let card = WorkflowCard {
        space: "default".to_owned(),
        name: "research".to_owned(),
        version: "0.1.0".to_owned(),
        uid: String::new(),
        labels: Labels::default(),
        annotations: Annotations::default(),
        spec: WorkflowSpec {
            steps: vec![step],
            ..WorkflowSpec::default()
        },
        cascade_children: vec![],
        created_at: chrono::Utc::now(),
    };

    let envelope = card.to_envelope().expect("to_envelope");
    let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
    assert_eq!(reloaded.cascade_children.len(), 1);
    assert_eq!(reloaded.cascade_children[0].kind, CardKind::Prompt);
    assert_eq!(reloaded.cascade_children[0].name.as_str(), "planner-prompt");
}

#[test]
fn workflow_card_cascade_card_ref_agent() {
    let card = WorkflowCard {
        space: "default".to_owned(),
        name: "research".to_owned(),
        version: "0.1.0".to_owned(),
        uid: String::new(),
        labels: Labels::default(),
        annotations: Annotations::default(),
        spec: WorkflowSpec {
            steps: vec![card_ref_agent_step("planner", "research-planner")],
            ..WorkflowSpec::default()
        },
        cascade_children: vec![],
        created_at: chrono::Utc::now(),
    };

    let envelope = card.to_envelope().expect("to_envelope");
    let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
    assert_eq!(reloaded.cascade_children.len(), 1);
    assert_eq!(reloaded.cascade_children[0].kind, CardKind::Agent);
    assert_eq!(
        reloaded.cascade_children[0].name.as_str(),
        "research-planner"
    );
}

#[test]
fn workflow_card_cascade_dedups_repeated_refs() {
    let spec = WorkflowSpec {
        steps: vec![
            card_ref_agent_step("planner-a", "shared-planner"),
            card_ref_agent_step("planner-b", "shared-planner"),
        ],
        ..WorkflowSpec::default()
    };
    let card = WorkflowCard {
        space: "default".to_owned(),
        name: "research".to_owned(),
        version: "0.1.0".to_owned(),
        uid: String::new(),
        labels: Labels::default(),
        annotations: Annotations::default(),
        spec,
        cascade_children: vec![],
        created_at: chrono::Utc::now(),
    };

    let envelope = card.to_envelope().expect("to_envelope");
    let reloaded = WorkflowCard::from_envelope(envelope).expect("from_envelope");
    assert_eq!(
        reloaded.cascade_children.len(),
        1,
        "expected dedup, got: {:?}",
        reloaded.cascade_children
    );
}

#[test]
fn workflow_validate_dag_duplicate_step() {
    let spec = WorkflowSpec {
        steps: vec![inline_agent_step("planner"), inline_agent_step("planner")],
        ..WorkflowSpec::default()
    };
    assert_eq!(
        spec.validate_dag().unwrap_err(),
        WorkflowValidationError::DuplicateStep
    );

    let wyrd: WyrdError = spec.validate_dag().unwrap_err().into();
    assert_eq!(wyrd.code(), "WYRD_WORKFLOW_422_DUPLICATE_STEP_ID");
}

#[test]
fn workflow_validate_dag_missing_dependency() {
    let mut planner = inline_agent_step("planner");
    planner.depends_on = vec!["never-defined".to_owned()];
    let spec = WorkflowSpec {
        steps: vec![planner],
        ..WorkflowSpec::default()
    };
    assert_eq!(
        spec.validate_dag().unwrap_err(),
        WorkflowValidationError::MissingDependency
    );

    let wyrd: WyrdError = spec.validate_dag().unwrap_err().into();
    assert_eq!(wyrd.code(), "WYRD_WORKFLOW_422_MISSING_DEPENDENCY");
}

#[test]
fn workflow_validate_dag_cycle() {
    let mut a = inline_agent_step("a");
    let mut b = inline_agent_step("b");
    a.depends_on = vec!["b".to_owned()];
    b.depends_on = vec!["a".to_owned()];
    let spec = WorkflowSpec {
        steps: vec![a, b],
        ..WorkflowSpec::default()
    };
    assert_eq!(
        spec.validate_dag().unwrap_err(),
        WorkflowValidationError::Cycle
    );

    let wyrd: WyrdError = spec.validate_dag().unwrap_err().into();
    assert_eq!(wyrd.code(), "WYRD_WORKFLOW_422_CYCLE");
}
