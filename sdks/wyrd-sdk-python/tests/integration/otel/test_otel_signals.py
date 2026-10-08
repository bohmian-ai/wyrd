"""Stock OpenTelemetry log and metric exporters, configured only by ``OTEL_EXPORTER_OTLP_*``, land in Bifrost.

These are Python's own OTel signal integrations; the shared span story lives in
``test_otel_export.py``.
"""

import logging
from math import inf

import pytest
from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.trace import TracerProvider
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration


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
    # Publish the exported signals so the query reads them.
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
