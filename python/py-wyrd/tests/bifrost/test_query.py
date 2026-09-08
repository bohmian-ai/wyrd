"""Public Python Bifrost query projection tests."""

import asyncio
import inspect
import json
import threading
import uuid
from datetime import datetime, timezone

import pyarrow
import pytest
from wyrd.bifrost import (
    BifrostQueryClient,
    BifrostQueryError,
    BifrostQueryStream,
    IncompleteQueryStreamError,
)


def _ipc_batch() -> bytes:
    batch = pyarrow.record_batch([pyarrow.array([1, 2])], names=["id"])
    sink = pyarrow.BufferOutputStream()
    with pyarrow.ipc.new_stream(sink, batch.schema) as writer:
        writer.write_batch(batch)
    return sink.getvalue().to_pybytes()


class _NativeStream:
    def __init__(self, payloads: list[bytes], terminal: dict[str, object] | None) -> None:
        self._payloads = payloads
        self.terminal_json = json.dumps(terminal) if terminal is not None else None
        self.closed = False
        self.request_id = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff"

    def next_ipc(self) -> bytes | None:
        return self._payloads.pop(0) if self._payloads else None

    def close(self) -> None:
        self.closed = True

    def raise_incomplete_error(self) -> None:
        error = IncompleteQueryStreamError("query stream ended before its required terminal frame")
        error.code = "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"
        error.status = 502
        error.title = "Query stream incomplete"
        error.message = str(error)
        error.detail = str(error)
        error.remediation = (
            "Retry the query because the response ended before its required terminal frame."
        )
        error.details = None
        raise error


def test_bifrost_query_public_imports_are_available() -> None:
    assert BifrostQueryClient.__module__ == "wyrd.bifrost"
    assert BifrostQueryStream.__module__ == "wyrd.bifrost"


def test_bifrost_query_defaults_match_shared_client_contract() -> None:
    parameters = inspect.signature(BifrostQueryClient.query).parameters
    assert parameters["visibility"].default == "published_only"
    assert parameters["freshness"].default == "strict"


def test_bifrost_query_forwards_defaults_and_explicit_opt_ins() -> None:
    calls: list[tuple[str, str, str, int | None]] = []

    class NativeClient:
        def query(
            self,
            sql: str,
            visibility: str,
            freshness: str,
            deadline_ms: int | None,
        ) -> _NativeStream:
            calls.append((sql, visibility, freshness, deadline_ms))
            return _NativeStream([], {"outcome": "success", "row_count": 0})

    client = object.__new__(BifrostQueryClient)
    client._native = NativeClient()

    async def run() -> None:
        await client.query("SELECT 1")
        await client.query(
            "SELECT 1",
            visibility="fused",
            freshness="allow_degraded",
            deadline_ms=250,
        )

    asyncio.run(run())
    assert calls == [
        ("SELECT 1", "published_only", "strict", None),
        ("SELECT 1", "fused", "allow_degraded", 250),
    ]


def test_bifrost_query_async_iterator_yields_pyarrow_and_terminal() -> None:
    terminal = {"outcome": "success", "row_count": 2}
    native = _NativeStream([_ipc_batch()], terminal)

    async def consume() -> tuple[list[pyarrow.RecordBatch], dict[str, object] | None]:
        stream = BifrostQueryStream(native)
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, observed = asyncio.run(consume())
    assert len(batches) == 1
    assert isinstance(batches[0], pyarrow.RecordBatch)
    assert batches[0].num_rows == 2
    assert observed == terminal


def test_bifrost_query_missing_terminal_raises_typed_exception() -> None:
    native = _NativeStream([], None)

    async def consume() -> None:
        stream = BifrostQueryStream(native)
        await stream.__anext__()

    try:
        asyncio.run(consume())
    except IncompleteQueryStreamError as error:
        assert error.code == "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"
        assert error.status == 502
        assert error.title == "Query stream incomplete"
        assert error.message == error.detail
        assert error.remediation
        assert error.details is None
    else:
        raise AssertionError("missing terminal must raise IncompleteQueryStreamError")


def test_bifrost_query_aclose_drops_native_stream() -> None:
    native = _NativeStream([], {"outcome": "success", "row_count": 0})

    async def close() -> None:
        await BifrostQueryStream(native).aclose()

    asyncio.run(close())
    assert native.closed


