"""A stock OpenTelemetry span exporter, configured only by ``OTEL_EXPORTER_OTLP_*``, lands in Bifrost.

A deployment points the exporter at the server's gRPC address and authenticates
it with the ``x-wyrd-api-key`` header; the span then reads back through
``Bifrost.sql`` as typed rows.
"""

import pytest
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.trace import Status, StatusCode
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration


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
    # Publish the exported spans so the query reads them.
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
    assert (answer.status_code, answer.status_message) == (StatusCode.ERROR.value, "refused")
    assert (answer.gen_ai_request_model, answer.gen_ai_usage_input_tokens) == (
        "claude-opus-5",
        4096,
    )
