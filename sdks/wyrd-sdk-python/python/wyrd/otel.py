"""OpenTelemetry integration: stock OTLP/HTTP exporters and Run span correlation.

``span_exporter``, ``log_exporter``, and ``metric_exporter`` build the stock
OTLP/HTTP protobuf exporters for one ``WyrdClient``: every export asks the
client for its current access token and sends it as ``x-wyrd-access-token``,
beside the client's workload root, when it has one, as ``x-wyrd-workload-uid``,
so a long-lived exporter keeps working after any one token expires. They need
the ``otel`` extra (``uv add 'wyrd-ai[otel]'``).

``Run.__enter__``/``__exit__`` delegate here so spans created inside
``with state.run(...)`` carry the record-level ``wyrd.card_uid`` and
``wyrd.run_id`` attributes Bifrost extracts. Everything in that path is
optional and fail-open: without ``opentelemetry-api`` it is a no-op.

``WyrdState.start_telemetry`` delegates to ``_start_telemetry``, which installs
one global SDK tracer provider exporting through ``span_exporter``.

``WyrdAgentsTraceProcessor`` is an OpenAI Agents SDK trace processor the
application passes to ``agents.set_trace_processors``; Wyrd never registers it.
"""

from __future__ import annotations

import contextvars
import threading
import time
from concurrent.futures import Future, wait
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any, cast

if TYPE_CHECKING:
    from agents.tracing import Span, Trace
    from opentelemetry.exporter.otlp.proto.http._log_exporter import OTLPLogExporter
    from opentelemetry.exporter.otlp.proto.http.metric_exporter import OTLPMetricExporter
    from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter

    from wyrd.client import WyrdClient

try:
    from opentelemetry import context as _otel_context
    from opentelemetry import trace as _otel_trace
except ImportError:  # optional: run correlation becomes a no-op
    _OTEL_AVAILABLE = False
else:
    _OTEL_AVAILABLE = True

if TYPE_CHECKING:
    from agents.tracing import TracingProcessor as _AgentsTracingProcessor
else:
    try:
        from agents.tracing import TracingProcessor as _AgentsTracingProcessor
    except ImportError:  # optional: the Agents SDK trace processor is then a plain object
        _AgentsTracingProcessor = object


_CARD_UID = "wyrd.card_uid"
_RUN_ID = "wyrd.run_id"

# The Run scope stack is a tuple of ``(card_uid, run_id)`` pairs, innermost
# last, held in this OpenTelemetry context value. Entry and exit each attach a
# new value and never detach, so no token or per-scope state exists outside it.
_SCOPE_KEY: Any = _otel_context.create_key("wyrd.run_scope") if _OTEL_AVAILABLE else None

# The same stack, mirrored in a plain context variable. Server instrumentation
# attaches a context it extracted from request headers, which starts empty and
# so drops ``_SCOPE_KEY``; this variable survives that attach, so spans the
# instrumentation starts inside a Run still find it.
_SCOPE_VAR: contextvars.ContextVar[tuple[tuple[str, str], ...]] = contextvars.ContextVar(
    "wyrd.run_scope", default=()
)

# Private marker set on a provider object once it accepted the Wyrd processor,
# so later Run entries skip it. ``_INSTALL_LOCK`` makes the check-and-add
# atomic, so concurrent first entries register exactly one processor, and a
# failed registration leaves no marker and can be retried.
_MARKER = "_wyrd_run_correlation"
_INSTALL_LOCK = threading.Lock()


def _scope_stack(parent_context: Any = None) -> tuple[tuple[str, str], ...]:
    """Return the Run scope stack stored in ``parent_context``, or ``()``.

    Only this module writes ``_SCOPE_KEY``, always as ``(card_uid, run_id)`` pairs.
    """
    stack = _otel_context.get_value(_SCOPE_KEY, parent_context)
    return cast("tuple[tuple[str, str], ...]", stack) if isinstance(stack, tuple) else ()


