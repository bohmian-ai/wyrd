//! Minimal spec constructors and drift mutators for each CardKind.

use wyrd_semver::VersionBlock;
use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec};
use wyrd_spec::card::artifact::{ArtifactSpec, FrameworkAdapterRef};
use wyrd_spec::card::audit::AuditSpec;
use wyrd_spec::card::data::{
    DataInterface, DataSchema, DataSpec, DataStats, PandasMeta, ParquetCompression,
};
use wyrd_spec::card::drift::DriftSpec;
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::card::experiment::ExperimentSpec;
use wyrd_spec::card::field::FieldSpec;
use wyrd_spec::card::mcp::McpSpec;
use wyrd_spec::card::model::{ModelInterface, ModelSignature, ModelSpec, SklearnMeta, TaskType};
use wyrd_spec::card::operator::OperatorSpec;
use wyrd_spec::card::policy::PolicySpec;
use wyrd_spec::card::service::ServiceSpec;
use wyrd_spec::card::source::{ObjectFormat, SourceAuth, SourceKind, SourceSpec};
use wyrd_spec::card::trigger::{TriggerSource, TriggerSpec};
use wyrd_spec::card::workflow::WorkflowSpec;
use wyrd_spec::envelope::{Card, CardKind, Spec};
use wyrd_spec::ids::{CardName, ColumnName, SpaceName};
use wyrd_spec::reference::{CardRef, PromptRef};

fn col(name: &str) -> ColumnName {
    ColumnName::new(name).expect("fixture column name")
}

fn card_ref(kind: CardKind, name: &str) -> CardRef {
    CardRef {
        kind,
        name: CardName::new(name).expect("fixture card ref name"),
        version: VersionBlock::parse("1.0.0").expect("fixture version"),
        space: SpaceName::new("prod").expect("fixture space"),
        uid: None,
    }
}

fn minimal_data_spec() -> DataSpec {
    DataSpec {
        interface: DataInterface::Pandas(PandasMeta {
            framework_version: "2.2.2".to_string(),
            compression: ParquetCompression::Snappy,
        }),
        schema: DataSchema::new(vec![
            FieldSpec::new(col("x"), "int64"),
            FieldSpec::new(col("y"), "float32"),
        ]),
        card_refs: vec![],
        splits: Default::default(),
        target_columns: vec![],
        sql: None,
        stats: DataStats {
            row_count: Some(1),
            col_count: Some(2),
            byte_count: 42,
            sha256: "a".repeat(64),
        },
    }
}

fn minimal_model_spec() -> ModelSpec {
    ModelSpec {
        interface: ModelInterface::Sklearn(SklearnMeta {
            framework_version: "1.4.0".to_string(),
            model_subtype: None,
        }),
        task_type: TaskType::Regression,
        signature: ModelSignature::new(
            vec![FieldSpec::new(col("input"), "float32")],
            vec![FieldSpec::new(col("output"), "float32")],
        ),
        sample_input: None,
        card_refs: vec![],
    }
}

fn minimal_agent_spec() -> AgentSpec {
    AgentSpec {
        prompt: PromptRef::Card(card_ref(CardKind::Prompt, "base-prompt")),
        tool_names: vec![],
        run_config: AgentRunConfigSpec::default(),
    }
}

fn minimal_prompt_spec() -> Spec {
    Spec::from_kind_and_value(
        &CardKind::Prompt,
        serde_json::json!({
            "provider": "openai",
            "model": "gpt-4o-mini",
            "messages": "You are a helpful assistant."
        }),
    )
    .expect("minimal prompt spec is valid")
}

fn minimal_operator_spec() -> OperatorSpec {
    OperatorSpec {
        adapter: FrameworkAdapterRef {
            name: "python-operator".to_string(),
            version: "1.0".to_string(),
            config: Default::default(),
        },
        inputs: vec![],
        pre_invoke: vec![],
        post_invoke: vec![],
        budget: None,
    }
}

fn minimal_trigger_spec() -> TriggerSpec {
    TriggerSpec {
        source: TriggerSource::Schedule {
            cron: "0 * * * *".to_string(),
        },
        target: card_ref(CardKind::Operator, "target-operator"),
        cooldown_seconds: None,
        config: Default::default(),
    }
}

fn minimal_source_spec() -> SourceSpec {
    SourceSpec {
        description: None,
        source: SourceKind::ObjectStore {
            uri: "s3://test-bucket/data".to_string(),
            format: ObjectFormat::Parquet,
            auth: SourceAuth::default(),
        },
        defaults: Default::default(),
    }
}

pub fn minimal_spec(kind: CardKind) -> Spec {
    match kind {
        CardKind::Data => Spec::Data(minimal_data_spec()),
        CardKind::Model => Spec::Model(minimal_model_spec()),
        CardKind::Experiment => Spec::Experiment(ExperimentSpec::default()),
        CardKind::Prompt => minimal_prompt_spec(),
        CardKind::Agent => Spec::Agent(minimal_agent_spec()),
        CardKind::Workflow => Spec::Workflow(WorkflowSpec::default()),
        CardKind::Eval => Spec::Eval(EvalSpec::default()),
        CardKind::Drift => Spec::Drift(DriftSpec::default()),
        CardKind::Service => Spec::Service(ServiceSpec::default()),
        CardKind::Policy => Spec::Policy(PolicySpec::default()),
        CardKind::Mcp => Spec::Mcp(McpSpec::default()),
        CardKind::Audit => Spec::Audit(AuditSpec::default()),
        CardKind::Artifact => Spec::Artifact(ArtifactSpec::default()),
        CardKind::Trigger => Spec::Trigger(minimal_trigger_spec()),
        CardKind::Operator => Spec::Operator(minimal_operator_spec()),
        CardKind::Source => Spec::Source(minimal_source_spec()),
        CardKind::External => panic!("External is not a registerable kind in e2e tests"),
    }
}

pub fn mutate_for_drift(card: &mut Card) {
    match &mut card.spec {
        Spec::Data(s) => s.stats.byte_count += 1,
        Spec::Model(s) => s
            .signature
            .inputs
            .push(FieldSpec::new(col("drift"), "int64")),
        Spec::Experiment(s) => s.description = Some("drift".to_string()),
        Spec::Prompt(_) => {
            card.spec = Spec::from_kind_and_value(
                &CardKind::Prompt,
                serde_json::json!({
                    "provider": "openai",
                    "model": "gpt-4o-mini",
                    "messages": "You are a drifted assistant."
                }),
            )
            .expect("drifted prompt spec is valid");
        }
        Spec::Agent(s) => s.run_config.max_iterations = Some(99),
        Spec::Workflow(s) => s.description = Some("drift".to_string()),
        Spec::Eval(s) => s.description = Some("drift".to_string()),
        Spec::Drift(s) => s.description = Some("drift".to_string()),
        Spec::Service(s) => s.description = Some("drift".to_string()),
        Spec::Policy(s) => s.description = Some("drift".to_string()),
        Spec::Mcp(s) => s.description = Some("drift".to_string()),
        Spec::Audit(s) => s.description = Some("drift".to_string()),
        Spec::Artifact(s) => s.artifact_kind = "drifted".to_string(),
        Spec::Trigger(s) => s.cooldown_seconds = Some(60),
        Spec::Operator(s) => s.adapter.version = "2.0".to_string(),
        Spec::Source(s) => s.description = Some("drift".to_string()),
    }
}
