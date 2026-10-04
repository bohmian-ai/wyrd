"""OTel observer for workflow-scoped Wyrd instrumentation, and Run span correlation.

``Run.__enter__``/``__exit__`` delegate here so spans created inside
``with state.run(...)`` carry the record-level ``wyrd.card_ref`` and
``wyrd.run_id`` attributes Bifrost extracts. Everything in that path is
optional and fail-open: without ``opentelemetry-api`` it is a no-op.
"""

from __future__ import annotations

import warnings
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
    """Observer that turns Wyrd lifecycle events into OpenTelemetry spans.

    Add ``OtelObserver()`` to ``Workflow(observers=[...])`` to emit a
    ``wyrd.workflow.run`` span per workflow run, a child ``wyrd.agent.run`` span
    per Agent run, and ``wyrd.model.call`` and ``wyrd.tool.call`` spans under
    it. Safe for concurrent workflow steps.
    """

    def __init__(self, tracer=None) -> None:
        """Create an OtelObserver.

        Args:
            tracer: an OpenTelemetry ``Tracer`` to create spans with. Omitted,
                ``opentelemetry.trace.get_tracer("wyrd")`` is used, and a
                warning is emitted if no tracer provider is configured yet,
                because those spans would be discarded.

        Raises:
            ImportError: when ``opentelemetry-api`` is not installed.
        """
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
        """As ``Observer.on_agent_start``; starts a ``wyrd.agent.run/<agent_id>`` span.

        The span is a child of the workflow span named by ``parent_run_id`` and
        carries ``wyrd.run_id`` and ``wyrd.agent.id``.
        """

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
        """As ``Observer.on_model_call``; starts a ``wyrd.model.call/<provider>`` span.

        A ``CLIENT`` span under the agent span carrying ``wyrd.run_id``,
        ``wyrd.agent.id``, ``wyrd.iteration``, ``gen_ai.system``, and
        ``gen_ai.request.model``.
        """

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
        """As ``Observer.on_model_result``; ends the model span.

        Sets ``gen_ai.response.finish_reason`` first.
        """
        span = self._pop_span(f"{run_id}.model.{iteration}")
        if span is not None:
            span.set_attribute("gen_ai.response.finish_reason", finish_reason)
            span.end()

    def on_tool_call(self, run_id, agent_id, iteration, call_id, tool_name) -> None:
        """As ``Observer.on_tool_call``; starts a ``wyrd.tool.call/<tool_name>`` span.

        The span sits under the agent span and carries ``wyrd.run_id``,
        ``wyrd.agent.id``, ``wyrd.iteration``, ``wyrd.call_id``, and
        ``tool.name``.
        """

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
        """As ``Observer.on_tool_result``; ends the tool span, as ``ERROR`` unless ``ok``."""

        from opentelemetry.trace import StatusCode

        span = self._pop_span(f"{run_id}.tool.{call_id}")
        if span is not None:
            if not ok:
                span.set_status(StatusCode.ERROR, "tool call failed")
            span.end()

    def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms) -> None:
        """As ``Observer.on_agent_finish``; ends the agent span.

        Sets ``wyrd.finish_reason``, ``wyrd.iterations``, and
        ``wyrd.duration_ms`` first.
        """

        span = self._pop_span(run_id)
        if span is not None:
            span.set_attribute("wyrd.finish_reason", finish_reason)
            span.set_attribute("wyrd.iterations", iterations)
            span.set_attribute("wyrd.duration_ms", duration_ms)
            span.end()

    def on_agent_error(self, run_id, agent_id, code, message) -> None:
        """As ``Observer.on_agent_error``; ends the agent span with ``ERROR`` status.

        The status description is ``message`` and ``error.code`` is ``code``.
        """

        from opentelemetry.trace import StatusCode

        span = self._pop_span(run_id)
        if span is not None:
            span.set_status(StatusCode.ERROR, message)
            span.set_attribute("error.code", code)
            span.end()

    def on_workflow_start(self, run_id, workflow_id, step_count) -> None:
        """As ``Observer.on_workflow_start``; starts the workflow span.

        A root span named ``wyrd.workflow.run/<workflow_id>`` carrying
        ``wyrd.run_id``, ``wyrd.workflow.id``, and ``wyrd.workflow.step_count``.
        """

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
        """As ``Observer.on_workflow_finish``; ends the workflow span.

        Sets ``wyrd.duration_ms`` first.
        """
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

# Private marker set on a provider object once the Wyrd processor is offered,
# so later Run entries skip it. Best effort: a concurrent first entry may add a
# second processor, which is harmless because the processor is stateless and
# setting the same two attributes again is idempotent.
_MARKER = "_wyrd_run_correlation"


class _RunCorrelationProcessor:
    """Stateless span processor copying the innermost Run scope onto started spans.

    Duck-typed rather than subclassing the SDK ``SpanProcessor`` so the OTel SDK
    stays optional. Every hook swallows its own failures.
    """

    def on_start(self, span: Any, parent_context: Any = None) -> None:
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


def install_run_correlation(provider: Any = None) -> bool:
    """Register the Wyrd Run-correlation span processor on ``provider``.

    ``provider`` defaults to the global tracer provider. The provider object is
    marked before registration so repeated calls skip it. Returns ``True`` when
    the provider is already marked or accepted the processor, ``False`` when
    OpenTelemetry is absent, the provider lacks ``add_span_processor``, cannot
    be marked, or raised while registering. Never raises. Pass a framework's
    private provider once; the global provider is installed on every ``Run``
    entry.
    """
    if _otel_trace is None:
        return False
    try:
        if provider is None:
            provider = _otel_trace.get_tracer_provider()
        if getattr(provider, _MARKER, False):
            return True
        add = provider.add_span_processor
        setattr(provider, _MARKER, True)
        add(_RunCorrelationProcessor())
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