class _RunCorrelationProcessor:
    """Stateless span processor copying the innermost Run scope onto started spans.

    Duck-typed rather than subclassing the SDK ``SpanProcessor`` so the OTel SDK
    stays optional. Every hook swallows its own failures.
    """

    def on_start(self, span: Any, parent_context: Any = None) -> None:
        try:
            stack = _scope_stack(parent_context) or _SCOPE_VAR.get()
            if stack:
                card_uid, run_id = stack[-1]
                span.set_attribute(_CARD_UID, card_uid)
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

    ``provider`` defaults to the global tracer provider. Returns ``True`` when
    the provider already holds the processor or accepted it now, and ``False``
    when OpenTelemetry is absent, the provider lacks ``add_span_processor``, or
    it raised while registering; a ``False`` call can be retried. Never raises.
    Install it once on an application-owned provider; the global provider is
    installed on every ``Run`` entry.
    """
    if not _OTEL_AVAILABLE:
        return False
    try:
        if provider is None:
            provider = _otel_trace.get_tracer_provider()
        with _INSTALL_LOCK:
            if getattr(provider, _MARKER, False):
                return True
            # The API provider has no ``add_span_processor``; only an SDK provider does.
            add = getattr(provider, "add_span_processor", None)
            if add is None:
                return False
            setattr(provider, _MARKER, True)
            try:
                add(_RunCorrelationProcessor())
            except Exception:
                delattr(provider, _MARKER)
                raise
            return True
    except Exception:  # telemetry must never fail the app
        return False


def _enter_run(card_uid: str, run_id: str) -> None:
    """Push one Run scope; called by ``Run.__enter__``. Never raises.

    Stamps the already-active recording span unless it already carries
    ``wyrd.card_uid``, so a nested scope never overwrites an outer correlation.
    """
    if not _OTEL_AVAILABLE:
        return
    _SCOPE_VAR.set((*_SCOPE_VAR.get(), (card_uid, run_id)))
    try:
        install_run_correlation()
        stack = _scope_stack()
        _otel_context.attach(_otel_context.set_value(_SCOPE_KEY, (*stack, (card_uid, run_id))))
        span = _otel_trace.get_current_span()
        if span.is_recording() and not _carries_card_uid(span):
            span.set_attribute(_CARD_UID, card_uid)
            span.set_attribute(_RUN_ID, run_id)
    except Exception:  # telemetry must never fail the app
        pass


def _carries_card_uid(span: Any) -> bool:
    """Whether ``span``'s readable attributes already hold ``wyrd.card_uid``."""
    try:
        return _CARD_UID in span.attributes
    except Exception:  # unreadable attributes: stamp as usual
        return False


def _exit_run(card_uid: str, run_id: str) -> None:
    """Pop this view's Run scope; called by ``Run.__exit__``. Never raises.

    Pops only when the innermost scope is exactly ``(card_uid, run_id)``; a
    mismatched top, empty stack, or failing context call changes nothing.
    """
    if not _OTEL_AVAILABLE:
        return
    mirrored = _SCOPE_VAR.get()
    if mirrored and mirrored[-1] == (card_uid, run_id):
        _SCOPE_VAR.set(mirrored[:-1])
    try:
        stack = _scope_stack()
        if stack and stack[-1] == (card_uid, run_id):
            _otel_context.attach(_otel_context.set_value(_SCOPE_KEY, stack[:-1]))
    except Exception:  # telemetry must never fail the app
        pass


_EXPORTER_EXTRA = (
    "Wyrd OTLP exporters need the otel extra: uv add 'wyrd-ai[otel]' "
    "(opentelemetry-sdk, opentelemetry-exporter-otlp-proto-http)"
)


def _session(client: WyrdClient, workload_uid: str | None) -> Any:
    """A ``requests`` session that authenticates every request as ``client``, run as ``workload_uid`` when given."""
    try:
        import requests
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    if workload_uid is not None:
        client = client.for_workload(workload_uid)

    def authenticate(request: Any) -> Any:
        request.headers["x-wyrd-access-token"] = f"Bearer {client.access_token()}"
        if client.workload_uid is not None:
            request.headers["x-wyrd-workload-uid"] = client.workload_uid
        if client.deployment_environment is not None:
            request.headers["x-wyrd-deployment-environment"] = client.deployment_environment
        return request

    session = requests.Session()
    session.auth = authenticate
    return session


