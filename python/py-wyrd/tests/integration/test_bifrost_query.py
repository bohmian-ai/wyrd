"""Real Python SDK to Gate to Oracle query journey."""

import asyncio

import pyarrow
import pytest
from wyrd.bifrost import BifrostQueryClient
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
