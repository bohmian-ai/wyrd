"""OTel observer for workflow-scoped Wyrd instrumentation, and Run span correlation.

``Run.__enter__``/``__exit__`` delegate here so spans created inside
``with state.run(...)`` carry the record-level ``wyrd.card_ref`` and
``wyrd.run_id`` attributes Bifrost extracts. Everything in that path is
optional and fail-open: without ``opentelemetry-api`` it is a no-op.
"""

from __future__ import annotations

import warnings
import weakref
from threading import Lock
from typing import Any

from wyrd.observer import Observer

try:
    from opentelemetry import context as _otel_context
    from opentelemetry import trace as _otel_trace
except ImportError:  # optional: run correlation becomes a no-op
    _otel_context = None
    _otel_trace = None


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


_CARD_REF = "wyrd.card_ref"
_RUN_ID = "wyrd.run_id"

# The Run scope stack lives entirely in this one context value: a tuple of
# ``(card_ref, run_id)`` pairs, innermost last. Entry and exit each attach a new
# value and never detach, so no token or per-scope state exists outside it.
_SCOPE_KEY: Any = None if _otel_context is None else _otel_context.create_key("wyrd.run_scope")

# Per-provider registration outcome as ``[weakref, outcome]`` pairs matched by
# referent identity (never equality), written before the foreign call and never
# discarded while the provider lives, so each provider object is asked at most
# once. A linear scan suits the handful of providers a process holds.
_outcomes: list[list[Any]] = []
_outcomes_lock = Lock()


class _RunCorrelationProcessor:
    """Span processor copying the innermost Run scope onto every started span.

    Duck-typed rather than subclassing the SDK ``SpanProcessor`` so the OTel SDK
    stays optional. Each registration attempt owns one instance, inert until
    ``add_span_processor`` returns normally, so a provider that kept it and then
    raised never enriches. Every hook swallows its own failures.
    """

    def __init__(self) -> None:
        self.active = False

    def on_start(self, span: Any, parent_context: Any = None) -> None:
        if not self.active:
            return
        try:
            stack = _otel_context.get_value(_SCOPE_KEY, parent_context)
            if stack:
                card_ref, run_id = stack[-1]
                span.set_attribute(_CARD_REF, card_ref)
                span.set_attribute(_RUN_ID, run_id)
        except Exception:  # telemetry must never fail the app
            pass

    def _on_ending(self, span: Any) -> None:
        pass

    def on_end(self, span: Any) -> None:
        pass

    def shutdown(self) -> None:
        pass

    def force_flush(self, timeout_millis: int = 30000) -> bool:
        return True


def _outcome_entry(provider: Any) -> list[Any] | None:
    """Return ``provider``'s outcome entry by identity, pruning dead entries.

    Caller holds ``_outcomes_lock``.
    """
    _outcomes[:] = [entry for entry in _outcomes if entry[0]() is not None]
    for entry in _outcomes:
        if entry[0]() is provider:
            return entry
    return None


def install_run_correlation(provider: Any = None) -> bool:
    """Register the Wyrd Run-correlation span processor on ``provider``.

    ``provider`` defaults to the global tracer provider. Registration is
    thread-safe, idempotent, and attempted at most once per provider: the
    outcome is cached and returned on every later call. Returns ``True`` when
    the provider accepted the processor, ``False`` when OpenTelemetry is absent
    or the provider cannot be weakly referenced, lacks ``add_span_processor``,
    or raised while registering. A failed provider is never retried, and a
    processor it kept before raising stays inert. Never raises. Pass a framework's
    private provider once; the global provider is installed on every ``Run``
    entry.
    """
    if _otel_trace is None:
        return False
    try:
        if provider is None:
            provider = _otel_trace.get_tracer_provider()
        with _outcomes_lock:
            entry = _outcome_entry(provider)
            if entry is not None:
                return entry[1]
            entry = [weakref.ref(provider), False]
            _outcomes.append(entry)
            add = getattr(provider, "add_span_processor", None)
            if add is None:
                return False
            processor = _RunCorrelationProcessor()
            add(processor)
            processor.active = True
            entry[1] = True
            return True
    except Exception:  # telemetry must never fail the app
        return False


def _enter_run(card_ref: str, run_id: str) -> None:
    """Push one Run scope; called by ``Run.__enter__``. Never raises.

    Stamps the already-active recording span unless it already carries
    ``wyrd.card_ref``, so a nested scope never overwrites an outer correlation.
    """
    if _otel_context is None:
        return
    try:
        install_run_correlation()
        stack = _otel_context.get_value(_SCOPE_KEY) or ()
        _otel_context.attach(_otel_context.set_value(_SCOPE_KEY, (*stack, (card_ref, run_id))))
        span = _otel_trace.get_current_span()
        if span.is_recording() and not _carries_card_ref(span):
            span.set_attribute(_CARD_REF, card_ref)
            span.set_attribute(_RUN_ID, run_id)
    except Exception:  # telemetry must never fail the app
        pass


def _carries_card_ref(span: Any) -> bool:
    """Whether ``span``'s readable attributes already hold ``wyrd.card_ref``."""
    try:
        return _CARD_REF in span.attributes
    except Exception:  # unreadable attributes: stamp as usual
        return False


def _exit_run(card_ref: str, run_id: str) -> None:
    """Pop this view's Run scope; called by ``Run.__exit__``. Never raises.

    Pops only when the innermost scope is exactly ``(card_ref, run_id)``; a
    mismatched top, empty stack, or failing context call changes nothing.
    """
    if _otel_context is None:
        return
    try:
        stack = _otel_context.get_value(_SCOPE_KEY)
        if stack and stack[-1] == (card_ref, run_id):
            _otel_context.attach(_otel_context.set_value(_SCOPE_KEY, stack[:-1]))
    except Exception:  # telemetry must never fail the app
        pass