def _endpoint(client: WyrdClient, signal: str) -> str:
    """The signal-specific OTLP/HTTP URL on ``client``'s server."""
    return f"{client.server_url.rstrip('/')}/v1/{signal}"


_FLUSH_TIMEOUT_MILLIS = 30000
"""How long a state waits for its managed provider to flush: the SDK's ``force_flush`` default."""


class _ManagedTelemetry:
    """The tracer provider a state installed, flushed within a real bound.

    The stock batch processor ignores ``force_flush``'s timeout and exports
    synchronously, so the flush runs on a daemon thread and the caller waits at
    most ``_FLUSH_TIMEOUT_MILLIS``. A flush that overruns finishes in the
    background. At most one runs per provider: a call that finds an earlier
    flush still running waits for it, then starts its own, because the earlier
    one may have drained the queue before this call's spans ended. Both waits
    share the call's one deadline.
    """

    def __init__(self, provider: Any) -> None:
        self.provider = provider
        self._lock = threading.Lock()
        self._attempt: Future[bool] | None = None
        # How many flushes have started; one numbered after a call's entry covers it.
        self._started = 0

    def force_flush(self) -> bool:
        """Flush the provider, raising ``TimeoutError`` when the bound expires first.

        Any other failure of the provider's own flush is re-raised unchanged.
        """
        deadline = time.monotonic() + _FLUSH_TIMEOUT_MILLIS / 1000
        with self._lock:
            entered = self._started
        while True:
            with self._lock:
                if self._attempt is None or self._attempt.done():
                    self._started += 1
                    self._attempt = Future()
                    threading.Thread(
                        target=self._flush,
                        args=(self._attempt,),
                        name="wyrd-telemetry-flush",
                        daemon=True,
                    ).start()
                attempt, covers = self._attempt, self._started > entered
            remaining = max(0.0, deadline - time.monotonic())
            if covers:
                # Since Python 3.11 a timed-out wait raises the builtin TimeoutError.
                return attempt.result(remaining)
            if not wait([attempt], remaining).done:
                raise TimeoutError("an earlier flush did not finish in time")

    def _flush(self, attempt: Future[bool]) -> None:
        """Run the provider's flush and settle ``attempt`` with its outcome."""
        try:
            attempt.set_result(self.provider.force_flush(_FLUSH_TIMEOUT_MILLIS))
        except BaseException as error:  # settled on the caller's thread
            attempt.set_exception(error)


def _start_telemetry(client: WyrdClient) -> Any:
    """Install the global tracer provider exporting spans as ``client``, and
    the W3C trace-context propagator for outbound context.

    Returns the installed provider wrapped for bounded flushing, which the
    calling state keeps so a repeated start is idempotent and its shutdown
    flushes it. Returns ``None`` when a
    global provider is already installed; the caller refuses with the catalog
    error.

    Raises:
        ImportError: naming the ``otel`` extra when the SDK or exporter is absent.
    """
    try:
        from opentelemetry.propagate import set_global_textmap
        from opentelemetry.sdk.trace import TracerProvider
        from opentelemetry.sdk.trace.export import BatchSpanProcessor
        from opentelemetry.trace.propagation.tracecontext import TraceContextTextMapPropagator
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    if not isinstance(_otel_trace.get_tracer_provider(), _otel_trace.ProxyTracerProvider):
        return None
    provider = TracerProvider()
    provider.add_span_processor(BatchSpanProcessor(span_exporter(client)))
    install_run_correlation(provider)
    _otel_trace.set_tracer_provider(provider)
    if _otel_trace.get_tracer_provider() is not provider:
        # Another provider won the once-only global install: release this one's
        # export thread and leave the winner untouched.
        provider.shutdown()
        return None
    set_global_textmap(TraceContextTextMapPropagator())
    return _ManagedTelemetry(provider)


