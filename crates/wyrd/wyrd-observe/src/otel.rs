//! OTel reference observer implementation.
//!
//! Enabled with the `otel` feature flag. Uses the globally configured OTel
//! tracer provider - whatever provider the user (or vala) installed via
//! `opentelemetry::global::set_tracer_provider`. If none is configured, spans
//! go to the no-op provider silently.

#![cfg(feature = "otel")]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use opentelemetry::global::{self, BoxedSpan, BoxedTracer};
use opentelemetry::trace::{Span, SpanContext, SpanKind, Status, TraceContextExt, Tracer};
use skald_agent::observer::Observer;

/// Span store - keyed by run_id (and run_id + suffix for sub-spans).
/// RwLock guards structural mutations; Mutex guards individual span mutation
/// (set_attribute + end take &mut self). Different run_ids never contend.
type SpanStore = Arc<RwLock<HashMap<String, Mutex<BoxedSpan>>>>;

/// SpanContext store for parent-child linking.
/// SpanContext is Clone and contains only IDs — no Mutex needed.
/// Stored separately so child span creation never needs to hold the span mutex.
type ContextStore = Arc<RwLock<HashMap<String, SpanContext>>>;

/// OTel observer. Creates and ends spans for each agent lifecycle event.
///
/// Span hierarchy:
/// - `wyrd.workflow.run` (trace root, from `on_workflow_start`)
///   - `wyrd.agent.run` (child of workflow when `parent_run_id` is set)
///     - `wyrd.model.call` (child of agent span)
///     - `wyrd.tool.call` (child of agent span)
///
/// Thread-safe for concurrent workflow steps. Each step uses a unique run_id
/// so span store operations for different runs never collide.
///
/// # Usage
///
/// Install a tracer provider before running agents:
///
/// ```rust,no_run
/// // The OtelObserver picks up whatever provider is globally registered.
/// use wyrd_observe::{set_global, OtelObserver};
/// use std::sync::Arc;
/// set_global(Arc::new(OtelObserver::new()));
/// ```
pub struct OtelObserver {
    tracer: BoxedTracer,
    spans: SpanStore,
    /// SpanContexts stored separately for parent lookup without holding span mutex.
    contexts: ContextStore,
}

