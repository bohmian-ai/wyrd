"""A caller queries Bifrost with bound parameters, as model rows or as an Arrow stream.

``query_table`` holds rows 1-3 written through the SDK.
"""

import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from .support import Count, QueryRow

pytestmark = pytest.mark.integration


def test_parameterized_sql_returns_the_callers_rows(bifrost: Bifrost, query_table: str) -> None:
    rows = bifrost.sql(
        f"SELECT id, value FROM {query_table} WHERE id >= $1 ORDER BY id", [2], model=QueryRow
    )
    assert rows == [QueryRow(id=2, value="two"), QueryRow(id=3, value="three")]


def test_bound_sql_text_is_treated_as_data(bifrost: Bifrost, query_table: str) -> None:
    rows = bifrost.sql(
        f"SELECT id, value FROM {query_table} WHERE value = $1", ["one' OR '1'='1"], model=QueryRow
    )
    assert rows == []


def test_unwritten_builtin_table_reads_as_empty(bifrost: Bifrost) -> None:
    [counted] = bifrost.sql("SELECT COUNT(*) AS n FROM vala.eval.result_items", model=Count)
    assert counted.n == 0


def test_stream_yields_arrow_batches_and_a_terminal(bifrost: Bifrost, query_table: str) -> None:
    batches = bifrost.stream(f"SELECT id FROM {query_table} ORDER BY id")

    ids = [id for batch in batches for id in batch.column("id").to_pylist()]

    assert ids == [1, 2, 3]
    assert batches.terminal is not None
    assert (batches.terminal.outcome, batches.terminal.row_count, batches.terminal.warnings) == (
        "success",
        3,
        [],
    )


def test_caller_without_bifrost_read_is_refused(
    wyrd_server: WyrdTestServer, query_table: str
) -> None:
    denied = Bifrost(
        client=WyrdClient(credential=wyrd_server.scoped_api_key("query_denied", ["cards:read"]))
    )

    with pytest.raises(WyrdError) as refused:
        denied.sql(f"SELECT id FROM {query_table}")
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