def span_exporter(client: WyrdClient, *, workload_uid: str | None = None) -> OTLPSpanExporter:
    """Build a stock OTLP/HTTP span exporter that authenticates as ``client``.

    Args:
        client: The client whose access token authenticates every export.
        workload_uid: Exact Service Card UID the exports run as;
            omitted, the client's own workload root, if any.

    Raises:
        ImportError: naming the ``otel`` extra when the exporter is not installed.
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``workload_uid`` is not a UUIDv7.
    """
    try:
        from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    return OTLPSpanExporter(
        endpoint=_endpoint(client, "traces"), session=_session(client, workload_uid)
    )


def log_exporter(client: WyrdClient, *, workload_uid: str | None = None) -> OTLPLogExporter:
    """Build a stock OTLP/HTTP log exporter that authenticates as ``client``.

    Args:
        client: The client whose access token authenticates every export.
        workload_uid: Exact Service Card UID the exports run as;
            omitted, the client's own workload root, if any.

    Raises:
        ImportError: naming the ``otel`` extra when the exporter is not installed.
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``workload_uid`` is not a UUIDv7.
    """
    try:
        from opentelemetry.exporter.otlp.proto.http._log_exporter import OTLPLogExporter
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    return OTLPLogExporter(
        endpoint=_endpoint(client, "logs"), session=_session(client, workload_uid)
    )


def metric_exporter(client: WyrdClient, *, workload_uid: str | None = None) -> OTLPMetricExporter:
    """Build a stock OTLP/HTTP metric exporter that authenticates as ``client``.

    Args:
        client: The client whose access token authenticates every export.
        workload_uid: Exact Service Card UID the exports run as;
            omitted, the client's own workload root, if any.

    Raises:
        ImportError: naming the ``otel`` extra when the exporter is not installed.
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``workload_uid`` is not a UUIDv7.
    """
    try:
        from opentelemetry.exporter.otlp.proto.http.metric_exporter import OTLPMetricExporter
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    return OTLPMetricExporter(
        endpoint=_endpoint(client, "metrics"), session=_session(client, workload_uid)
    )


class WyrdAgentsTraceProcessor(_AgentsTracingProcessor):
    """OpenAI Agents SDK trace processor recording each trace and span as an OTel span.

    Pass it to the Agents SDK yourself: ``add_trace_processor`` keeps the SDK's
    default OpenAI exporter alongside it, ``set_trace_processors`` replaces it.
    Wyrd never registers it. Spans go to the global tracer provider, so they
    export wherever the application's OpenTelemetry pipeline does.

    An SDK trace becomes a span started in the current context, so it nests
    under any enclosing OpenTelemetry span and carries the Run active when it
    started; each SDK span becomes a child of its SDK parent, else its trace.
    Framework timestamps are kept when valid. Agent, tool, and model spans
    follow the GenAI conventions for operation, agent and tool name, model, and
    token usage, read when the span ends. Prompts, completions, tool arguments
    and results, and error messages are never recorded: an SDK error sets
    ERROR status and ``error.type`` only.

    Callbacks are thread-safe. A repeated start is ignored, as are ends with no
    start and every callback after ``shutdown``, which ends unfinished spans
    with ERROR. ``shutdown`` and ``force_flush`` never touch the provider.
    Without ``opentelemetry-api`` installed it records nothing.
    """

    def __init__(self) -> None:
        # Each open SDK trace or span's OTel span, by SDK id, guarded by ``_lock``.
        self._open: dict[str, Any] = {}
        self._lock = threading.Lock()
        self._closed = False

    def on_trace_start(self, trace: Trace) -> None:
        """Start the span for ``trace`` in the current context."""
        self._start(trace.trace_id, (), trace.name, None)

    def on_trace_end(self, trace: Trace) -> None:
        """End the span for ``trace``."""
        self._end(trace.trace_id, None, None)

    def on_span_start(self, span: Span[Any]) -> None:
        """Start a child of ``span``'s SDK parent, else its trace, else the current context."""
        data = span.span_data
        parents = (span.parent_id, span.trace_id) if span.parent_id else (span.trace_id,)
        self._start(span.span_id, parents, _span_name(data), span.started_at)

    def on_span_end(self, span: Span[Any]) -> None:
        """Record ``span``'s GenAI attributes and error status, then end it."""
        self._end(span.span_id, span, span.ended_at)

    def shutdown(self) -> None:
        """End every unfinished span with ERROR and ignore later callbacks."""
        with self._lock:
            self._closed = True
            unfinished = list(self._open.values())
            self._open.clear()
        for otel_span in unfinished:
            otel_span.set_status(_otel_trace.Status(_otel_trace.StatusCode.ERROR, _INTERRUPTED))
            otel_span.end()

    def force_flush(self) -> None:
        """Nothing to flush: the application's tracer provider owns export."""

    def _start(
        self, sdk_id: str, parents: tuple[str, ...], name: str, started_at: str | None
    ) -> None:
        """Start the span for ``sdk_id`` under the first open SDK id in ``parents``.

        The parent span is placed in the current context, so the Run scope and
        other context values are those active at this start. With no open
        parent the span nests under the current context's span.
        """
        if not _OTEL_AVAILABLE:
            return
        with self._lock:
            if self._closed or sdk_id in self._open:
                return
            parent_span = next((self._open[p] for p in parents if p in self._open), None)
            parent_context = _otel_context.get_current()
            if parent_span is not None:
                parent_context = _otel_trace.set_span_in_context(parent_span, parent_context)
            self._open[sdk_id] = _otel_trace.get_tracer("wyrd.agents").start_span(
                name, parent_context, start_time=_nanos(started_at)
            )

    def _end(self, sdk_id: str, span: Span[Any] | None, ended_at: str | None) -> None:
        """Record ``span``'s final data on, and end, the open span for ``sdk_id``."""
        with self._lock:
            otel_span = self._open.pop(sdk_id, None)
        if otel_span is None:
            return
        if span is not None:
            name, attributes = _gen_ai(span.span_data)
            if name is not None:
                otel_span.update_name(name)
            otel_span.set_attributes(attributes)
            if span.error is not None:
                otel_span.set_attribute("error.type", "_OTHER")
                otel_span.set_status(_otel_trace.Status(_otel_trace.StatusCode.ERROR))
        otel_span.end(end_time=_nanos(ended_at))


