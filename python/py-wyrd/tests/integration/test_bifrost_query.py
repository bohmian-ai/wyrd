"""Real Python SDK to Gate to Oracle query journey."""

import asyncio

import pyarrow
import pytest
from wyrd.bifrost import AsyncBifrost, BifrostQueryError, IncompleteQueryStreamError
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
        with pytest.raises(IncompleteQueryStreamError) as captured:
            while True:
                await stream.__anext__()
        assert captured.value.code == "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"
        assert captured.value.status == 502
        assert captured.value.title == "Query stream incomplete"
        assert captured.value.message == captured.value.detail
        assert captured.value.remediation
        assert captured.value.details is None
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
        with pytest.raises(BifrostQueryError) as captured:
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
