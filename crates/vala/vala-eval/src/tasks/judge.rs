//! `LlmJudgeTask` executor.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use wyrd_spec::vala::eval::{AssertionResult, EvalTask, LlmJudgeTask};

use crate::context::{ContextSnapshot, TaskOutput, extract_required_jsonpath_from};
use crate::error::EvalExecError;
use crate::executor::TaskExecutor;
use crate::judge::{JudgeError, JudgeInvoker};
use crate::operators;
use crate::store::JudgeOutcome;
use crate::tasks::media::{MediaBindings, bindings_as_context};

/// Executor for prompt-backed LLM judge tasks.
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
