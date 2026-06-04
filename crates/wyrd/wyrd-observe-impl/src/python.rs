//! Python bridge: PythonObserver + set_observer.

#![cfg(feature = "python")]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use skald_agent::observer::Observer;

/// Wraps a Python object that subclasses `wyrd.Observer`.
///
/// Each observer method offloads GIL acquisition to the blocking thread pool
/// via `tokio::task::spawn_blocking`. This prevents Python attachment from
/// blocking a Tokio async worker thread when parallel workflow steps fire
/// observer events simultaneously.
///
/// `Arc<Py<PyAny>>` is used because `Py<T>::clone()` requires the GIL token
/// in PyO3 0.28+ (`clone_ref(py)`). `Arc::clone` is GIL-free and lets us
/// cheaply move the handle into `spawn_blocking` closures without acquiring
/// the GIL twice.
pub struct PythonObserver(Arc<Py<PyAny>>);

// PythonObserver implements all Observer methods by firing spawn_blocking.
// Each method follows the same pattern:
//   1. Clone self.0 (Arc::clone — cheap, no GIL required)
//   2. Own all &str args as String
//   3. spawn_blocking -> Python::attach -> call_method1
//   4. .await.ok() - observer failures never propagate
#[async_trait]
impl Observer for PythonObserver {
    async fn on_agent_start(
        &self,
        run_id: &str,
        parent_run_id: Option<&str>,
        agent_id: &str,
        input: &str,
        session_id: Option<&str>,
    ) {
        let run_id = run_id.to_owned();
        let parent_run_id = parent_run_id.map(str::to_owned);
        let agent_id = agent_id.to_owned();
        let input = input.to_owned();
        let session_id = session_id.map(str::to_owned);
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_agent_start",
                    (run_id, parent_run_id, agent_id, input, session_id),
                );
            });
        })
        .await
        .ok();
    }

    async fn on_iteration(&self, run_id: &str, agent_id: &str, index: u32) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(py, "on_iteration", (run_id, agent_id, index));
            });
        })
        .await
        .ok();
    }

    async fn on_model_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        provider: &str,
        model: &str,
    ) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let provider = provider.to_owned();
        let model = model.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_model_call",
                    (run_id, agent_id, iteration, provider, model),
                );
            });
        })
        .await
        .ok();
    }

    async fn on_model_result(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        finish_reason: &str,
        synthetic: bool,
    ) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let finish_reason = finish_reason.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_model_result",
                    (run_id, agent_id, iteration, finish_reason, synthetic),
                );
            });
        })
        .await
        .ok();
    }

    async fn on_tool_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        tool_name: &str,
    ) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let call_id = call_id.to_owned();
        let tool_name = tool_name.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_tool_call",
                    (run_id, agent_id, iteration, call_id, tool_name),
                );
            });
        })
        .await
        .ok();
    }

    async fn on_tool_result(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        ok: bool,
    ) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let call_id = call_id.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_tool_result",
                    (run_id, agent_id, iteration, call_id, ok),
                );
            });
        })
        .await
        .ok();
    }

    async fn on_agent_finish(
        &self,
        run_id: &str,
        agent_id: &str,
        finish_reason: &str,
        iterations: u32,
        duration: Duration,
    ) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let finish_reason = finish_reason.to_owned();
        let duration_ms = duration.as_millis() as u64;
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_agent_finish",
                    (run_id, agent_id, finish_reason, iterations, duration_ms),
                );
            });
        })
        .await
        .ok();
    }

    async fn on_agent_error(&self, run_id: &str, agent_id: &str, code: &str, message: &str) {
        let run_id = run_id.to_owned();
        let agent_id = agent_id.to_owned();
        let code = code.to_owned();
        let message = message.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(py, "on_agent_error", (run_id, agent_id, code, message));
            });
        })
        .await
        .ok();
    }

    async fn on_workflow_start(&self, run_id: &str, workflow_id: &str, step_count: usize) {
        let run_id = run_id.to_owned();
        let workflow_id = workflow_id.to_owned();
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ =
                    inner.call_method1(py, "on_workflow_start", (run_id, workflow_id, step_count));
            });
        })
        .await
        .ok();
    }

    async fn on_workflow_finish(&self, run_id: &str, workflow_id: &str, duration: Duration) {
        let run_id = run_id.to_owned();
        let workflow_id = workflow_id.to_owned();
        let duration_ms = duration.as_millis() as u64;
        let inner = Arc::clone(&self.0);
        tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let _ = inner.call_method1(
                    py,
                    "on_workflow_finish",
                    (run_id, workflow_id, duration_ms),
                );
            });
        })
        .await
        .ok();
    }
}

/// Install a Python observer object as the process-wide global observer.
///
/// The observer must be a subclass of `wyrd.Observer`. It is wrapped in a
/// `PythonObserver` bridge that dispatches Rust observer events to the Python
/// object via `spawn_blocking + Python::attach`.
///
/// Subsequent calls are silent no-ops (first caller wins, matching
/// `wyrd_observe::set_global` semantics).
#[pyfunction]
pub fn set_observer(observer: Py<PyAny>) {
    wyrd_observe::set_global(Arc::new(PythonObserver(Arc::new(observer))));
}

/// Initialize the Rust observer bridge.
#[pyfunction]
pub fn _init() {
    crate::init();
}

/// Register `_init` and `set_observer` on `wyrd._wyrd`.
///
/// # Errors
///
/// Returns PyO3 registration errors.
pub fn python_register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(_init, module)?)?;
    module.add_function(wrap_pyfunction!(set_observer, module)?)?;
    Ok(())
}