_INTERRUPTED = "span did not finish before the trace processor shut down"


def _nanos(timestamp: str | None) -> int | None:
    """An SDK ISO-8601 timestamp in Unix nanoseconds, or ``None`` for callback time."""
    if timestamp is None:
        return None
    try:
        moment = datetime.fromisoformat(timestamp)
    except (TypeError, ValueError):
        return None
    if moment.tzinfo is None:
        moment = moment.replace(tzinfo=timezone.utc)
    return int(moment.timestamp() * 1_000_000) * 1_000


def _span_name(data: Any) -> str:
    """The GenAI start-time span name for agent and tool spans, else the SDK name or type."""
    name = getattr(data, "name", None)
    if data.type == "agent" and name:
        return f"invoke_agent {name}"
    if data.type == "function" and name:
        return f"execute_tool {name}"
    return name or data.type


def _gen_ai(data: Any) -> tuple[str | None, dict[str, Any]]:
    """The final span name, if it changes, and the GenAI attributes ``data`` supplies.

    Only identity, model, and token counts are read; content fields never are.
    """
    if data.type == "agent":
        return None, {"gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": data.name}
    if data.type == "function":
        return None, {"gen_ai.operation.name": "execute_tool", "gen_ai.tool.name": data.name}
    if data.type == "generation":
        model, usage = data.model, data.usage or {}
        input_tokens, output_tokens = usage.get("input_tokens"), usage.get("output_tokens")
    elif data.type == "response":
        # The SDK supplies usage even when content capture withholds the response.
        response = data.response
        model = getattr(response, "model", None)
        usage, full = data.usage or {}, getattr(response, "usage", None)
        input_tokens = usage.get("input_tokens", getattr(full, "input_tokens", None))
        output_tokens = usage.get("output_tokens", getattr(full, "output_tokens", None))
    else:
        return None, {}
    attributes = {
        "gen_ai.operation.name": "chat",
        "gen_ai.request.model": model,
        "gen_ai.usage.input_tokens": input_tokens,
        "gen_ai.usage.output_tokens": output_tokens,
    }
    return (
        f"chat {model}" if model else "chat",
        {key: value for key, value in attributes.items() if value is not None},
    )
