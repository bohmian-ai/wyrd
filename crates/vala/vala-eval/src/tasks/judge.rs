//! `LlmJudgeTask` executor.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use wyrd_spec::reference::InlineableRef;

use wyrd_spec::vala::eval::{AssertionResult, EvalTask, LlmJudgeTask};

use crate::context::{ContextSnapshot, TaskOutput, extract_required_jsonpath_from};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;
use crate::judge::{JudgeError, JudgeInvoker};
use crate::operators;
use crate::store::JudgeOutcome;
use crate::tasks::media::{MediaBindings, bindings_as_context};

/// Executor for Agent-backed LLM judge tasks.
pub struct JudgeTaskExecutor {
    invoker: Arc<dyn JudgeInvoker>,
}

impl JudgeTaskExecutor {
    /// Fresh judge executor with an injected invoker.
    #[must_use]
    pub fn new(invoker: Arc<dyn JudgeInvoker>) -> Self {
        Self { invoker }
    }
}

#[async_trait]
impl TaskExecutor for JudgeTaskExecutor {
    async fn execute(
        &self,
        task: &EvalTask,
        snapshot: &ContextSnapshot,
        stage: u32,
    ) -> Result<TaskOutput, EvalExecError> {
        let EvalTask::LlmJudge(judge) = task else {
            return Err(EvalExecError::DagInvalid {
                reason: format!(
                    "stage {stage} dispatched {} task {:?} to judge executor",
                    task.discriminator(),
                    task.id().as_str()
                ),
            });
        };

        for media_id in snapshot.required_media.iter() {
            if snapshot.media.get(media_id).is_none() {
                return Err(EvalExecError::MediaBindingMissingForTask {
                    task_id: judge.id.clone(),
                    media_id: media_id.clone(),
                });
            }
        }

        let view = snapshot.build_scoped_view(&judge.depends_on);
        let narrowed = match &judge.context_path {
            Some(path) => extract_required_jsonpath_from(&view, path, &judge.id)?,
            None => view,
        };
        let context_for_invoker = embed_media(narrowed, &snapshot.media);
        let parsed =
            invoke_with_retries(self.invoker.as_ref(), judge, context_for_invoker.clone()).await?;

        let started_at = Utc::now();
        let verdict = operators::evaluate_operator(&parsed, &judge.operator, &judge.expected)
            .map_err(|error| EvalExecError::OperatorTypeMismatch {
                task_id: judge.id.clone(),
                operator: format!("{:?}", judge.operator),
                reason: error.to_string(),
            })?;

        let result = AssertionResult {
            task_id: judge.id.clone(),
            passed: verdict.passed,
            actual: verdict.observed,
            expected: verdict.expected.unwrap_or_else(|| judge.expected.clone()),
            operator: judge.operator.clone(),
            message: None,
            stage,
            started_at,
            duration_ms: elapsed_ms(started_at),
        };
        let outcome = JudgeOutcome {
            raw: context_for_invoker,
            parsed,
            judge_ref: judge.judge_ref.as_card_ref().cloned(),
        };
        Ok(TaskOutput::Judge { result, outcome })
    }
}

fn embed_media(context: Value, bindings: &MediaBindings) -> Value {
    if bindings.is_empty() {
        return context;
    }

    let media_value = bindings_as_context(bindings);
    match context {
        Value::Object(mut map) => {
            map.insert("media".to_owned(), media_value);
            Value::Object(map)
        }
        other => serde_json::json!({
            "context": other,
            "media": media_value,
        }),
    }
}