def test_bifrost_query_request_id_and_lifecycle_controls_are_distinct_from_close() -> None:
    calls: list[tuple[str, str | None]] = []
    running = {
        "request_id": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff",
        "query_class": "interactive",
        "started_at": "2026-08-21T00:00:00Z",
        "deadline": "2026-08-21T00:01:00Z",
        "state": "running",
        "progress": {"completed_participants": 0, "total_participants": 1},
        "cancellation_requested": False,
    }

    class NativeClient:
        def running(self) -> list[dict[str, object]]:
            calls.append(("running", None))
            return [running]

        def status(self, request_id: str) -> dict[str, object]:
            calls.append(("status", request_id))
            return running

        def cancel(self, request_id: str) -> dict[str, object]:
            calls.append(("cancel", request_id))
            return {"request_id": request_id, "cancellation_started": True}

    client = object.__new__(BifrostQueryClient)
    client._native = NativeClient()

    async def controls() -> None:
        request_id = running["request_id"]
        assert await client.running() == [running]
        assert await client.status(request_id) == running  # type: ignore[arg-type]
        cancelled = await client.cancel(request_id)  # type: ignore[arg-type]
        assert cancelled["cancellation_started"] is True
        native_stream = _NativeStream([], None)
        stream = BifrostQueryStream(native_stream)
        assert stream.request_id == request_id
        await stream.aclose()
        assert native_stream.closed

    asyncio.run(controls())
    assert [call[0] for call in calls] == ["running", "status", "cancel"]
    assert "without requesting server cancellation" in BifrostQueryStream.aclose.__doc__


def test_bifrost_query_malformed_lifecycle_id_has_stable_error() -> None:
    client = BifrostQueryClient("http://127.0.0.1:1", "test-token")

    async def status() -> None:
        with pytest.raises(BifrostQueryError) as captured:
            await client.status("not-a-request-id")
        assert captured.value.code == "WYRD_SPEC_400_VALIDATION"
        assert captured.value.status == 400
        assert captured.value.details["field"] == "request_id"

    asyncio.run(status())


def test_bifrost_query_decode_failure_closes_native_without_masking_error() -> None:
    native = _NativeStream([b"not-arrow"], None)

    def failing_close() -> None:
        native.closed = True
        raise RuntimeError("cleanup failed")

    native.close = failing_close  # type: ignore[method-assign]

    async def consume() -> None:
        stream = BifrostQueryStream(native)
        await stream.__anext__()

    try:
        asyncio.run(consume())
    except Exception as error:  # noqa: BLE001 - assert decode error survives cleanup
        assert isinstance(error, pyarrow.ArrowException)
    else:
        raise AssertionError("invalid IPC must raise a decode error")
    assert native.closed


def test_bifrost_query_cancellation_waits_for_cleanup_and_preserves_cancelled_error() -> None:
    entered = threading.Event()
    released = threading.Event()

    class BlockingNativeStream(_NativeStream):
        def __init__(self) -> None:
            super().__init__([], None)
            self.close_calls = 0

        def next_ipc(self) -> bytes | None:
            entered.set()
            released.wait()
            return None

        def close(self) -> None:
            self.close_calls += 1
            self.closed = True
            released.set()
            if self.close_calls == 1:
                raise RuntimeError("secondary cleanup failure")

    native = BlockingNativeStream()

    async def cancel_poll() -> tuple[str, int]:
        stream = BifrostQueryStream(native)
        consumer = asyncio.create_task(stream.__anext__())
        assert await asyncio.to_thread(entered.wait)
        consumer.cancel("original cancellation")
        with pytest.raises(asyncio.CancelledError) as captured:
            await consumer
        await stream.aclose()
        with pytest.raises(StopAsyncIteration):
            await stream.__anext__()
        return captured.value.args[0], native.close_calls

    reason, close_calls = asyncio.run(cancel_poll())
    assert reason == "original cancellation"
    assert native.closed
    assert close_calls == 2


