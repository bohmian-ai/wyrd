"""Stock OpenTelemetry exporters, configured only by ``OTEL_EXPORTER_OTLP_*``, land in Bifrost.

A deployment points the exporter at the server's gRPC address and authenticates
it with the ``x-wyrd-api-key`` header; the signal then reads back through
``Bifrost.sql`` as typed rows.
"""

import logging
import os
from math import inf

import pytest
from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.trace import Status, StatusCode
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.testing import WyrdTestServer


class SpanRow(BaseModel):
    """The ``vala.traces.spans`` columns the trace story reads."""

    name: str
    trace_id: bytes
    span_id: bytes
    parent_span_id: bytes | None
    status_code: int
    status_message: str | None
    gen_ai_request_model: str | None
    gen_ai_usage_input_tokens: int | None


class LogRow(BaseModel):
    """The ``vala.logs.records`` columns the log story reads."""

    severity_text: str | None
    severity_number: int | None
    trace_id: bytes | None
    span_id: bytes | None


class MetricRow(BaseModel):
    """The ``vala.metrics.points`` columns the metric story reads."""

    metric_name: str
    metric_type: str
    int_value: int | None
    double_value: float | None
    histogram_count: int | None
    histogram_sum: float | None


@pytest.fixture
def otlp_environment(wyrd_server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch) -> None:
    """Point every stock OTLP exporter at the deployment's Wyrd gRPC address and key."""
    monkeypatch.setenv("OTEL_EXPORTER_OTLP_ENDPOINT", os.environ["WYRD_GRPC_URL"])
    monkeypatch.setenv("OTEL_EXPORTER_OTLP_INSECURE", "true")
    monkeypatch.setenv("OTEL_EXPORTER_OTLP_HEADERS", f"x-wyrd-api-key={os.environ['WYRD_API_KEY']}")


@pytest.mark.integration
@pytest.mark.usefixtures("otlp_environment")
def test_stock_exporter_span_reads_back_through_bifrost(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    traces = TracerProvider()
    traces.add_span_processor(BatchSpanProcessor(OTLPSpanExporter()))
    tracer = traces.get_tracer("wyrd.tests.otel.trace", "1.0.0")
    with tracer.start_as_current_span("answer") as parent:
        parent.set_attribute("gen_ai.request.model", "claude-opus-5")
        parent.set_attribute("gen_ai.usage.input_tokens", 4096)
        parent.set_status(Status(StatusCode.ERROR, "refused"))
        with tracer.start_as_current_span("retrieve"):
            pass
    traces.shutdown()
    wyrd_server.flush_bifrost()

    rows = bifrost.sql(
        "SELECT name, trace_id, span_id, parent_span_id, status_code, status_message, "
        "gen_ai_request_model, gen_ai_usage_input_tokens FROM vala.traces.spans "
        "WHERE scope_name = 'wyrd.tests.otel.trace' ORDER BY name",
        model=SpanRow,
    )
    answer, retrieve = rows
    assert (answer.name, retrieve.name) == ("answer", "retrieve")
    assert retrieve.trace_id == answer.trace_id
    assert retrieve.parent_span_id == answer.span_id
    assert (answer.status_code, answer.status_message) == (2, "refused")
    assert (answer.gen_ai_request_model, answer.gen_ai_usage_input_tokens) == (
        "claude-opus-5",
        4096,
    )


@pytest.mark.integration
@pytest.mark.usefixtures("otlp_environment")
def test_stdlib_log_record_reads_back_through_bifrost(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    logs = LoggerProvider()
    logs.add_log_record_processor(BatchLogRecordProcessor(OTLPLogExporter()))
    logger = logging.getLogger("wyrd.tests.otel.log")
    logger.propagate = False
    logger.addHandler(LoggingHandler(logger_provider=logs))
    with TracerProvider().get_tracer("wyrd.tests.otel.log").start_as_current_span("ship") as span:
        logger.warning("order delayed")
    logs.shutdown()
    wyrd_server.flush_bifrost()

    [row] = bifrost.sql(
        "SELECT severity_text, severity_number, trace_id, span_id FROM vala.logs.records "
        "WHERE scope_name = 'wyrd.tests.otel.log'",
        model=LogRow,
    )
    context = span.get_span_context()
    assert (row.severity_text, row.severity_number) == ("WARN", 13)
    assert row.trace_id == context.trace_id.to_bytes(16, "big")
    assert row.span_id == context.span_id.to_bytes(8, "big")


@pytest.mark.integration
@pytest.mark.usefixtures("otlp_environment")
def test_stock_metric_points_read_back_through_bifrost(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    reader = PeriodicExportingMetricReader(OTLPMetricExporter(), export_interval_millis=inf)
    metrics = MeterProvider(metric_readers=[reader])
    meter = metrics.get_meter("wyrd.tests.otel.metric", "1.0.0")
    meter.create_counter("orders.created").add(7)
    meter.create_gauge("queue.depth").set(3.5)
    meter.create_histogram("request.duration", unit="ms").record(12.5)
    assert metrics.force_flush()
    metrics.shutdown()
    wyrd_server.flush_bifrost()

    rows = bifrost.sql(
        "SELECT metric_name, metric_type, int_value, double_value, histogram_count, "
        "histogram_sum FROM vala.metrics.points "
        "WHERE scope_name = 'wyrd.tests.otel.metric' ORDER BY metric_name",
        model=MetricRow,
    )
    assert rows == [
        MetricRow(
            metric_name="orders.created",
            metric_type="sum",
            int_value=7,
            double_value=None,
            histogram_count=None,
            histogram_sum=None,
        ),
        MetricRow(
            metric_name="queue.depth",
            metric_type="gauge",
            int_value=None,
            double_value=3.5,
            histogram_count=None,
            histogram_sum=None,
        ),
        MetricRow(
            metric_name="request.duration",
            metric_type="histogram",
            int_value=None,
            double_value=None,
            histogram_count=1,
            histogram_sum=12.5,
        ),
    ]
