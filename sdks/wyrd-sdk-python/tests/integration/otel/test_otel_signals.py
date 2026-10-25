"""Stock OpenTelemetry log and metric exporters, configured only by ``OTEL_EXPORTER_OTLP_*``, land in Bifrost.

These are Python's own OTel signal integrations; the shared span story lives in
``test_otel_export.py``.
"""

import logging
from math import inf

import pytest
from opentelemetry.exporter.otlp.proto.grpc._log_exporter import OTLPLogExporter
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk._logs.export import BatchLogRecordProcessor
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.testing import WyrdTestServer

from ..support import declare

pytestmark = pytest.mark.integration


class LogRow(BaseModel):
    """The ``bifrost.logs.records`` columns the log story reads."""

    severity_text: str | None
    severity_number: int | None
    trace_id: bytes | None
    span_id: bytes | None


class MetricRow(BaseModel):
    """The ``bifrost.metrics.points`` columns the metric story reads."""

    metric_name: str
    metric_type: str
    int_value: int | None
    double_value: float | None
    histogram_count: int | None
    histogram_sum: float | None


class DimensionRow(BaseModel):
    """The shared dimensions and workload every signal row carries."""

    service_name: str | None
    service_version: str | None
    deployment_environment: str | None
    subject_kind: str | None
    subject_name: str | None
    subject_version: str | None
    workload_card_uid: str | None


class ExceptionRow(BaseModel):
    """The exception columns a span or log record promotes."""

    exception_type: str | None
    exception_message: str | None
    is_exception: bool


class SpanPromotionRow(ExceptionRow):
    """The URL and exception columns a span promotes from its attributes and events."""

    http_request_method: str | None
    url_full: str | None
    url_path: str | None
    url_query: str | None


DIMENSIONS = (
    "service_name, service_version, deployment_environment, subject_kind, subject_name, "
    "subject_version, workload_card_uid"
)


# The application's own standard resource, with no Wyrd state behind it.
OWNED = Resource.create(
    {
        "service.name": "checkout",
        "service.version": "2.1.0",
        "deployment.environment.name": "staging",
    }
)


@pytest.mark.usefixtures("otlp_environment")
def test_signal_promotions_and_attribution_read_back_through_bifrost(
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
    cards: Cards,
    observed_key: str,
) -> None:
    model = declare(cards, "cards/observe_a_run/observed-model.yaml")["observed-model"]
    root = declare(cards, "cards/observe_a_run/observed-service.yaml")["observed-service"]
    subject = {"wyrd.card_uid": str(model.uid)}

    spans = TracerProvider(resource=OWNED)
    spans.add_span_processor(BatchSpanProcessor(OTLPSpanExporter()))
    with spans.get_tracer("wyrd.tests.otel.owned").start_as_current_span(
        "checkout",
        attributes={
            **subject,
            "http.request.method": "GET",
            "url.full": "https://shop.test/cart?id=7",
            "url.path": "/cart",
            "url.query": "id=7",
        },
    ) as span:
        span.record_exception(ValueError("sold out"))
    logs = LoggerProvider(resource=OWNED)
    logs.add_log_record_processor(BatchLogRecordProcessor(OTLPLogExporter()))
    logger = logging.getLogger("wyrd.tests.otel.owned")
    logger.propagate = False
    logger.addHandler(LoggingHandler(logger_provider=logs))
    try:
        raise ValueError("sold out")
    except ValueError:
        logger.exception("checkout failed", extra=subject)
    reader = PeriodicExportingMetricReader(OTLPMetricExporter(), export_interval_millis=inf)
    metrics = MeterProvider(resource=OWNED, metric_readers=[reader])
    metrics.get_meter("wyrd.tests.otel.owned").create_counter("orders.failed").add(1, subject)
    rooted = TracerProvider(resource=OWNED)
    rooted.add_span_processor(
        BatchSpanProcessor(
            OTLPSpanExporter(
                headers={"x-wyrd-api-key": observed_key, "x-wyrd-workload-uid": str(root.uid)}
            )
        )
    )
    rooted.get_tracer("wyrd.tests.otel.rooted").start_span("checkout", attributes=subject).end()
    assert metrics.force_flush()
    for provider in (spans, logs, metrics, rooted):
        provider.shutdown()
    wyrd_server.flush_bifrost()

    observed = DimensionRow(
        service_name="checkout",
        service_version="2.1.0",
        deployment_environment="staging",
        subject_kind="Model",
        subject_name="observed-model",
        subject_version="1.0.0",
        workload_card_uid=None,
    )
    for table in ("bifrost.traces.spans", "bifrost.logs.records", "bifrost.metrics.points"):
        assert bifrost.sql(
            f"SELECT {DIMENSIONS} FROM {table} WHERE scope_name = 'wyrd.tests.otel.owned'",
            model=DimensionRow,
        ) == [observed], table
    assert bifrost.sql(
        f"SELECT {DIMENSIONS} FROM bifrost.traces.spans WHERE scope_name = 'wyrd.tests.otel.rooted'",
        model=DimensionRow,
    ) == [
        observed.model_copy(
            update={
                "service_name": "observed-service",
                "service_version": "1.0.0",
                "workload_card_uid": str(root.uid),
            }
        )
    ]
    assert bifrost.sql(
        "SELECT http_request_method, url_full, url_path, url_query, exception_type, "
        "exception_message, is_exception FROM bifrost.traces.spans "
        "WHERE scope_name = 'wyrd.tests.otel.owned'",
        model=SpanPromotionRow,
    ) == [
        SpanPromotionRow(
            http_request_method="GET",
            url_full="https://shop.test/cart?id=7",
            url_path="/cart",
            url_query="id=7",
            exception_type="ValueError",
            exception_message="sold out",
            is_exception=True,
        )
    ]
    assert bifrost.sql(
        "SELECT exception_type, exception_message, is_exception FROM bifrost.logs.records "
        "WHERE scope_name = 'wyrd.tests.otel.owned'",
        model=ExceptionRow,
    ) == [
        ExceptionRow(exception_type="ValueError", exception_message="sold out", is_exception=True)
    ]


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
        "SELECT severity_text, severity_number, trace_id, span_id FROM bifrost.logs.records "
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
        "histogram_sum FROM bifrost.metrics.points "
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
