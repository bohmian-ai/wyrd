"""Real Python SDK to Gate to Oracle query journey."""

import asyncio
import json

import pyarrow
import pytest
from wyrd import WyrdError
from wyrd.bifrost import AsyncBifrost
from wyrd.testing import WyrdTestServer


@pytest.mark.integration
def test_bifrost_query_yields_pyarrow_and_terminal(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()

    async def query() -> tuple[list[pyarrow.RecordBatch], dict[str, object] | None]:
        stream = await AsyncBifrost(server_url=wyrd_server.base_url, credential=token).stream(
            f"SELECT id, value FROM {table_fqn} ORDER BY id",
        )
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, terminal = asyncio.run(query())
    assert batches
    assert all(isinstance(batch, pyarrow.RecordBatch) for batch in batches)
    ids = [value for batch in batches for value in batch.column("id").to_pylist()]
    assert ids == [1, 2, 3], "one query reads the published cut and the live row"
    assert terminal is not None
    assert terminal["outcome"] == "success"
    assert terminal["row_count"] == 3
    assert terminal["warnings"] == []
    assert "freshness" not in terminal


@pytest.mark.integration
def test_query_stream_schema_once_eos(wyrd_server: WyrdTestServer) -> None:
    """The Python journey sees one schema and an explicitly closed IPC stream.

    The server emits one Arrow IPC stream split across Wyrd frames, so every
    batch this client decodes carries the query's single schema and the
    successful terminal carries the end-of-stream delta that closes it. An empty
    end-of-stream would mean a truncated result rather than a complete one.
    """
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()

    async def query() -> tuple[list[pyarrow.RecordBatch], dict[str, object] | None]:
        stream = await AsyncBifrost(server_url=wyrd_server.base_url, credential=token).stream(
            f"SELECT id, value FROM {table_fqn} ORDER BY id",
        )
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, terminal = asyncio.run(query())
    assert batches
    schemas = {batch.schema for batch in batches}
    assert len(schemas) == 1
    assert [field.name for field in batches[0].schema] == ["id", "value"]
    assert sum(batch.num_rows for batch in batches) == 3
    assert terminal is not None
    assert terminal["outcome"] == "success"
    assert terminal["row_count"] == 3
    assert bytes(terminal["arrow_ipc_eos"]) == b"\xff\xff\xff\xff\x00\x00\x00\x00"


@pytest.mark.integration
@pytest.mark.parametrize("fault", ["schema", "batch"])
def test_bifrost_query_missing_terminal_fails_closed(
    wyrd_server: WyrdTestServer, fault: str
) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    getattr(wyrd_server, f"fail_next_query_after_{fault}")()

    async def query() -> None:
        stream = await AsyncBifrost(server_url=wyrd_server.base_url, credential=token).stream(
            f"SELECT id, value FROM {table_fqn} ORDER BY id",
        )
        with pytest.raises(WyrdError) as captured:
            while True:
                await stream.__anext__()
        assert captured.value.code == "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"
        assert captured.value.status == 502
        assert captured.value.title == "Query stream incomplete"
        assert captured.value.message == captured.value.detail
        assert captured.value.remediation
        assert captured.value.details == {"variant": "query_stream_incomplete"}
        await stream.aclose()
        with pytest.raises(StopAsyncIteration):
            await stream.__anext__()

    asyncio.run(query())


@pytest.mark.integration
def test_bifrost_query_out_of_range_deadline_is_the_shared_validation_error(
    wyrd_server: WyrdTestServer,
) -> None:
    _table_fqn, token = wyrd_server.prepare_oracle_query_fixture()

    async def query(deadline_ms: int) -> None:
        with pytest.raises(WyrdError) as captured:
            await AsyncBifrost(server_url=wyrd_server.base_url, credential=token).stream(
                "SELECT 1",
                deadline_ms=deadline_ms,
            )
        assert captured.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
        assert captured.value.status == 400

    for deadline_ms in (0, 2**32):
        asyncio.run(query(deadline_ms))


@pytest.mark.integration
def test_bifrost_query_gate_denial_has_no_oracle_side_effect(
    wyrd_server: WyrdTestServer,
) -> None:
    table_fqn, _token = wyrd_server.prepare_oracle_query_fixture()
    denied_token = wyrd_server.query_denied_token()
    before = wyrd_server.bifrost_read_decision_count()

    async def query() -> None:
        with pytest.raises(WyrdError) as captured:
            await AsyncBifrost(server_url=wyrd_server.base_url, credential=denied_token).stream(
                f"SELECT * FROM {table_fqn}",
            )
        assert captured.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
        assert captured.value.status == 403
        assert captured.value.title == "Permission denied (RBAC)"
        assert captured.value.detail
        assert captured.value.remediation
        assert isinstance(captured.value.details, dict)

    asyncio.run(query())
    assert wyrd_server.bifrost_read_decision_count() == before


@pytest.mark.integration
def test_bifrost_query_cancellation_releases_all_resources(
    wyrd_server: WyrdTestServer,
) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture(fused=True)
    baseline = {
        "admission_slots": 0,
        "memory_bytes": 0,
        "peer_slots": 0,
    }
    wyrd_server.stall_next_query_after_schema()

    async def cancel_query() -> tuple[dict[str, int], dict[str, int]]:
        stream = await AsyncBifrost(server_url=wyrd_server.base_url, credential=token).stream(
            f"SELECT id, value FROM {table_fqn} ORDER BY id",
        )
        consumer = asyncio.create_task(stream.__anext__())
        query_id = await asyncio.to_thread(wyrd_server.wait_query_schema_stall)
        active = await asyncio.to_thread(wyrd_server.bifrost_query_resource_snapshot, query_id)
        consumer.cancel("cancel stalled Bifrost query")
        with pytest.raises(asyncio.CancelledError):
            await consumer
        released = await asyncio.to_thread(
            wyrd_server.wait_bifrost_query_resources_released,
            query_id,
            baseline,
        )
        await stream.aclose()
        with pytest.raises(StopAsyncIteration):
            await stream.__anext__()
        return active, released

    active, released = asyncio.run(cancel_query())
    assert active["admission_slots"] > baseline["admission_slots"]
    assert active["memory_bytes"] > baseline["memory_bytes"]
    assert active["peer_slots"] > baseline["peer_slots"]
    assert released == baseline


MODEL = "claude-opus-5"
INPUT_TOKENS = 1280
OUTPUT_TOKENS = 320
INPUT_MESSAGES = '[{"role":"user","parts":[{"type":"text","content":"summarize the incident"}]}]'
OUTPUT_MESSAGES = '[{"role":"assistant","parts":[{"type":"text","content":"the writer stalled"}]}]'
LOG_BODY = "tool call exhausted its retry budget"
EVENT_NAME = "gen_ai.choice"
COUNTER_VALUE = 7
GAUGE_VALUE = 0.75
HISTOGRAM_VALUES = (1.0, 2.5, 3.0, 6.0)
SERVICE = "wyrd.fixture.service"


def _otlp_headers(server: WyrdTestServer) -> tuple[tuple[str, str], ...]:
    """The access-token header a stock OTLP exporter sends to the Wyrd collector."""

    return (("x-wyrd-access-token", f"Bearer {server.access_token()}"),)


TRACE_ID = bytes.fromhex("c1a0112233445566778899aabbccddee")
PARENT_SPAN_ID = bytes.fromhex("a1a2a3a4a5a6a7a8")
CHILD_SPAN_ID = bytes.fromhex("b1b2b3b4b5b6b7b8")


def _is_variant(field: pyarrow.Field) -> bool:
    return (field.metadata or {}).get(b"ARROW:extension:name") == b"arrow.parquet.variant"


def _empty(field: pyarrow.Field) -> object:
    """The value a column takes in a row that does not name it."""

    if field.nullable:
        return None
    if _is_variant(field):
        return "{}"
    if pyarrow.types.is_string(field.type):
        return ""
    if pyarrow.types.is_boolean(field.type):
        return False
    if pyarrow.types.is_list(field.type):
        return []
    return 0


def _batch(schema: pyarrow.Schema, rows: list[dict[str, object]]) -> pyarrow.RecordBatch:
    """One batch for a built-in signal table, written the way a caller would.

    Variant columns are JSON text, which ``write_batch`` encodes. Nullable
    columns no row names are left out and arrive as nulls; required ones take
    their empty value.
    """

    columns = [
        field for field in schema if not field.nullable or any(field.name in row for row in rows)
    ]
    return pyarrow.RecordBatch.from_pylist(
        [{field.name: row.get(field.name, _empty(field)) for field in columns} for row in rows],
        schema=pyarrow.schema(
            pyarrow.field(field.name, pyarrow.string(), field.nullable)
            if _is_variant(field)
            else field
            for field in columns
        ),
    )


def _envelope(scope: str) -> dict[str, object]:
    """The resource and scope every signal row shares."""

    return {
        "resource_present": True,
        "resource_attributes": json.dumps({"service.name": SERVICE}),
        "scope_present": True,
        "scope_name": scope,
        "scope_version": "1.0.0",
    }


def _spans(scope: str, anchor: int) -> list[dict[str, object]]:
    """The GenAI chat span and the failing tool call it made."""

    span = _envelope(scope) | {
        "trace_id": TRACE_ID,
        "service_name": SERVICE,
        "gen_ai_request_model": MODEL,
        "status_present": True,
    }
    chat = span | {
        "span_id": PARENT_SPAN_ID,
        "name": "chat claude-opus-5",
        "kind": 3,
        "start_time_unix_nano": anchor,
        "end_time_unix_nano": anchor + 2_000_000,
        "duration_nano": 2_000_000,
        "status_code": 1,
        "attributes": json.dumps(
            {
                "gen_ai.input.messages": INPUT_MESSAGES,
                "gen_ai.output.messages": OUTPUT_MESSAGES,
                "gen_ai.usage.input_tokens": INPUT_TOKENS,
            }
        ),
        "gen_ai_operation_name": "chat",
        "gen_ai_usage_input_tokens": INPUT_TOKENS,
        "gen_ai_usage_output_tokens": OUTPUT_TOKENS,
    }
    tool = span | {
        "span_id": CHILD_SPAN_ID,
        "parent_span_id": PARENT_SPAN_ID,
        "name": "execute_tool search",
        "kind": 1,
        "start_time_unix_nano": anchor + 100_000,
        "end_time_unix_nano": anchor + 900_000,
        "duration_nano": 800_000,
        "status_code": 2,
        "status_message": LOG_BODY,
        "attributes": json.dumps({"gen_ai.tool.name": "search"}),
        "gen_ai_operation_name": "execute_tool",
        "gen_ai_usage_input_tokens": 64,
        "gen_ai_usage_output_tokens": 16,
    }
    return [chat, tool]


def _logs(scope: str, anchor: int) -> list[dict[str, object]]:
    """The error log the failing tool call wrote."""

    return [
        _envelope(scope)
        | {
            "time_unix_nano": anchor + 800_000,
            "observed_time_unix_nano": anchor + 850_000,
            "severity_number": 17,
            "severity_text": "ERROR",
            "body": json.dumps(LOG_BODY),
            "trace_id": TRACE_ID,
            "span_id": CHILD_SPAN_ID,
            "attributes": json.dumps({"gen_ai.tool.name": "search"}),
        }
    ]


def _points(scope: str, anchor: int) -> list[dict[str, object]]:
    """One counter, one gauge, and one histogram point."""

    point = _envelope(scope) | {
        "unit": "1",
        "time_unix_nano": anchor,
        "start_time_unix_nano": anchor,
        "attributes": json.dumps({"gen_ai.request.model": MODEL}),
    }
    return [
        point
        | {
            "metric_name": "wyrd.fixture.requests",
            "metric_type": "sum",
            "int_value": COUNTER_VALUE,
            "aggregation_temporality": 2,
            "is_monotonic": True,
        },
        point
        | {
            "metric_name": "wyrd.fixture.saturation",
            "metric_type": "gauge",
            "double_value": GAUGE_VALUE,
        },
        point
        | {
            "metric_name": "wyrd.fixture.latency",
            "metric_type": "histogram",
            "aggregation_temporality": 2,
            "histogram_count": len(HISTOGRAM_VALUES),
            "histogram_sum": sum(HISTOGRAM_VALUES),
            "histogram_min": min(HISTOGRAM_VALUES),
            "histogram_max": max(HISTOGRAM_VALUES),
        },
    ]


@pytest.mark.integration
def test_canonical_signal_arrow_write_and_sql_read_round_trip(
    wyrd_server: WyrdTestServer,
) -> None:
    """Python writes canonical signal Arrow batches and reads them back natively.

    Each batch follows the table's described schema with its Variant columns
    as JSON text, which ``write_batch`` encodes. Hierarchy, token totals,
    log-to-span correlation, and metric aggregation are canonical SQL; Variant
    payloads read back as native values, and the payload gate is proved from
    the caller's side with a separately scoped principal.
    """

    import uuid
    from typing import Any

    from pydantic import BaseModel
    from wyrd.bifrost import Bifrost

    for namespace, name in (("traces", "spans"), ("logs", "records"), ("metrics", "points")):
        wyrd_server.ensure_builtin_table(namespace, name)

    scope = f"wyrd.python.canonical.{uuid.uuid4().hex}"
    anchor = 1_760_000_000_000_000_000
    writer = Bifrost(
        server_url=wyrd_server.base_url,
        credential=wyrd_server.bootstrap_service(["admin"], "python-canonical-writer"),
    )
    for table, rows in (
        ("vala.traces.spans", _spans(scope, anchor)),
        ("vala.logs.records", _logs(scope, anchor)),
        ("vala.metrics.points", _points(scope, anchor)),
    ):
        writer.use_table_by_name(table)
        assert writer.table is not None
        writer.write_batch(table, _batch(writer.table.arrow_schema, rows))
    wyrd_server.flush_bifrost()

    hierarchy = (
        writer.sql(
            "SELECT name, gen_ai_operation_name, status_code, "
            "CAST(CASE WHEN parent_span_id IS NULL THEN 1 ELSE 0 END AS BIGINT) AS is_root "
            f"FROM vala.traces.spans WHERE scope_name = '{scope}' "
            "ORDER BY is_root DESC"
        )
        .to_arrow()
        .to_pylist()
    )
    assert [row["gen_ai_operation_name"] for row in hierarchy] == ["chat", "execute_tool"]
    assert [row["is_root"] for row in hierarchy] == [1, 0]
    assert [row["status_code"] for row in hierarchy] == [1, 2]

    tokens = (
        writer.sql(
            "SELECT CAST(SUM(gen_ai_usage_input_tokens) AS BIGINT) AS input_tokens, "
            "CAST(SUM(gen_ai_usage_output_tokens) AS BIGINT) AS output_tokens, "
            "CAST(COUNT(*) AS BIGINT) AS spans FROM vala.traces.spans "
            f"WHERE scope_name = '{scope}' AND gen_ai_request_model = '{MODEL}'"
        )
        .to_arrow()
        .to_pylist()
    )
    assert tokens == [
        {
            "input_tokens": INPUT_TOKENS + 64,
            "output_tokens": OUTPUT_TOKENS + 16,
            "spans": 2,
        }
    ]

    class Correlated(BaseModel):
        severity_text: str
        body: Any
        tool: str
        span_name: str

    correlated = writer.sql(
        "SELECT l.severity_text, l.body, l.attributes ->> 'gen_ai.tool.name' AS tool, "
        "s.name AS span_name "
        "FROM vala.logs.records l JOIN vala.traces.spans s "
        "ON l.trace_id = s.trace_id AND l.span_id = s.span_id "
        f"WHERE l.scope_name = '{scope}'",
        Correlated,
    )
    assert [row.model_dump() for row in correlated] == [
        {
            "severity_text": "ERROR",
            "body": LOG_BODY,
            "tool": "search",
            "span_name": "execute_tool search",
        }
    ]

    metrics = (
        writer.sql(
            "SELECT metric_type, "
            "CAST(SUM(COALESCE(int_value, 0)) AS BIGINT) AS ints, "
            "CAST(SUM(COALESCE(double_value, 0.0)) AS DOUBLE) AS doubles, "
            "CAST(SUM(COALESCE(histogram_count, 0)) AS BIGINT) AS observations, "
            "CAST(SUM(COALESCE(histogram_sum, 0.0)) AS DOUBLE) AS observed "
            f"FROM vala.metrics.points WHERE scope_name = '{scope}' "
            "GROUP BY metric_type ORDER BY metric_type"
        )
        .to_arrow()
        .to_pylist()
    )
    assert metrics == [
        {
            "metric_type": "gauge",
            "ints": 0,
            "doubles": GAUGE_VALUE,
            "observations": 0,
            "observed": 0.0,
        },
        {
            "metric_type": "histogram",
            "ints": 0,
            "doubles": 0.0,
            "observations": len(HISTOGRAM_VALUES),
            "observed": sum(HISTOGRAM_VALUES),
        },
        {
            "metric_type": "sum",
            "ints": COUNTER_VALUE,
            "doubles": 0.0,
            "observations": 0,
            "observed": 0.0,
        },
    ]

    class Payload(BaseModel):
        attributes: dict[str, Any]
        input_tokens: int
        service: str

    payload_reader = Bifrost(
        server_url=wyrd_server.base_url,
        credential=wyrd_server.scoped_api_key(
            f"py_canonical_reader_{uuid.uuid4().hex[:8]}", ["bifrost_query:read"]
        ),
    )
    payload = payload_reader.sql(
        "SELECT attributes, "
        "CAST(attributes ->> 'gen_ai.usage.input_tokens' AS BIGINT) AS input_tokens, "
        "resource_attributes ->> 'service.name' AS service "
        "FROM vala.traces.spans "
        f"WHERE scope_name = '{scope}' AND parent_span_id IS NULL",
        Payload,
    )
    assert payload == [
        Payload(
            attributes={
                "gen_ai.input.messages": INPUT_MESSAGES,
                "gen_ai.output.messages": OUTPUT_MESSAGES,
                "gen_ai.usage.input_tokens": INPUT_TOKENS,
            },
            input_tokens=INPUT_TOKENS,
            service=SERVICE,
        )
    ]


@pytest.mark.integration
def test_builtin_variant_and_struct_payloads_are_queryable(
    wyrd_server: WyrdTestServer,
) -> None:
    """A stock OTLP span's Variant and Struct payloads read back natively.

    The exporter writes Variant attribute collections and Struct events. The
    Arrow terminal keeps the Variant extension, the typed terminal decodes
    every Variant into ``dict``, ``list``, and exact ``int`` values, Struct
    access stays exact, and ``parse_json`` over invalid JSON is the stable
    Variant error, before the first batch or after a delivered one, while
    ``try_parse_json`` is null.
    """

    import os
    import uuid
    from typing import Any

    from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
    from opentelemetry.sdk.resources import Resource
    from opentelemetry.sdk.trace import TracerProvider
    from opentelemetry.sdk.trace.export import SimpleSpanProcessor
    from pydantic import BaseModel
    from wyrd.bifrost import Bifrost, TableConfig

    scope = f"wyrd.python.variant.{uuid.uuid4().hex}"
    traces = TracerProvider(resource=Resource.create({"service.name": SERVICE}))
    traces.add_span_processor(
        SimpleSpanProcessor(
            OTLPSpanExporter(
                endpoint=os.environ["WYRD_GRPC_URL"],
                insecure=True,
                headers=_otlp_headers(wyrd_server),
            )
        )
    )
    with traces.get_tracer(scope, "1.0.0").start_as_current_span("variant-parent") as span:
        span.set_attribute("gen_ai.request.model", MODEL)
        span.set_attribute("wyrd.test.big", 2**60)
        span.set_attribute("wyrd.test.values", [1, 2, 3])
        span.add_event(EVENT_NAME, {"gen_ai.finish_reason": "stop"})
    traces.shutdown()
    wyrd_server.flush_bifrost()

    reader = Bifrost(server_url=wyrd_server.base_url, credential=wyrd_server.api_key)
    where = f"FROM vala.traces.spans WHERE scope_name = '{scope}'"
    raw = reader.sql(f"SELECT attributes {where}").to_arrow()
    field = raw.schema.field("attributes")
    extension = (
        getattr(field.type, "extension_name", None)
        or (field.metadata or {}).get(b"ARROW:extension:name", b"").decode()
    )
    assert extension == "arrow.parquet.variant", "the Arrow terminal keeps the extension"

    class Row(BaseModel):
        model: str
        service: str
        big: int
        absent: Any
        attributes: dict[str, Any]
        event_name: str
        finish_reason: str
        parsed: dict[str, Any]
        lenient: Any

    rows = reader.sql(
        "SELECT attributes ->> 'gen_ai.request.model' AS model, "
        "resource_attributes ->> 'service.name' AS service, "
        "CAST(attributes ->> 'wyrd.test.big' AS BIGINT) AS big, "
        "attributes -> 'absent' AS absent, attributes, "
        "events[1]['name'] AS event_name, "
        "events[1]['attributes'] ->> 'gen_ai.finish_reason' AS finish_reason, "
        """parse_json('{"n": 9007199254740993, "u": 18446744073709551615, "a": [1, "x", null]}') """
        "AS parsed, "
        f"try_parse_json('{{bad') AS lenient {where}",
        Row,
    )
    assert [row.model_dump() for row in rows] == [
        {
            "model": MODEL,
            "service": SERVICE,
            "big": 2**60,
            "absent": None,
            "attributes": {
                "gen_ai.request.model": MODEL,
                "wyrd.test.big": 2**60,
                "wyrd.test.values": [1, 2, 3],
            },
            "event_name": EVENT_NAME,
            "finish_reason": "stop",
            "parsed": {"n": 9007199254740993, "u": 2**64 - 1, "a": [1, "x", None]},
            "lenient": None,
        }
    ]

    with pytest.raises(WyrdError) as invalid:
        reader.sql(f"SELECT parse_json('{{bad') AS v {where}")
    assert invalid.value.code == "WYRD_VALA_400_VARIANT_INVALID_JSON"

    # A failure after a delivered batch keeps the pre-stream problem: one
    # published object streams 8192-row batches in id order, so rows from id
    # 8192 fail only in the second batch. An unrelated late cast failure stays
    # generic, and neither result is returned partially.
    class LateRow(BaseModel):
        id: int
        value: str

    late_fqn = f"vala.datasets.variant_late_{uuid.uuid4().hex}"
    writer = Bifrost(
        TableConfig(LateRow, late_fqn),
        server_url=wyrd_server.base_url,
        credential=wyrd_server.api_key,
    )
    assert writer.register() == "created"
    writer.write_batch(
        late_fqn,
        pyarrow.record_batch(
            [pyarrow.array(range(10_000), pyarrow.int64()), pyarrow.array(["batch"] * 10_000)],
            schema=pyarrow.schema(
                [
                    pyarrow.field("id", pyarrow.int64(), nullable=False),
                    pyarrow.field("value", pyarrow.string(), nullable=False),
                ]
            ),
        ),
    )
    writer.shutdown()
    wyrd_server.flush_bifrost()
    early = invalid.value
    for value, expected in [
        ("parse_json(CASE WHEN id < 8192 THEN '1' ELSE '{bad' END)", early),
        ("CAST(CASE WHEN id < 8192 THEN '1' ELSE 'x' END AS BIGINT)", None),
    ]:
        sql = f"SELECT id, {value} AS v FROM {late_fqn}"
        delivered = 0
        with pytest.raises(WyrdError) as streamed:
            for batch in reader.stream(sql):
                delivered += batch.num_rows
        assert delivered == 8192, "one valid batch preceded the failure"
        with pytest.raises(WyrdError) as collected:
            reader.sql(sql)
        for late in (streamed.value, collected.value):
            if expected is None:
                assert late.code == "WYRD_VALA_500_QUERY_EXECUTION_FAILED"
                assert late.details == {"variant": "query_execution_failed"}
            else:
                assert (late.code, late.status, late.detail, late.details) == (
                    expected.code,
                    expected.status,
                    expected.detail,
                    expected.details,
                )