def test_typed_trace_and_genai_client_contracts() -> None:
    calls: list[tuple[str, tuple[object, ...]]] = []

    class NativeClient:
        def get_trace(
            self, trace_id: str, since: str | None, until: str | None
        ) -> dict[str, object]:
            calls.append(("get_trace", (trace_id, since, until)))
            return {
                "trace": {
                    "trace_id": trace_id,
                    "spans": [
                        {
                            "span_id": "0102030405060708",
                            "name": "chat",
                            "start_time_unix_nano": 7,
                            "events": [{"time_unix_nano": 8, "name": "chunk"}],
                            "links": [],
                        }
                    ],
                }
            }

        def query_genai(self, *args: object) -> dict[str, object]:
            calls.append(("query_genai", args))
            return {
                "rows": [
                    {
                        "model": "gpt-4o",
                        "start_time_unix_nano": 11,
                        "input_messages": [{"role": "user", "parts": [1, None]}],
                    }
                ],
                "next_page_token": "next",
            }

    client = object.__new__(BifrostQueryClient)
    client._native = NativeClient()

    async def run() -> tuple[dict[str, object], dict[str, object]]:
        trace = await client.get_trace(
            "0102030405060708090a0b0c0d0e0f10",
            since=datetime(2026, 7, 1, tzinfo=timezone.utc),
        )
        generations = await client.query_genai(model="gpt-4o", limit=10)
        return trace, generations

    trace, generations = asyncio.run(run())

    assert calls[0] == (
        "get_trace",
        ("0102030405060708090a0b0c0d0e0f10", "2026-07-01T00:00:00+00:00", None),
    )
    assert calls[1] == ("query_genai", (None, None, 10, None, None, "gpt-4o", None))

    spans = trace["trace"]["spans"]
    assert len(spans) == 1
    assert len(spans[0]["events"]) == 1, "events stay nested on their owning span"
    assert "events" not in trace["trace"], "trace detail carries no top-level children"

    assert generations["next_page_token"] == "next"
    row = generations["rows"][0]
    assert row["input_messages"] == [{"role": "user", "parts": [1, None]}]
    assert "output_messages" not in row, "an omitted payload stays absent"
    assert "prompt" not in row and "cost_usd" not in row


def test_canonical_arrow_insert_uses_described_schema() -> None:
    described = {
        "entry": {"namespace": "vala.traces", "name": "spans"},
        "user_fields": [{"name": "trace_id"}],
        "correlation_fields": [{"name": "card_ref"}, {"name": "run_id"}],
        "managed_candidates": [{"name": "wyrd_event_time"}],
        "physical_layout": {"partition_granularity": "hour"},
    }
    schema = pyarrow.schema(
        [
            pyarrow.field("trace_id", pyarrow.binary(), nullable=False),
            pyarrow.field("card_ref", pyarrow.string(), nullable=False),
            pyarrow.field("run_id", pyarrow.string(), nullable=True),
        ]
    )
    sink = pyarrow.BufferOutputStream()
    with pyarrow.ipc.new_stream(sink, schema):
        pass
    schema_only_ipc = sink.getvalue().to_pybytes()
    sent: list[tuple[str, bytes, bytes]] = []
    requested: list[tuple[str, bool]] = []

    class NativeClient:
        def writable_schema_ipc(self, description_json: str, include_event_time: bool) -> bytes:
            requested.append((description_json, include_event_time))
            return schema_only_ipc

        def insert_batch(self, table: str, batch_id: bytes, arrow_ipc: bytes) -> bytes:
            sent.append((table, batch_id, arrow_ipc))
            return batch_id

    client = object.__new__(BifrostQueryClient)
    client._native = NativeClient()
    batch_id = uuid.uuid4()

    async def run() -> tuple[pyarrow.Schema, uuid.UUID]:
        writable = await client.writable_schema(described)
        batch = pyarrow.record_batch(
            [
                pyarrow.array([b"\x01"], type=pyarrow.binary()),
                pyarrow.array(["space/Data/spans@1"]),
                pyarrow.array([None], type=pyarrow.string()),
            ],
            schema=writable,
        )
        return writable, await client.insert_batch("vala.traces.spans", batch_id, batch)

    writable, acked = asyncio.run(run())

    assert json.loads(requested[0][0])["user_fields"] == [{"name": "trace_id"}]
    assert requested[0][1] is False, "a writer supplies event time only when it asks to"
    assert writable.names == ["trace_id", "card_ref", "run_id"]
    assert acked == batch_id
    table, sent_id, ipc = sent[0]
    assert table == "vala.traces.spans"
    assert sent_id == batch_id.bytes
    assert pyarrow.ipc.open_stream(ipc).schema == writable, (
        "the batch travels on the described schema, not a rebuilt one"
    )