async fn invoke_with_retries(
    invoker: &dyn JudgeInvoker,
    task: &LlmJudgeTask,
    context: Value,
) -> Result<Value, EvalExecError> {
    if matches!(
        &task.judge_ref,
        InlineableRef::Path(_) | InlineableRef::Sibling { .. }
    ) {
        return Err(EvalExecError::JudgeRetriesExhausted {
                task_id: task.id.clone(),
                attempts: 0,
                last_error: "WYRD_REGISTRY_400_UNRESOLVED_PATH_REF: judge Agent path must be rewritten by the loader before execution".to_owned(),
            });
    }
    let max_attempts = task.max_retries.saturating_add(1);
    let mut last_error = String::from("no attempt made");

    for attempt in 0..max_attempts {
        match invoker.invoke(&task.judge_ref, context.clone()).await {
            Ok(value) => return Ok(value),
            Err(error) if error.is_retryable() && attempt + 1 < max_attempts => {
                last_error = error.to_string();
            }
            Err(JudgeError::InvalidStructuredOutput { reason }) => {
                return Err(EvalExecError::JudgeInvalidOutput {
                    task_id: task.id.clone(),
                    reason,
                });
            }
            Err(error) => {
                return Err(EvalExecError::JudgeRetriesExhausted {
                    task_id: task.id.clone(),
                    attempts: attempt + 1,
                    last_error: error.to_string(),
                });
            }
        }
    }

    Err(EvalExecError::JudgeRetriesExhausted {
        task_id: task.id.clone(),
        attempts: max_attempts,
        last_error,
    })
}

fn elapsed_ms(start: chrono::DateTime<chrono::Utc>) -> u64 {
    let diff = Utc::now().signed_duration_since(start);
    diff.num_milliseconds().max(0) as u64
}

#[cfg(test)]
mod llm_judge_executor {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::context::ExecutionContext;
    use crate::executor::{EvalReport, Executors, TaskRunOutcome, execute_plan};
    use crate::store::TaskRegistry;
    use crate::tasks::{
        AgentTaskExecutor, AssertionTaskExecutor, JudgeTaskExecutor, TraceTaskExecutor,
    };
    use crate::{InMemoryTraceSource, JudgeError, MockJudgeInvoker};
    use serde_json::{Value, json};
    use uuid::Uuid;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_spec::vala::eval::{
        AssertionTask, ComparisonOperator, EvalSpec, EvalTask, JsonPath, LlmJudgeTask, RecordId,
        RunId, TaskId,
    };

    fn tid(value: &str) -> TaskId {
        TaskId::new(value).expect("static task id is valid")
    }

    fn jp(value: &str) -> JsonPath {
        JsonPath::new(value).expect("static JSONPath is valid")
    }

