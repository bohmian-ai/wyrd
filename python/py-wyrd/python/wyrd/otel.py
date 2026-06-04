"""OTel observer and WyrdInstrumentor."""

from __future__ import annotations

import warnings
from threading import Lock

import wyrd as _wyrd
from wyrd.observer import Observer


class _CompositeObserver(Observer):
    def __init__(self, observers: list[Observer]) -> None:
        self._observers = list(observers)  # immutable after construction

    def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id) -> None:
        for obs in self._observers:
            obs.on_agent_start(run_id, parent_run_id, agent_id, input, session_id)

    def on_iteration(self, run_id, agent_id, index) -> None:
        for obs in self._observers:
            obs.on_iteration(run_id, agent_id, index)

    def on_model_call(self, run_id, agent_id, iteration, provider, model) -> None:
        for obs in self._observers:
            obs.on_model_call(run_id, agent_id, iteration, provider, model)

    def on_model_result(self, run_id, agent_id, iteration, finish_reason, synthetic) -> None:
        for obs in self._observers:
            obs.on_model_result(run_id, agent_id, iteration, finish_reason, synthetic)

    def on_tool_call(self, run_id, agent_id, iteration, call_id, tool_name) -> None:
        for obs in self._observers:
            obs.on_tool_call(run_id, agent_id, iteration, call_id, tool_name)

    def on_tool_result(self, run_id, agent_id, iteration, call_id, ok) -> None:
        for obs in self._observers:
            obs.on_tool_result(run_id, agent_id, iteration, call_id, ok)

    def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms) -> None:
        for obs in self._observers:
            obs.on_agent_finish(run_id, agent_id, finish_reason, iterations, duration_ms)

    def on_agent_error(self, run_id, agent_id, code, message) -> None:
        for obs in self._observers:
            obs.on_agent_error(run_id, agent_id, code, message)

    def on_workflow_start(self, run_id, workflow_id, step_count) -> None:
        for obs in self._observers:
            obs.on_workflow_start(run_id, workflow_id, step_count)

    def on_workflow_finish(self, run_id, workflow_id, duration_ms) -> None:
        for obs in self._observers:
            obs.on_workflow_finish(run_id, workflow_id, duration_ms)


class OtelObserver(Observer):
    """OTel observer. Automatically uses the globally configured tracer provider.

    Zero-config usage - spans go to whatever OTel provider is configured:

        WyrdInstrumentor().instrument()   # installs OtelObserver by default

    Thread-safe. The internal span store is protected by threading.Lock, which
    works correctly under both standard CPython (GIL) and no-GIL Python
    (PEP 703 / free-threading). The lock is held only during dict operations,
    never during span creation or ending.

    If no real OTel provider is configured (i.e., the global provider is still
    the default ProxyTracerProvider), a warning is emitted and spans are
    silently discarded. This is not an error.
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
                    "discarded. Configure a provider before calling "
                    "WyrdInstrumentor().instrument().",
                    stacklevel=2,
                )
            self._tracer = trace.get_tracer("wyrd")
        else:
            self._tracer = tracer

        # span_key -> trace.Span
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
        from opentelemetry.trace import SpanKind

        span = self._tracer.start_span(
            f"wyrd.agent.run/{agent_id}",
            kind=SpanKind.INTERNAL,
            attributes={
                "wyrd.run_id": run_id,
                "wyrd.agent.id": agent_id,
            },
        )
        self._set_span(run_id, span)

    def on_model_call(self, run_id, agent_id, iteration, provider, model) -> None:
        from opentelemetry.trace import SpanKind

        span = self._tracer.start_span(
            f"wyrd.model.call/{provider}",
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

    def on_model_result(self, run_id, agent_id, iteration, finish_reason, synthetic) -> None:
        span = self._pop_span(f"{run_id}.model.{iteration}")
        if span is not None:
            span.set_attribute("gen_ai.response.finish_reason", finish_reason)
            span.end()

    def on_tool_call(self, run_id, agent_id, iteration, call_id, tool_name) -> None:
        from opentelemetry.trace import SpanKind

        span = self._tracer.start_span(
            f"wyrd.tool.call/{tool_name}",
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


class WyrdInstrumentor:
    """Wires a Wyrd observer into the agent runtime.

    Defaults to OtelObserver when called with no arguments. Spans go to
    whatever OTel provider the user (or their deployment) configured.

    Usage:

        # Zero-config - OTel spans via global provider
        WyrdInstrumentor().instrument()

        # Custom observer
        WyrdInstrumentor().instrument(observer=MyObserver())

        # Multiple observers
        WyrdInstrumentor().instrument(observers=[obs_a, obs_b])

        # Mix
        WyrdInstrumentor().instrument(observer=my_otel, observers=[my_logger])
    """

    def instrument(
        self,
        *,
        observer: Observer | None = None,
        observers: list[Observer] | None = None,
    ) -> None:
        """Install the observer(s) into the Wyrd runtime.

        Calls wyrd.set_observer which installs the observer process-wide.
        The first call wins - subsequent calls are no-ops.

        Args:
            observer: Optional single observer. Prepended to the observers list.
            observers: Optional list of observers. Combined with observer if both given.
        """
        combined: list[Observer] = []
        if observer is not None:
            combined.append(observer)
        if observers:
            combined.extend(observers)
        if not combined:
            combined.append(OtelObserver())
        active = _CompositeObserver(combined) if len(combined) > 1 else combined[0]
        _wyrd.set_observer(active)

    def uninstrument(self) -> None:
        """No-op - the global observer cannot be uninstalled after first install."""
