"""Real Python SDK to Gate to Oracle query journey."""

import asyncio

import pyarrow
import pytest
from wyrd import WyrdError
from wyrd.bifrost import AsyncBifrost
from wyrd.testing import WyrdTestServer


@pytest.mark.integration
def test_bifrost_query_yields_pyarrow_and_terminal(
    wyrd_server: WyrdTestServer, query_table: str
) -> None:
    async def query() -> tuple[list[pyarrow.RecordBatch], dict[str, object] | None]:
        stream = await AsyncBifrost(
            server_url=wyrd_server.base_url, credential=wyrd_server.api_key
        ).stream(
            f"SELECT id, value FROM {query_table} ORDER BY id",
        )
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, terminal = asyncio.run(query())
    assert batches
    assert all(isinstance(batch, pyarrow.RecordBatch) for batch in batches)
    ids = [value for batch in batches for value in batch.column("id").to_pylist()]
    assert ids == [1, 2, 3]
    assert terminal is not None
    assert terminal.outcome == "success"
    assert terminal.row_count == 3
    assert terminal.warnings == []
    assert not hasattr(terminal, "freshness")


@pytest.mark.integration
def test_bifrost_query_out_of_range_deadline_is_the_shared_validation_error(
    wyrd_server: WyrdTestServer,
) -> None:
    async def query(deadline_ms: int) -> None:
        with pytest.raises(WyrdError) as captured:
            await AsyncBifrost(
                server_url=wyrd_server.base_url, credential=wyrd_server.api_key
            ).stream(
                "SELECT 1",
                deadline_ms=deadline_ms,
            )
        assert captured.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
        assert captured.value.status == 400

    for deadline_ms in (0, 2**32):
        asyncio.run(query(deadline_ms))
