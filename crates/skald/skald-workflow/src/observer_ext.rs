//! Workflow-level event emission through the agent observer hook.

use skald_agent::Observer;
use tracing::debug_span;

use crate::run::TaskEvent;

pub(crate) fn emit_task_event(observer: &dyn Observer, event: &TaskEvent) {
    let _span = debug_span!(
        "skald_workflow.task",
        task_id = event.task_id.as_str(),
        status = ?event.status,
        attempt = event.attempt,
        error = event.error.as_deref().unwrap_or(""),
    )
    .entered();
    observer.on_iteration(&format!("task:{}", event.task_id), event.attempt);
}
