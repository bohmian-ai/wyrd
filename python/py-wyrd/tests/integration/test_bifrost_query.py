"""Real Python SDK to Gate to Oracle query journey."""

import asyncio

import pyarrow
import pytest
from wyrd.bifrost import BifrostQueryClient, BifrostQueryError, IncompleteQueryStreamError
from wyrd.testing import WyrdTestServer


@pytest.mark.integration
def test_bifrost_query_yields_pyarrow_and_terminal(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()

    async def query() -> tuple[list[pyarrow.RecordBatch], dict[str, object] | None]:
        stream = await BifrostQueryClient(wyrd_server.base_url, token).query(
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
@pytest.mark.parametrize("fault", ["schema", "batch"])
def test_bifrost_query_missing_terminal_fails_closed(
    wyrd_server: WyrdTestServer, fault: str
) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    getattr(wyrd_server, f"fail_next_query_after_{fault}")()

    async def query() -> None:
        stream = await BifrostQueryClient(wyrd_server.base_url, token).query(
            f"SELECT id, value FROM {table_fqn} ORDER BY id",
        )
        with pytest.raises(IncompleteQueryStreamError, match="WYRD_VALA_502_QUERY_STREAM_INCOMPLETE"):
            while True:
                await stream.__anext__()
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
        with pytest.raises(BifrostQueryError, match="WYRD_PERMISSION_403_DENIED_RBAC"):
            await BifrostQueryClient(wyrd_server.base_url, denied_token).query(
                f"SELECT * FROM {table_fqn}",
            )

    asyncio.run(query())
    assert wyrd_server.bifrost_read_decision_count() == before
