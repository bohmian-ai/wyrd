"""OpenTelemetry integration: stock OTLP/HTTP exporters and Run span correlation.

``span_exporter``, ``log_exporter``, and ``metric_exporter`` build the stock
OTLP/HTTP protobuf exporters for one ``WyrdClient``: every export asks the
client for its current access token and sends it as ``x-wyrd-access-token``,
so a long-lived exporter keeps working after any one token expires. They need
the ``otel`` extra (``pip install 'wyrd[otel]'``).

``Run.__enter__``/``__exit__`` delegate here so spans created inside
``with state.run(...)`` carry the record-level ``wyrd.card_uid`` and
``wyrd.run_id`` attributes Bifrost extracts. Everything in that path is
optional and fail-open: without ``opentelemetry-api`` it is a no-op.

``WyrdState.start_telemetry`` delegates to ``_start_telemetry``, which installs
one global SDK tracer provider exporting through ``span_exporter``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, cast

if TYPE_CHECKING:
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


_CARD_UID = "wyrd.card_uid"
_RUN_ID = "wyrd.run_id"

# The Run scope stack lives entirely in this one context value: a tuple of
# ``(card_uid, run_id)`` pairs, innermost last. Entry and exit each attach a new
# value and never detach, so no token or per-scope state exists outside it.
_SCOPE_KEY: Any = _otel_context.create_key("wyrd.run_scope") if _OTEL_AVAILABLE else None

# Private marker set on a provider object once the Wyrd processor is offered,
# so later Run entries skip it. Best effort: a concurrent first entry may add a
# second processor, which is harmless because the processor is stateless and
# setting the same two attributes again is idempotent.
_MARKER = "_wyrd_run_correlation"


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
            stack = _scope_stack(parent_context)
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

    ``provider`` defaults to the global tracer provider. The provider object is
    marked before registration so repeated calls skip it. Returns ``True`` when
    the provider is already marked or accepted the processor, ``False`` when
    OpenTelemetry is absent, the provider lacks ``add_span_processor``, cannot
    be marked, or raised while registering. Never raises. Pass a framework's
    private provider once; the global provider is installed on every ``Run``
    entry.
    """
    if not _OTEL_AVAILABLE:
        return False
    try:
        if provider is None:
            provider = _otel_trace.get_tracer_provider()
        if getattr(provider, _MARKER, False):
            return True
        # The API provider has no ``add_span_processor``; only an SDK provider does.
        add = getattr(provider, "add_span_processor", None)
        if add is None:
            return False
        setattr(provider, _MARKER, True)
        add(_RunCorrelationProcessor())
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
    try:
        stack = _scope_stack()
        if stack and stack[-1] == (card_uid, run_id):
            _otel_context.attach(_otel_context.set_value(_SCOPE_KEY, stack[:-1]))
    except Exception:  # telemetry must never fail the app
        pass


_EXPORTER_EXTRA = (
    "Wyrd OTLP exporters need the otel extra: pip install 'wyrd[otel]' "
    "(opentelemetry-sdk, opentelemetry-exporter-otlp-proto-http)"
)


def _session(client: WyrdClient) -> Any:
    """A ``requests`` session that authenticates every request as ``client``."""
    try:
        import requests
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing

    def authenticate(request: Any) -> Any:
        request.headers["x-wyrd-access-token"] = f"Bearer {client.access_token()}"
        return request

    session = requests.Session()
    session.auth = authenticate
    return session


def _endpoint(client: WyrdClient, signal: str) -> str:
    """The signal-specific OTLP/HTTP URL on ``client``'s server."""
    return f"{client.server_url.rstrip('/')}/v1/{signal}"


def _start_telemetry(client: WyrdClient) -> Any:
    """Install the global tracer provider exporting spans as ``client``.

    Returns the installed provider, which the calling state keeps so a repeated
    start is idempotent and its shutdown flushes it. Returns ``None`` when a
    global provider is already installed; the caller refuses with the catalog
    error.

    Raises:
        ImportError: naming the ``otel`` extra when the SDK or exporter is absent.
    """
    try:
        from opentelemetry.sdk.trace import TracerProvider
        from opentelemetry.sdk.trace.export import BatchSpanProcessor
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    if not isinstance(_otel_trace.get_tracer_provider(), _otel_trace.ProxyTracerProvider):
        return None
    provider = TracerProvider()
    provider.add_span_processor(BatchSpanProcessor(span_exporter(client)))
    install_run_correlation(provider)
    _otel_trace.set_tracer_provider(provider)
    return provider


def span_exporter(client: WyrdClient) -> OTLPSpanExporter:
    """Build a stock OTLP/HTTP span exporter that authenticates as ``client``.

    Args:
        client: The client whose access token authenticates every export.

    Raises:
        ImportError: naming the ``otel`` extra when the exporter is not installed.
    """
    try:
        from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    return OTLPSpanExporter(endpoint=_endpoint(client, "traces"), session=_session(client))


def log_exporter(client: WyrdClient) -> OTLPLogExporter:
    """Build a stock OTLP/HTTP log exporter that authenticates as ``client``.

    Args:
        client: The client whose access token authenticates every export.

    Raises:
        ImportError: naming the ``otel`` extra when the exporter is not installed.
    """
    try:
        from opentelemetry.exporter.otlp.proto.http._log_exporter import OTLPLogExporter
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    return OTLPLogExporter(endpoint=_endpoint(client, "logs"), session=_session(client))


def metric_exporter(client: WyrdClient) -> OTLPMetricExporter:
    """Build a stock OTLP/HTTP metric exporter that authenticates as ``client``.

    Args:
        client: The client whose access token authenticates every export.

    Raises:
        ImportError: naming the ``otel`` extra when the exporter is not installed.
    """
    try:
        from opentelemetry.exporter.otlp.proto.http.metric_exporter import OTLPMetricExporter
    except ImportError as missing:
        raise ImportError(_EXPORTER_EXTRA) from missing
    return OTLPMetricExporter(endpoint=_endpoint(client, "metrics"), session=_session(client))
