"""OTel observer for workflow-scoped Wyrd instrumentation."""

from __future__ import annotations

import warnings
from threading import Lock

from wyrd.observer import Observer


class OtelObserver(Observer):
    """OTel observer attached through `Workflow(observers=[...])`.

    Add `OtelObserver()` to a workflow's observer list to emit spans into the
    Python OpenTelemetry tracer provider configured by the host application.
    """

    def __init__(self, tracer=None) -> None:
        try:
            from opentelemetry import trace
            from opentelemetry.trace import ProxyTracerProvider
        except ImportError as e:
            raise ImportError(
                "OtelObserver requires opentelemetry-api. "
                "Install it with: pip install opentelemetry-api"
            ) from e

        if tracer is None:
            provider = trace.get_tracer_provider()
            if isinstance(provider, ProxyTracerProvider):
                warnings.warn(
                    "OtelObserver: no OTel tracer provider configured - spans will be "
                    "discarded. Configure a provider before constructing workflows "
                    "with OtelObserver().",
                    stacklevel=2,
                )
            self._tracer = trace.get_tracer("wyrd")
        else:
            self._tracer = tracer

        self._spans: dict[str, object] = {}
        self._lock = Lock()

    def _set_span(self, key: str, span: object) -> None:
        with self._lock:
            self._spans[key] = span

    def _get_span(self, key: str) -> object | None:
        with self._lock:
            return self._spans.get(key)

    def _pop_span(self, key: str) -> object | None:
        with self._lock:
            return self._spans.pop(key, None)

    def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id) -> None:
        from opentelemetry import trace
        from opentelemetry.trace import SpanKind

        parent_span = self._get_span(f"wf.{parent_run_id}") if parent_run_id else None
        parent_ctx = trace.set_span_in_context(parent_span) if parent_span is not None else None

        span = self._tracer.start_span(
            f"wyrd.agent.run/{agent_id}",
            context=parent_ctx,
            kind=SpanKind.INTERNAL,
            attributes={
                "wyrd.run_id": run_id,
                "wyrd.agent.id": agent_id,
            },
        )
        self._set_span(run_id, span)

    def on_model_call(self, run_id, agent_id, iteration, provider, model, request) -> None:
        from opentelemetry import trace
        from opentelemetry.trace import SpanKind

        agent_span = self._get_span(run_id)
        parent_ctx = trace.set_span_in_context(agent_span) if agent_span is not None else None

        span = self._tracer.start_span(
            f"wyrd.model.call/{provider}",
            context=parent_ctx,
            kind=SpanKind.CLIENT,
            attributes={
                "wyrd.run_id": run_id,
                "wyrd.agent.id": agent_id,
                "wyrd.iteration": iteration,
                "gen_ai.system": provider,
                "gen_ai.request.model": model,
            },
        )
        self._set_span(f"{run_id}.model.{iteration}", span)

    def on_model_result(
        self,
        run_id,
        agent_id,
        iteration,
        finish_reason,
        synthetic,
        response,
    ) -> None:
        span = self._pop_span(f"{run_id}.model.{iteration}")
        if span is not None:
            span.set_attribute("gen_ai.response.finish_reason", finish_reason)
            span.end()

    def on_tool_call(self, run_id, agent_id, iteration, call_id, tool_name) -> None:
        from opentelemetry import trace
        from opentelemetry.trace import SpanKind

        agent_span = self._get_span(run_id)
        parent_ctx = trace.set_span_in_context(agent_span) if agent_span is not None else None

        span = self._tracer.start_span(
            f"wyrd.tool.call/{tool_name}",
            context=parent_ctx,
            kind=SpanKind.INTERNAL,
            attributes={
                "wyrd.run_id": run_id,
                "wyrd.agent.id": agent_id,
                "wyrd.iteration": iteration,
                "wyrd.call_id": call_id,
                "tool.name": tool_name,
            },
        )
        self._set_span(f"{run_id}.tool.{call_id}", span)

    def on_tool_result(self, run_id, agent_id, iteration, call_id, ok) -> None:
        from opentelemetry.trace import StatusCode

        span = self._pop_span(f"{run_id}.tool.{call_id}")
        if span is not None:
            if not ok:
                span.set_status(StatusCode.ERROR, "tool call failed")
            span.end()

    def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms) -> None:
        span = self._pop_span(run_id)
        if span is not None:
            span.set_attribute("wyrd.finish_reason", finish_reason)
            span.set_attribute("wyrd.iterations", iterations)
            span.set_attribute("wyrd.duration_ms", duration_ms)
            span.end()

    def on_agent_error(self, run_id, agent_id, code, message) -> None:
        from opentelemetry.trace import StatusCode

        span = self._pop_span(run_id)
        if span is not None:
            span.set_status(StatusCode.ERROR, message)
            span.set_attribute("error.code", code)
            span.end()

    def on_workflow_start(self, run_id, workflow_id, step_count) -> None:
        from opentelemetry.trace import SpanKind

        span = self._tracer.start_span(
            f"wyrd.workflow.run/{workflow_id}",
            kind=SpanKind.INTERNAL,
            attributes={
                "wyrd.run_id": run_id,
                "wyrd.workflow.id": workflow_id,
                "wyrd.workflow.step_count": step_count,
            },
        )
        self._set_span(f"wf.{run_id}", span)

    def on_workflow_finish(self, run_id, workflow_id, duration_ms) -> None:
        span = self._pop_span(f"wf.{run_id}")
        if span is not None:
            span.set_attribute("wyrd.duration_ms", duration_ms)
            span.end()
