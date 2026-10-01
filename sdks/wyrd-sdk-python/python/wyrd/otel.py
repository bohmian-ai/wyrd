"""OTel observer for workflow-scoped Wyrd instrumentation, and Run span correlation.

``Run.__enter__``/``__exit__`` delegate here so spans created inside
``with state.run(...)`` carry the record-level ``wyrd.card_ref`` and
``wyrd.run_id`` attributes Bifrost extracts. Everything in that path is
optional and fail-open: without ``opentelemetry-api`` it is a no-op.
"""

from __future__ import annotations

import warnings
import weakref
from contextvars import ContextVar
from threading import Lock
from typing import Any

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


_CARD_REF = "wyrd.card_ref"
_RUN_ID = "wyrd.run_id"

# One execution-local stack of OTel context tokens (``None`` when an entry
# attached nothing), so nested and concurrent scopes of one immutable Run each
# detach exactly the token their own entry installed.
_scope_tokens: ContextVar[tuple[object | None, ...]] = ContextVar(
    "wyrd_run_scope_tokens", default=()
)
_scope_key: Any = None
_registered: weakref.WeakSet[Any] = weakref.WeakSet()
_registered_lock = Lock()


def _key() -> Any:
    """Return the private OTel context key holding ``(card_ref, run_id)``."""
    global _scope_key
    if _scope_key is None:
        from opentelemetry.context import create_key

        _scope_key = create_key("wyrd.run_scope")
    return _scope_key


class _RunCorrelationProcessor:
    """Span processor copying the scoped Run correlation onto every started span.

    Duck-typed rather than subclassing the SDK ``SpanProcessor`` so the OTel SDK
    stays optional. Stateless; every hook swallows its own failures.
    """

    def on_start(self, span: Any, parent_context: Any = None) -> None:
        try:
            from opentelemetry.context import get_value

            scope = get_value(_key(), parent_context)
            if scope is not None:
                span.set_attribute(_CARD_REF, scope[0])
                span.set_attribute(_RUN_ID, scope[1])
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


def install_run_correlation(provider: Any = None) -> bool:
    """Register the Wyrd Run-correlation span processor on ``provider``.

    ``provider`` defaults to the global tracer provider. Registration is
    thread-safe and idempotent per provider. Returns ``True`` when the provider
    holds the processor, ``False`` when OpenTelemetry is absent or the provider
    cannot accept span processors. Never raises. Pass a framework's private
    provider once; the global provider is installed on every ``Run`` entry.
    """
    try:
        if provider is None:
            from opentelemetry import trace

            provider = trace.get_tracer_provider()
        add = getattr(provider, "add_span_processor", None)
        if add is None:
            return False
        with _registered_lock:
            if provider in _registered:
                return True
            # Track first: a provider that cannot be weakly referenced fails
            # here, before it could receive a processor on every entry.
            _registered.add(provider)
            try:
                add(_RunCorrelationProcessor())
            except Exception:
                _registered.discard(provider)
                raise
        return True
    except Exception:  # telemetry must never fail the app
        return False


def _enter_run(card_ref: str, run_id: str) -> None:
    """Attach one Run scope; called by ``Run.__enter__``. Never raises.

    Always pushes exactly one stack entry so ``_exit_run`` stays paired.
    """
    token = None
    try:
        from opentelemetry import context, trace

        install_run_correlation()
        token = context.attach(context.set_value(_key(), (card_ref, run_id)))
        span = trace.get_current_span()
        if span.is_recording():
            span.set_attribute(_CARD_REF, card_ref)
            span.set_attribute(_RUN_ID, run_id)
    except Exception:  # telemetry must never fail the app
        pass
    _scope_tokens.set((*_scope_tokens.get(), token))


def _exit_run() -> None:
    """Detach the innermost scope token of this execution context. Never raises."""
    tokens = _scope_tokens.get()
    if not tokens:
        return
    _scope_tokens.set(tokens[:-1])
    if tokens[-1] is None:
        return
    try:
        from opentelemetry import context

        context.detach(tokens[-1])
    except Exception:  # telemetry must never fail the app
        pass
