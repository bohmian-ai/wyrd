//! Per-record named media bindings for LLM judges.
//!
//! A binding is the committed Eval [`MediaRef`] descriptor — never bytes and
//! never provider-facing text. The judge invoker resolves each descriptor to
//! authorized bytes at call time and binds them into the judge Prompt's
//! matching `${media:id}` placeholder as provider-native content.

use std::collections::BTreeMap;

use wyrd_spec::vala::eval::media::MediaRef;

/// Engine-owned collection of one record's media descriptors, keyed by binding id.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MediaBindings {
    /// Descriptors keyed by their `${media:id}` binding id.
    by_id: BTreeMap<String, MediaRef>,
}

impl MediaBindings {
    /// Empty binding set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind every descriptor of one record; a repeated id keeps the last one.
    #[must_use]
    pub fn from_refs(refs: impl IntoIterator<Item = MediaRef>) -> Self {
        let mut bindings = Self::new();
        for media in refs {
            bindings.insert(media);
        }
        bindings
    }

    /// Borrow a binding by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MediaRef> {
        self.by_id.get(id)
    }

    /// Insert or replace a binding by its id.
    pub fn insert(&mut self, media: MediaRef) {
        self.by_id.insert(media.id.as_str().to_owned(), media);
    }

    /// Iterate binding ids in stable order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.by_id.keys().map(String::as_str)
    }

    /// Iterate descriptors in stable binding-id order.
    pub fn iter(&self) -> impl Iterator<Item = &MediaRef> {
        self.by_id.values()
    }

    /// True when no bindings are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
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
        AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, MediaBindings,
        TraceTaskExecutor,
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
            space: Some(SpaceName::new("default").expect("valid space")),
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

    /// Drive `spec` over `base` with bound `media` and the scripted judge `mock`.
    ///
    /// # Errors
    /// Returns whatever execution error the driver propagates.
    async fn drive(
        spec: EvalSpec,
        base: Value,
        media: MediaBindings,
        required_media: Vec<String>,
        mock: Arc<MockJudgeInvoker>,
    ) -> Result<EvalReport, crate::EvalExecError> {
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        let cx = ExecutionContext::new(
            base,
            RunId::from_string("run-media".to_owned()),
            RecordId(Uuid::from_u128(11)),
            None,
        )
        .with_media(media, required_media);
        execute_plan(&plan, &cx, &registry, &executors(mock)).await
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

    /// Named media reaches the invoker as a typed binding, never as context text.
    #[tokio::test]
    async fn bound_media_reaches_invoker_as_binding_not_context_text() {
        let descriptor = wyrd_spec::vala::eval::media::MediaRef {
            id: wyrd_spec::ids::MediaBindingId::new("image_under_review")
                .expect("static binding id is valid"),
            kind: wyrd_spec::vala::eval::media::MediaKind::Image,
            uri: "file:///tmp/example.png".to_owned(),
            media_type: Some("image/png".to_owned()),
        };
        let media = MediaBindings::from_refs([descriptor.clone()]);
        let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
        let report = drive(
            spec_of(vec![judge_task()]),
            json!({"response": "look at this"}),
            media,
            Vec::new(),
            Arc::clone(&mock),
        )
        .await
        .expect("plan executes");
        assert!(result(&report, "judge").passed);
        let calls = mock.calls().await;
        assert_eq!(calls.len(), 1);
        assert!(
            !calls[0].1.to_string().contains("file:///tmp/example.png"),
            "the private URI must not become judge context text"
        );
        let media_calls = mock.media_calls().await;
        assert_eq!(media_calls[0].get("image_under_review"), Some(&descriptor));
    }

    /// Unbound required media is an execution error and never invokes the judge.
    #[tokio::test]
    async fn required_media_id_unbound_errors_without_invoking_judge() {
        let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
        let error = drive(
            spec_of(vec![judge_task()]),
            json!({"response": "look at this"}),
            MediaBindings::new(),
            vec!["image_under_review".to_owned()],
            Arc::clone(&mock),
        )
        .await
        .expect_err("unbound required media must not produce a result");
        assert!(error.to_string().contains("required media id"), "{error}");
        assert_eq!(mock.calls().await.len(), 0);
    }
}
