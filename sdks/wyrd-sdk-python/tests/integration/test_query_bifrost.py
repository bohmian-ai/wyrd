"""A caller queries Bifrost with bound parameters, as model rows or as an Arrow stream.

``query_table`` holds rows 1-3 written through the SDK and published;
``live_table`` holds the same rows acknowledged by Scribe and never published,
rows 1 and 2 under one managed ``run_id`` and row 3 under another.
"""

import uuid

import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from .support import Count, QueryRow

pytestmark = pytest.mark.integration

LIVE_ROWS = "vala.datasets.live_query_rows"


@pytest.fixture(scope="module")
def live_table(wyrd_server: WyrdTestServer) -> str:
    """A ``vala.datasets`` table holding rows 1-3 that only Scribe's live tail serves.

    Rows 1 and 2 share one managed ``run_id`` and row 3 carries another.
    """
    writer = Bifrost(TableConfig(QueryRow, LIVE_ROWS))
    writer.register()
    shared, other = str(uuid.uuid4()), str(uuid.uuid4())
    for row_id, value, run_id in ((1, "one", shared), (2, "two", shared), (3, "three", other)):
        writer.insert({"id": row_id, "value": value}, {"run_id": run_id})
    writer.flush()
    writer.shutdown()
    return LIVE_ROWS


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


def ids(bifrost: Bifrost, sql: str) -> list[int]:
    """The ``id`` column of every row ``sql`` returns, in order."""
    return [id for batch in bifrost.stream(sql) for id in batch.column("id").to_pylist()]


@pytest.mark.parametrize(
    ("subquery", "join", "expected"),
    [
        (
            f"SELECT id FROM {LIVE_ROWS} WHERE id = "
            f"(SELECT id FROM {LIVE_ROWS} ORDER BY id DESC LIMIT 1)",
            f"SELECT a.id FROM {LIVE_ROWS} a JOIN (SELECT MAX(id) AS id FROM {LIVE_ROWS}) b "
            "ON a.id = b.id",
            [3],
        ),
        (
            f"SELECT id FROM {LIVE_ROWS} WHERE id IN "
            f"(SELECT id FROM {LIVE_ROWS} WHERE value <> 'two') ORDER BY id",
            f"SELECT a.id FROM {LIVE_ROWS} a JOIN {LIVE_ROWS} b ON a.id = b.id "
            "WHERE b.value <> 'two' ORDER BY a.id",
            [1, 3],
        ),
        (
            f"SELECT a.id FROM {LIVE_ROWS} a WHERE EXISTS "
            f"(SELECT 1 FROM {LIVE_ROWS} b WHERE b.id = a.id + 1) ORDER BY a.id",
            f"SELECT a.id FROM {LIVE_ROWS} a JOIN {LIVE_ROWS} b ON b.id = a.id + 1 ORDER BY a.id",
            [1, 2],
        ),
        (
            f"SELECT a.id FROM {LIVE_ROWS} a WHERE EXISTS "
            f"(SELECT 1 FROM {LIVE_ROWS} b WHERE b.run_id = a.run_id AND b.id <> a.id) "
            "ORDER BY a.id",
            f"SELECT DISTINCT a.id FROM {LIVE_ROWS} a JOIN {LIVE_ROWS} b "
            "ON b.run_id = a.run_id AND b.id <> a.id ORDER BY a.id",
            [1, 2],
        ),
    ],
    ids=["scalar", "in", "exists", "managed_run_id"],
)
def test_live_table_subqueries_match_their_joins(
    bifrost: Bifrost, live_table: str, subquery: str, join: str, expected: list[int]
) -> None:
    assert ids(bifrost, subquery) == expected
    assert ids(bifrost, join) == expected


@pytest.mark.parametrize(
    "sql",
    [
        f"SELECT r.valeu FROM {LIVE_ROWS} r",
        f"SELECT r.valeu, r.valeu AS again FROM {LIVE_ROWS} r JOIN {LIVE_ROWS} s ON r.id = s.id "
        "JOIN vala.verification.results v ON v.result_id = s.value",
        f"SELECT id FROM {LIVE_ROWS} WHERE id IN (SELECT r.valeu FROM {LIVE_ROWS} r)",
    ],
    ids=["single", "join", "subquery"],
)
def test_unknown_column_carries_the_planners_suggestion(
    bifrost: Bifrost, live_table: str, sql: str
) -> None:
    with pytest.raises(WyrdError) as refused:
        bifrost.sql(sql)
    assert refused.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
    assert "No field named r.valeu. Did you mean 'r.value'?" in refused.value.detail


@pytest.mark.parametrize(
    ("sql", "detail"),
    [
        (
            f"SELECT value + 1 AS n FROM {LIVE_ROWS}",
            "Cannot coerce arithmetic expression Utf8 + Int64",
        ),
        (
            f"SELECT id FROM {LIVE_ROWS} a JOIN {LIVE_ROWS} b ON a.id = b.id",
            "Ambiguous reference to unqualified field id",
        ),
    ],
    ids=["type_mismatch", "ambiguous_reference"],
)
def test_type_mismatch_and_ambiguous_reference_carry_the_planners_detail(
    bifrost: Bifrost, live_table: str, sql: str, detail: str
) -> None:
    with pytest.raises(WyrdError) as refused:
        bifrost.sql(sql)
    assert refused.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
    assert detail in refused.value.detail


def test_unauthorized_table_refusal_reveals_no_columns(
    wyrd_server: WyrdTestServer, live_table: str
) -> None:
    datasets_only = {
        "resource": "bifrost_query",
        "action": "read",
        "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "datasets"}}},
    }
    reader = Bifrost(
        client=WyrdClient(credential=wyrd_server.scoped_api_key("datasets_reader", [datasets_only]))
    )
    assert ids(reader, f"SELECT id FROM {live_table} ORDER BY id") == [1, 2, 3]

    with pytest.raises(WyrdError) as refused:
        reader.sql(
            f"SELECT q.id, v.verdcit FROM {live_table} q "
            "JOIN vala.verification.results v ON v.result_id = q.value"
        )
    assert refused.value.code == "WYRD_VALA_403_QUERY_FORBIDDEN"
    assert "verdcit" not in refused.value.detail and "verdict" not in refused.value.detail
