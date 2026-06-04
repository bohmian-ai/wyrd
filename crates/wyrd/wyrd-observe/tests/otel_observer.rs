//! OtelObserver integration tests — require the `otel` feature flag.

#![cfg(feature = "otel")]

use std::sync::Arc;
use std::time::Duration;

use skald_agent::Observer;
use wyrd_observe::OtelObserver;

fn noop_observer() -> OtelObserver {
    // Uses the global no-op provider. Spans are discarded but span store
    // operations run normally, letting us verify thread safety without an exporter.
    OtelObserver::new()
}

#[tokio::test]
async fn otel_observer_concurrent_workflow_steps_no_deadlock() {
    // Mirrors the real workflow dispatch path: one observer shared across
    // concurrent Tokio tasks, each representing a parallel workflow step.
    // Verifies the RwLock on the span/context stores doesn't deadlock under load.
    let obs = Arc::new(noop_observer());
    let mut handles = Vec::with_capacity(8);

    for i in 0_u32..8 {
        let obs = Arc::clone(&obs);
        handles.push(tokio::spawn(async move {
            let run_id = format!("step-{i}");
            let wf_run_id = "wf-concurrent";
            obs.on_agent_start(&run_id, Some(wf_run_id), "agent", "input", None)
                .await;
            obs.on_model_call(&run_id, "agent", 0, "openai", "gpt-4o")
                .await;
            obs.on_tool_call(&run_id, "agent", 0, &format!("call-{i}"), "search")
                .await;
            obs.on_tool_result(&run_id, "agent", 0, &format!("call-{i}"), true)
                .await;
            obs.on_model_result(&run_id, "agent", 0, "stop", false)
                .await;
            obs.on_agent_finish(
                &run_id,
                "agent",
                "model_stopped",
                1,
                Duration::from_millis(10),
            )
            .await;
        }));
    }

    for handle in handles {
        handle.await.expect("task");
    }
}

#[tokio::test]
async fn otel_observer_orphaned_events_do_not_panic() {
    // on_model_result / on_tool_result with no preceding start must be silent no-ops.
    // This can happen if an observer is installed after a run is already in progress.
    let obs = noop_observer();
    obs.on_model_result("ghost", "agent", 0, "stop", false)
        .await;
    obs.on_tool_result("ghost", "agent", 0, "call-x", false)
        .await;
    obs.on_agent_finish("ghost", "agent", "model_stopped", 0, Duration::ZERO)
        .await;
}