    fn judge_card_ref() -> CardRef {
        CardRef {
            kind: CardKind::Agent,
            name: CardName::new("eval-judge").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("default").expect("valid space")),
            uid: None,
        }
    }

    fn llm_judge(
        id: &str,
        expected: Value,
        op: ComparisonOperator,
        deps: &[&str],
        max_retries: u32,
    ) -> EvalTask {
        EvalTask::LlmJudge(LlmJudgeTask {
            id: tid(id),
            judge_ref: judge_card_ref().into(),
            context_path: None,
            expected,
            operator: op,
            depends_on: deps.iter().map(|dep| tid(dep)).collect(),
            max_retries,
            condition: None,
        })
    }

    fn assertion(
        id: &str,
        path: &str,
        op: ComparisonOperator,
        expected: Value,
        deps: &[&str],
    ) -> EvalTask {
        EvalTask::Assertion(AssertionTask {
            id: tid(id),
            context_path: Some(jp(path)),
            item_context_path: None,
            operator: op,
            expected,
            depends_on: deps.iter().map(|dep| tid(dep)).collect(),
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

    fn context(base: Value) -> ExecutionContext {
        ExecutionContext::new(
            base,
            RunId::from_string("run-llm-judge".to_owned()),
            RecordId(Uuid::from_u128(10)),
            None,
        )
    }

    async fn drive(spec: EvalSpec, base: Value, mock: Arc<MockJudgeInvoker>) -> EvalReport {
        let plan = spec.execution_plan().expect("test spec has valid DAG");
        let registry = TaskRegistry::from_plan(&plan, spec.tasks.clone()).expect("registry builds");
        execute_plan(&plan, &context(base), &registry, &executors(mock))
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
    async fn missing_invoker_is_construction_concern_not_runtime() {
        let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
        let spec = spec_of(vec![llm_judge(
            "judge",
            json!("pass"),
            ComparisonOperator::Equals,
            &[],
            0,
        )]);
        let report = drive(spec, json!({"response": "ok"}), mock).await;
        assert!(result(&report, "judge").passed);
    }

    #[tokio::test]
    async fn happy_path_stores_verdict_addressable_downstream() {
        let mock = MockJudgeInvoker::new([Ok(json!({"verdict": "pass"}))]);
        let spec = spec_of(vec![
            llm_judge(
                "judge",
                json!({"verdict": "pass"}),
                ComparisonOperator::Equals,
                &[],
                0,
            ),
            assertion(
                "downstream",
                "$.task.judge.parsed.verdict",
                ComparisonOperator::Equals,
                json!("pass"),
                &["judge"],
            ),
        ]);
        let report = drive(spec, json!({"response": "ok"}), mock).await;
        assert!(result(&report, "judge").passed);
        assert!(result(&report, "downstream").passed);
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let mock = MockJudgeInvoker::new([
            Err(JudgeError::Retryable {
                reason: "provider 503".to_owned(),
            }),
            Err(JudgeError::Timeout { elapsed_ms: 250 }),
            Ok(json!("pass")),
        ]);
        let spec = spec_of(vec![llm_judge(
            "judge",
            json!("pass"),
            ComparisonOperator::Equals,
            &[],
            3,
        )]);
        let report = drive(spec, json!({"response": "ok"}), Arc::clone(&mock)).await;
        assert!(result(&report, "judge").passed);
        assert_eq!(mock.calls().await.len(), 3);
    }

    #[tokio::test]
    async fn retry_budget_exhausted_errors() {
        let mock = MockJudgeInvoker::new([
            Err(JudgeError::Retryable {
                reason: "provider 503".to_owned(),
            }),
            Err(JudgeError::Retryable {
                reason: "provider still 503".to_owned(),
            }),
        ]);
        let spec = spec_of(vec![llm_judge(
            "judge",
            json!("pass"),
            ComparisonOperator::Equals,
            &[],
            1,
        )]);
        let report = drive(spec, json!({"response": "ok"}), Arc::clone(&mock)).await;
        let result = result(&report, "judge");
        assert!(!result.passed);
        assert_eq!(mock.calls().await.len(), 2);
        assert!(
            result
                .message
                .as_deref()
                .unwrap_or("")
                .contains("judge failed after 2 attempt")
        );
    }

    #[tokio::test]
    async fn invalid_structured_output_short_circuits() {
        let mock = MockJudgeInvoker::new([Err(JudgeError::InvalidStructuredOutput {
            reason: "missing verdict".to_owned(),
        })]);
        let spec = spec_of(vec![llm_judge(
            "judge",
            json!("pass"),
            ComparisonOperator::Equals,
            &[],
            3,
        )]);
        let report = drive(spec, json!({"response": "ok"}), Arc::clone(&mock)).await;
        let result = result(&report, "judge");
        assert!(!result.passed);
        assert_eq!(mock.calls().await.len(), 1);
        assert!(
            result
                .message
                .as_deref()
                .unwrap_or("")
                .contains("missing verdict")
        );
    }

    #[tokio::test]
    async fn context_path_narrows_invoker_input() {
        let mock = MockJudgeInvoker::new([Ok(json!("pass"))]);
        let mut task = match llm_judge("judge", json!("pass"), ComparisonOperator::Equals, &[], 0) {
            EvalTask::LlmJudge(task) => task,
            _ => unreachable!("fixture returns judge"),
        };
        task.context_path = Some(jp("$.response"));
        let spec = spec_of(vec![EvalTask::LlmJudge(task)]);

        let report = drive(
            spec,
            json!({"response": "what the judge sees", "extra": "hidden"}),
            Arc::clone(&mock),
        )
        .await;
        assert!(result(&report, "judge").passed);
        let calls = mock.calls().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, json!("what the judge sees"));
    }
}
