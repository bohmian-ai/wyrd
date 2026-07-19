//! Per-record media bindings for LLM judge context.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One opaque media payload bound to a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalMediaBinding {
    /// Media identifier referenced by the judge prompt.
    pub id: String,
    /// Opaque payload for the eventual provider-specific renderer.
    pub payload: Value,
}

/// Engine-owned collection of per-record media bindings.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MediaBindings {
    by_id: BTreeMap<String, EvalMediaBinding>,
}

impl MediaBindings {
    /// Empty binding set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow a binding by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&EvalMediaBinding> {
        self.by_id.get(id)
    }

    /// Insert or replace a binding by its id.
    pub fn insert(&mut self, binding: EvalMediaBinding) {
        self.by_id.insert(binding.id.clone(), binding);
    }

    /// Iterate binding ids in stable order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.by_id.keys().map(String::as_str)
    }

    /// True when no bindings are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Project media bindings into the context shape seen by a judge.
#[must_use]
pub fn bindings_as_context(bindings: &MediaBindings) -> Value {
    let mut root = Map::new();
    for id in bindings.ids() {
        if let Some(binding) = bindings.get(id) {
            root.insert(
                id.to_owned(),
                serde_json::json!({ "payload": binding.payload }),
            );
        }
    }
    Value::Object(root)
}

#[cfg(test)]
mod media_binding {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::context::ExecutionContext;
    use crate::executor::{EvalReport, Executors, TaskRunOutcome, execute_plan};
    use crate::store::TaskRegistry;
    use crate::tasks::{
        AgentTaskExecutor, AssertionTaskExecutor, EvalMediaBinding, JudgeTaskExecutor,
        MediaBindings, TraceTaskExecutor,
    };
    use crate::{InMemoryTraceSource, MockJudgeInvoker};
    use serde_json::{Value, json};
    use uuid::Uuid;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::{
        ComparisonOperator, EvalSpec, EvalTask, LlmJudgeTask, RecordId, RunId, TaskId,
    };

    fn tid(value: &str) -> TaskId {
        TaskId::new(value).expect("static task id is valid")
    }

    fn judge_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Agent,
            name: CardName::new("media-judge").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: SpaceName::new("default").expect("valid space"),
            uid: None,
        }
    }

    fn judge_task() -> EvalTask {
        EvalTask::LlmJudge(LlmJudgeTask {
            id: tid("judge"),
            judge_ref: judge_card_ref().into(),
            context_path: None,
            expected: json!("pass"),
            operator: ComparisonOperator::Equals,
            depends_on: Vec::new(),
            max_retries: 0,
            condition: None,
        })
    }

    fn spec_of(tasks: Vec<EvalTask>) -> EvalSpec {
        let mut map = BTreeMap::new();
        for task in tasks {
            map.insert(task.id().clone(), task);
        }
        EvalSpec {
            subject_ref: None,
            dataset: None,
            tasks: map,
            workflow: None,
            sampling: None,
            pass_gate: None,
            context_capture: None,
        }
    }

    fn executors(mock: Arc<MockJudgeInvoker>) -> Executors {
        Executors {
            assertion: Arc::new(AssertionTaskExecutor::new()),
            judge: Arc::new(JudgeTaskExecutor::new(mock)),
            trace: Arc::new(TraceTaskExecutor::new(
                Arc::new(InMemoryTraceSource::new()),
                Duration::from_millis(250),
            )),
            agent: Arc::new(AgentTaskExecutor::new()),
        }
    }

    async fn drive(
        spec: EvalSpec,
        base: Value,
        media: MediaBindings,
        required_media: Vec<String>,
        mock: Arc<MockJudgeInvoker>,
    ) -> EvalReport {
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        let cx = ExecutionContext::new(
            base,
            RunId::from_string("run-media".to_owned()),
            RecordId(Uuid::from_u128(11)),
            None,
        )
        .with_media(media, required_media);
        execute_plan(&plan, &cx, &registry, &executors(mock))
            .await
            .expect("plan executes")
    }

    fn result<'a>(report: &'a EvalReport, id: &str) -> &'a wyrd_spec::vala::eval::AssertionResult {
        report
            .outcomes
            .iter()
            .find_map(|outcome| match outcome {
                TaskRunOutcome::Ran(result) if result.task_id == tid(id) => Some(result),
                _ => None,
            })
            .expect("task result exists")
    }

    #[tokio::test]
    async fn bound_media_appears_in_invoker_context_under_media_key() {
        let mut media = MediaBindings::new();
        media.insert(EvalMediaBinding {
            id: "image_under_review".to_owned(),
            payload: json!({
                "kind": "image",
                "url": "file:///tmp/example.png",
            }),
        });
        let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
        let report = drive(
            spec_of(vec![judge_task()]),
            json!({"response": "look at this"}),
            media,
            Vec::new(),
            Arc::clone(&mock),
        )
        .await;
        assert!(result(&report, "judge").passed);
        let calls = mock.calls().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1["media"]["image_under_review"]["payload"]["url"],
            json!("file:///tmp/example.png")
        );
    }

    #[tokio::test]
    async fn required_media_id_unbound_errors_without_invoking_judge() {
        let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
        let report = drive(
            spec_of(vec![judge_task()]),
            json!({"response": "look at this"}),
            MediaBindings::new(),
            vec!["image_under_review".to_owned()],
            Arc::clone(&mock),
        )
        .await;
        let result = result(&report, "judge");
        assert!(!result.passed);
        assert!(
            result
                .message
                .as_deref()
                .unwrap_or("")
                .contains("required media id")
        );
        assert_eq!(mock.calls().await.len(), 0);
    }
}
