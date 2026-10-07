"""A caller queries Bifrost with bound parameters, synchronously or as an async Arrow stream.

``query_table`` holds rows 1-3 written through the SDK.
"""

import asyncio

import pyarrow
import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import AsyncBifrost, Bifrost, QueryTerminal
from wyrd.testing import WyrdTestServer

from .conftest import QueryRow


class Count(BaseModel):
    """One ``COUNT(*)`` result."""

    n: int


@pytest.mark.integration
def test_parameterized_sql_returns_the_callers_rows(bifrost: Bifrost, query_table: str) -> None:
    rows = bifrost.sql(
        f"SELECT id, value FROM {query_table} WHERE id >= $1 ORDER BY id", [2], model=QueryRow
    )
    assert rows == [QueryRow(id=2, value="two"), QueryRow(id=3, value="three")]


@pytest.mark.integration
def test_bound_sql_text_is_treated_as_data(bifrost: Bifrost, query_table: str) -> None:
    rows = bifrost.sql(
        f"SELECT id, value FROM {query_table} WHERE value = $1", ["one' OR '1'='1"], model=QueryRow
    )
    assert rows == []


@pytest.mark.integration
def test_unwritten_builtin_table_reads_as_empty(bifrost: Bifrost) -> None:
    [counted] = bifrost.sql("SELECT COUNT(*) AS n FROM vala.eval.result_items", model=Count)
    assert counted.n == 0


@pytest.mark.integration
def test_async_stream_yields_arrow_batches_and_a_terminal(query_table: str) -> None:
    async def stream() -> tuple[list[pyarrow.RecordBatch], QueryTerminal | None]:
        batches = await AsyncBifrost().stream(f"SELECT id FROM {query_table} ORDER BY id")
        return [batch async for batch in batches], batches.terminal

    batches, terminal = asyncio.run(stream())
    assert [id for batch in batches for id in batch.column("id").to_pylist()] == [1, 2, 3]
    assert terminal is not None
    assert (terminal.outcome, terminal.row_count) == ("success", 3)


@pytest.mark.integration
@pytest.mark.parametrize("deadline_ms", [0, 2**32])
def test_out_of_range_deadline_is_refused(wyrd_server: WyrdTestServer, deadline_ms: int) -> None:
    async def stream() -> None:
        await AsyncBifrost().stream("SELECT 1", deadline_ms=deadline_ms)

    with pytest.raises(WyrdError) as raised:
        asyncio.run(stream())
    assert raised.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
