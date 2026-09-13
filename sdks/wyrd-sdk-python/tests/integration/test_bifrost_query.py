"""Real Python SDK to Gate to Oracle query journey."""

import asyncio

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
            visibility="published_only",
            freshness="strict",
        )
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, terminal = asyncio.run(query())
    assert batches
    assert all(isinstance(batch, pyarrow.RecordBatch) for batch in batches)
    assert sum(batch.num_rows for batch in batches) == 2
    assert terminal is not None
    assert terminal["outcome"] == "success"
    assert terminal["row_count"] == 2


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
            visibility="published_only",
            freshness="strict",
        )
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, terminal = asyncio.run(query())
    assert batches
    schemas = {batch.schema for batch in batches}
    assert len(schemas) == 1
    assert [field.name for field in batches[0].schema] == ["id", "value"]
    assert sum(batch.num_rows for batch in batches) == 2
    assert terminal is not None
    assert terminal["outcome"] == "success"
    assert terminal["row_count"] == 2
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
        "tail_fences": 0,
    }
    wyrd_server.stall_next_query_after_schema()

    async def cancel_query() -> tuple[dict[str, int], dict[str, int]]:
        stream = await AsyncBifrost(server_url=wyrd_server.base_url, credential=token).stream(
            f"SELECT id, value FROM {table_fqn} ORDER BY id",
            visibility="fused",
            freshness="strict",
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
    # The live tail is drained into query-owned memory before schema emission.
    assert active["tail_fences"] == baseline["tail_fences"]
    assert released == baseline


TRACE_ID = bytes.fromhex("c1a0112233445566778899aabbccddee")
PARENT_SPAN_ID = bytes.fromhex("a1a2a3a4a5a6a7a8")
CHILD_SPAN_ID = bytes.fromhex("b1b2b3b4b5b6b7b8")
MODEL = "claude-opus-5"
INPUT_TOKENS = 1280
OUTPUT_TOKENS = 320
INPUT_MESSAGES = '[{"role":"user","parts":[{"type":"text","content":"summarize the incident"}]}]'
OUTPUT_MESSAGES = '[{"role":"assistant","parts":[{"type":"text","content":"the writer stalled"}]}]'
LOG_BODY = "tool call exhausted its retry budget"
EVENT_NAME = "gen_ai.choice"
LINK_TRACE_STATE = "wyrd=fixture"
LINKED_SPAN_ID = bytes.fromhex("c1c2c3c4c5c6c7c8")
COUNTER_VALUE = 7
GAUGE_VALUE = 0.75
HISTOGRAM_COUNT = 4
HISTOGRAM_SUM = 12.5
SERVICE = "wyrd.fixture.service"


def _attributes(pairs: dict[str, str]) -> bytes:
    """Encode string attributes as the canonical ``KeyValueList`` bytes.

    Ingress decodes and re-encodes every canonical binary payload, so a
    fixture cannot substitute a JSON blob here.
    """

    from opentelemetry.proto.common.v1.common_pb2 import AnyValue, KeyValue, KeyValueList

    return KeyValueList(
        values=[
            KeyValue(key=key, value=AnyValue(string_value=value)) for key, value in pairs.items()
        ]
    ).SerializeToString()


def _any_value(text: str) -> bytes:
    """Encode one string as the canonical ``AnyValue`` bytes a log body carries."""

    from opentelemetry.proto.common.v1.common_pb2 import AnyValue

    return AnyValue(string_value=text).SerializeToString()


def _default(field: pyarrow.Field) -> object:
    """The value an unnamed column takes, decided by its described type."""

    if field.nullable:
        return None
    if pyarrow.types.is_string(field.type):
        return ""
    if pyarrow.types.is_boolean(field.type):
        return False
    if pyarrow.types.is_floating(field.type):
        return 0.0
    if pyarrow.types.is_binary(field.type) or pyarrow.types.is_fixed_size_binary(field.type):
        return b""
    if pyarrow.types.is_list(field.type):
        return []
    return 0


def _batch(schema: pyarrow.Schema, rows: list[dict[str, object]]) -> pyarrow.RecordBatch:
    """Build one batch over ``schema`` from rows that name only some columns.

    The schema comes from the server's own description, so the fixture never
    restates the canonical ledger: it supplies the handful of values its
    assertions depend on and lets every other described column take the
    canonical empty value for its type.
    """

    columns = [
        pyarrow.array(
            [row.get(field.name, _default(field)) for row in rows],
            type=field.type,
        )
        for field in schema
    ]
    return pyarrow.RecordBatch.from_arrays(columns, schema=schema)


def _spans(schema: pyarrow.Schema, scope: str, anchor: int) -> pyarrow.RecordBatch:
    """The parent GenAI chat span and the failing tool span it made."""

    envelope = {
        "resource_present": True,
        "resource_attributes": _attributes({"service.name": SERVICE}),
        "scope_present": True,
        "scope_name": scope,
        "scope_version": "1.0.0",
        "service_name": SERVICE,
        "gen_ai_provider_name": "anthropic",
        "gen_ai_request_model": MODEL,
        "gen_ai_conversation_id": "conversation-fixture",
        "trace_id": TRACE_ID,
        "status_present": True,
    }
    parent = envelope | {
        "span_id": PARENT_SPAN_ID,
        "name": "chat claude-opus-5",
        "kind": 3,
        "start_time_unix_nano": anchor,
        "end_time_unix_nano": anchor + 2_000_000,
        "duration_nano": 2_000_000,
        "status_code": 1,
        "status_message": "ok",
        "attributes": _attributes(
            {
                "gen_ai.input.messages": INPUT_MESSAGES,
                "gen_ai.output.messages": OUTPUT_MESSAGES,
            }
        ),
        "gen_ai_operation_name": "chat",
        "gen_ai_usage_input_tokens": INPUT_TOKENS,
        "gen_ai_usage_output_tokens": OUTPUT_TOKENS,
        "events": [
            {
                "time_unix_nano": anchor + 1_000_000,
                "name": EVENT_NAME,
                "attributes": _attributes({"gen_ai.finish_reason": "stop"}),
                "dropped_attributes_count": 0,
            }
        ],
        "links": [
            {
                "trace_id": TRACE_ID,
                "span_id": LINKED_SPAN_ID,
                "trace_state": LINK_TRACE_STATE,
                "flags": 1,
                "attributes": _attributes({"link.kind": "follows_from"}),
                "dropped_attributes_count": 0,
            }
        ],
    }
    child = envelope | {
        "span_id": CHILD_SPAN_ID,
        "parent_span_id": PARENT_SPAN_ID,
        "name": "execute_tool search",
        "kind": 1,
        "start_time_unix_nano": anchor + 100_000,
        "end_time_unix_nano": anchor + 900_000,
        "duration_nano": 800_000,
        "status_code": 2,
        "status_message": LOG_BODY,
        "attributes": _attributes({"gen_ai.tool.name": "search"}),
        "gen_ai_operation_name": "execute_tool",
        "gen_ai_usage_input_tokens": 64,
        "gen_ai_usage_output_tokens": 16,
    }
    return _batch(schema, [parent, child])


def _logs(schema: pyarrow.Schema, scope: str, anchor: int) -> pyarrow.RecordBatch:
    """The error log correlated to the tool span that failed."""

    return _batch(
        schema,
        [
            {
                "time_unix_nano": anchor + 800_000,
                "observed_time_unix_nano": anchor + 850_000,
                "severity_number": 17,
                "severity_text": "ERROR",
                "event_name": "tool.retry.exhausted",
                "body": _any_value(LOG_BODY),
                "trace_id": TRACE_ID,
                "span_id": CHILD_SPAN_ID,
                "attributes": _attributes({"gen_ai.tool.name": "search"}),
                "resource_present": True,
                "resource_attributes": _attributes({"service.name": SERVICE}),
                "scope_present": True,
                "scope_name": scope,
                "scope_version": "1.0.0",
            }
        ],
    )


def _points(schema: pyarrow.Schema, scope: str, anchor: int) -> pyarrow.RecordBatch:
    """One counter, one gauge and one histogram point."""

    def common(name: str, kind: str) -> dict[str, object]:
        return {
            "metric_name": name,
            "description": f"fixture {kind}",
            "unit": "1",
            "metric_type": kind,
            "time_unix_nano": anchor,
            "start_time_unix_nano": anchor,
            "attributes": _attributes({"gen_ai.request.model": MODEL}),
            "resource_present": True,
            "resource_attributes": _attributes({"service.name": SERVICE}),
            "scope_present": True,
            "scope_name": scope,
            "scope_version": "1.0.0",
        }

    counter = common("wyrd.fixture.requests", "sum") | {
        "int_value": COUNTER_VALUE,
        "aggregation_temporality": 2,
        "is_monotonic": True,
    }
    gauge = common("wyrd.fixture.saturation", "gauge") | {"double_value": GAUGE_VALUE}
    histogram = common("wyrd.fixture.latency", "histogram") | {
        "aggregation_temporality": 2,
        "histogram_count": HISTOGRAM_COUNT,
        "histogram_sum": HISTOGRAM_SUM,
        "histogram_min": 1.0,
        "histogram_max": 6.0,
    }
    return _batch(schema, [counter, gauge, histogram])


@pytest.mark.integration
def test_canonical_signal_arrow_write_and_sql_read_round_trip(
    wyrd_server: WyrdTestServer,
) -> None:
    """Python builds canonical Arrow signals, writes them, and reads them back.

    Every batch is built against the schema the server publishes for that
    table, so this is the journey a Python caller actually has: bind the table
    by name, read its Arrow schema, write, publish, and answer questions
    through canonical SQL. The payload gate is proved from the caller's side
    with two differently scoped principals.
    """

    import uuid

    from wyrd.bifrost import Bifrost

    for namespace, name in (("traces", "spans"), ("logs", "records"), ("metrics", "points")):
        wyrd_server.ensure_builtin_table(namespace, name)

    scope = f"wyrd.python.canonical.{uuid.uuid4().hex}"
    anchor = 1_760_000_000_000_000_000
    writer = Bifrost(
        server_url=wyrd_server.base_url,
        credential=wyrd_server.bootstrap_service(["admin"], "python-canonical-writer"),
    )
    for fqn, build in (
        ("vala.traces.spans", _spans),
        ("vala.logs.records", _logs),
        ("vala.metrics.points", _points),
    ):
        writer.use_table_by_name(fqn)
        bound = writer.table
        assert bound is not None
        writer.write_batch(fqn, build(bound.arrow_schema, scope, anchor))
    wyrd_server.flush_bifrost()

    hierarchy = (
        writer.sql(
            "SELECT name, gen_ai_operation_name, status_code, "
            "CAST(CASE WHEN parent_span_id IS NULL THEN 1 ELSE 0 END AS BIGINT) AS is_root "
            f"FROM vala.traces.spans WHERE scope_name = '{scope}' "
            "ORDER BY start_time_unix_nano"
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

    correlated = (
        writer.sql(
            "SELECT l.severity_text, l.event_name, s.name AS span_name "
            "FROM vala.logs.records l JOIN vala.traces.spans s "
            "ON l.trace_id = s.trace_id AND l.span_id = s.span_id "
            f"WHERE l.scope_name = '{scope}'"
        )
        .to_arrow()
        .to_pylist()
    )
    assert correlated == [
        {
            "severity_text": "ERROR",
            "event_name": "tool.retry.exhausted",
            "span_name": "execute_tool search",
        }
    ]

    metrics = (
        writer.sql(
            "SELECT metric_type, "
            "CAST(SUM(COALESCE(int_value, 0)) AS BIGINT) AS ints, "
            "CAST(SUM(COALESCE(double_value, 0.0)) AS DOUBLE) AS doubles, "
            "CAST(SUM(COALESCE(histogram_count, 0)) AS BIGINT) AS observations "
            f"FROM vala.metrics.points WHERE scope_name = '{scope}' "
            "GROUP BY metric_type ORDER BY metric_type"
        )
        .to_arrow()
        .to_pylist()
    )
    assert metrics == [
        {"metric_type": "gauge", "ints": 0, "doubles": GAUGE_VALUE, "observations": 0},
        {
            "metric_type": "histogram",
            "ints": 0,
            "doubles": 0.0,
            "observations": HISTOGRAM_COUNT,
        },
        {"metric_type": "sum", "ints": COUNTER_VALUE, "doubles": 0.0, "observations": 0},
    ]

    payload_reader = Bifrost(
        server_url=wyrd_server.base_url,
        credential=wyrd_server.scoped_api_key(
            f"py_canonical_reader_{uuid.uuid4().hex[:8]}", ["bifrost_query:read"]
        ),
    )
    nested = (
        payload_reader.sql(
            "SELECT CAST(array_length(events) AS BIGINT) AS events, "
            "CAST(array_length(links) AS BIGINT) AS links, "
            "events[1]['name'] AS event_name, links[1]['trace_state'] AS link_state "
            "FROM vala.traces.spans "
            f"WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
        )
        .to_arrow()
        .to_pylist()
    )
    assert nested == [
        {
            "events": 1,
            "links": 1,
            "event_name": EVENT_NAME,
            "link_state": LINK_TRACE_STATE,
        }
    ]

    messages = (
        payload_reader.sql(
            "SELECT CAST(attributes AS VARCHAR) AS attributes FROM vala.traces.spans "
            f"WHERE scope_name = '{scope}' AND parent_span_id IS NULL"
        )
        .to_arrow()
        .to_pylist()
    )
    assert len(messages) == 1
    assert INPUT_MESSAGES in messages[0]["attributes"]
    assert OUTPUT_MESSAGES in messages[0]["attributes"]