impl OtelObserver {
    /// Create an OtelObserver using the globally configured tracer provider.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tracer: global::tracer("wyrd"),
            spans: Arc::new(RwLock::new(HashMap::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create an OtelObserver with a specific named tracer scope.
    #[must_use]
    pub fn with_scope(scope: &'static str) -> Self {
        Self {
            tracer: global::tracer(scope),
            spans: Arc::new(RwLock::new(HashMap::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn insert_span(&self, key: String, span: BoxedSpan) {
        // Extract SpanContext (just IDs, Clone-able) before moving span into mutex.
        let sc = span.span_context().clone();
        if let Ok(mut store) = self.spans.write() {
            store.insert(key.clone(), Mutex::new(span));
        }
        if let Ok(mut ctxs) = self.contexts.write() {
            ctxs.insert(key, sc);
        }
    }

    fn remove_span(&self, key: &str) -> Option<Mutex<BoxedSpan>> {
        if let Ok(mut ctxs) = self.contexts.write() {
            ctxs.remove(key);
        }
        self.spans.write().ok()?.remove(key)
    }

    /// Build an OTel Context that makes `parent_key`'s span the parent of a new span.
    ///
    /// Returns the current ambient context when the parent key is not found —
    /// this produces a new trace root, which is correct for standalone agent runs.
    fn make_parent_context(&self, parent_key: &str) -> opentelemetry::Context {
        let sc = self
            .contexts
            .read()
            .ok()
            .and_then(|store| store.get(parent_key).cloned());
        match sc {
            Some(sc) => opentelemetry::Context::current().with_remote_span_context(sc),
            None => opentelemetry::Context::current(),
        }
    }
}

impl Default for OtelObserver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Observer for OtelObserver {
    async fn on_agent_start(
        &self,
        run_id: &str,
        parent_run_id: Option<&str>,
        agent_id: &str,
        _input: &str,
        _session_id: Option<&str>,
    ) {
        // When this agent is a workflow step, the workflow span is the parent.
        // Workflow spans are keyed "wf.{workflow_run_id}".
        // When parent_run_id is None (standalone run), this is a new trace root.
        let parent_cx = parent_run_id
            .map(|id| self.make_parent_context(&format!("wf.{id}")))
            .unwrap_or_else(opentelemetry::Context::current);
        let span = self
            .tracer
            .span_builder(format!("wyrd.agent.run/{agent_id}"))
            .with_kind(SpanKind::Internal)
            .with_attributes([
                opentelemetry::KeyValue::new("wyrd.run_id", run_id.to_owned()),
                opentelemetry::KeyValue::new("wyrd.agent.id", agent_id.to_owned()),
            ])
            .start_with_context(&self.tracer, &parent_cx);
        self.insert_span(run_id.to_owned(), span);
    }

    async fn on_model_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        provider: &str,
        model: &str,
        _request: &skald_spec::ProviderRequest,
    ) {
        // Agent span (keyed by run_id) is the parent of model call spans.
        let parent_cx = self.make_parent_context(run_id);
        let span = self
            .tracer
            .span_builder(format!("wyrd.model.call/{provider}"))
            .with_kind(SpanKind::Client)
            .with_attributes([
                opentelemetry::KeyValue::new("wyrd.run_id", run_id.to_owned()),
                opentelemetry::KeyValue::new("wyrd.agent.id", agent_id.to_owned()),
                opentelemetry::KeyValue::new("wyrd.iteration", i64::from(iteration)),
                opentelemetry::KeyValue::new("gen_ai.system", provider.to_owned()),
                opentelemetry::KeyValue::new("gen_ai.request.model", model.to_owned()),
            ])
            .start_with_context(&self.tracer, &parent_cx);
        self.insert_span(format!("{run_id}.model.{iteration}"), span);
    }

    async fn on_model_result(
        &self,
        run_id: &str,
        _agent_id: &str,
        iteration: u32,
        finish_reason: &str,
        _synthetic: bool,
        _response: &skald_spec::ProviderResponse,
    ) {
        let key = format!("{run_id}.model.{iteration}");
        if let Some(mutex) = self.remove_span(&key) {
            if let Ok(mut span) = mutex.into_inner() {
                span.set_attribute(opentelemetry::KeyValue::new(
                    "gen_ai.response.finish_reason",
                    finish_reason.to_owned(),
                ));
                span.end();
            }
        }
    }

    async fn on_tool_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        tool_name: &str,
    ) {
        // Agent span (keyed by run_id) is the parent of tool call spans.
        let parent_cx = self.make_parent_context(run_id);
        let span = self
            .tracer
            .span_builder(format!("wyrd.tool.call/{tool_name}"))
            .with_kind(SpanKind::Internal)
            .with_attributes([
                opentelemetry::KeyValue::new("wyrd.run_id", run_id.to_owned()),
                opentelemetry::KeyValue::new("wyrd.agent.id", agent_id.to_owned()),
                opentelemetry::KeyValue::new("wyrd.iteration", i64::from(iteration)),
                opentelemetry::KeyValue::new("wyrd.call_id", call_id.to_owned()),
                opentelemetry::KeyValue::new("tool.name", tool_name.to_owned()),
            ])
            .start_with_context(&self.tracer, &parent_cx);
        self.insert_span(format!("{run_id}.tool.{call_id}"), span);
    }

    async fn on_tool_result(
        &self,
        run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        call_id: &str,
        ok: bool,
    ) {
        let key = format!("{run_id}.tool.{call_id}");
        if let Some(mutex) = self.remove_span(&key) {
            if let Ok(mut span) = mutex.into_inner() {
                if !ok {
                    span.set_status(Status::error("tool call failed"));
                }
                span.end();
            }
        }
    }

    async fn on_agent_finish(
        &self,
        run_id: &str,
        _agent_id: &str,
        finish_reason: &str,
        iterations: u32,
        _duration: Duration,
    ) {
        if let Some(mutex) = self.remove_span(run_id) {
            if let Ok(mut span) = mutex.into_inner() {
                span.set_attribute(opentelemetry::KeyValue::new(
                    "wyrd.finish_reason",
                    finish_reason.to_owned(),
                ));
                span.set_attribute(opentelemetry::KeyValue::new(
                    "wyrd.iterations",
                    i64::from(iterations),
                ));
                span.end();
            }
        }
    }

    async fn on_agent_error(&self, run_id: &str, _agent_id: &str, code: &str, message: &str) {
        if let Some(mutex) = self.remove_span(run_id) {
            if let Ok(mut span) = mutex.into_inner() {
                span.set_status(Status::error(message.to_owned()));
                span.set_attribute(opentelemetry::KeyValue::new("error.code", code.to_owned()));
                span.end();
            }
        }
    }

    async fn on_workflow_start(&self, run_id: &str, workflow_id: &str, step_count: usize) {
        let span = self
            .tracer
            .span_builder(format!("wyrd.workflow.run/{workflow_id}"))
            .with_kind(SpanKind::Internal)
            .with_attributes([
                opentelemetry::KeyValue::new("wyrd.run_id", run_id.to_owned()),
                opentelemetry::KeyValue::new("wyrd.workflow.id", workflow_id.to_owned()),
                opentelemetry::KeyValue::new(
                    "wyrd.workflow.step_count",
                    i64::try_from(step_count).unwrap_or(i64::MAX),
                ),
            ])
            .start(&self.tracer);
        self.insert_span(format!("wf.{run_id}"), span);
    }

    async fn on_workflow_finish(&self, run_id: &str, _workflow_id: &str, _duration: Duration) {
        let key = format!("wf.{run_id}");
        if let Some(mutex) = self.remove_span(&key) {
            if let Ok(mut span) = mutex.into_inner() {
                span.end();
            }
        }
    }
}
