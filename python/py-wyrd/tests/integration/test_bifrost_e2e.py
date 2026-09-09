"""Integration journeys for the one public Bifrost client."""

from __future__ import annotations

import asyncio
import json
import os
import uuid
from typing import TYPE_CHECKING

import pytest
from pydantic import BaseModel
from wyrd.bifrost import (
    AsyncBifrost,
    Bifrost,
    BifrostQueryError,
    NoCredentialsError,
    TableConfig,
)
from wyrd.observe import record

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer


class Prompt(BaseModel):
    """The user columns the telemetry journeys declare."""

    model: str
    tokens: int
    score: float


class Fixture(BaseModel):
    """The user columns of the server-prepared Oracle fixture table."""

    id: int
    value: str


# The harness API key is scoped to this deterministic service card.
CARD_REF = "test/Service/python-integration-writer@1.0.0"

SCHEMA_JSON = json.dumps(Prompt.model_json_schema())


def _row(i: int) -> Prompt:
    return Prompt(model="claude-opus-4-8", tokens=i, score=0.5)


def _client(server: WyrdTestServer, table: str) -> Bifrost:
    """Connect a writer bound to `table`, using the harness's write key."""

    return Bifrost(
        TableConfig(Prompt, table),
        server_url=server.base_url,
        credential=server.api_key,
    )


def _fixture_client(server: WyrdTestServer, table_fqn: str, credential: str) -> Bifrost:
    """Connect a writer bound to the server-prepared fixture table."""

    return Bifrost(
        TableConfig(Fixture, table_fqn),
        server_url=server.base_url,
        credential=credential,
    )


def _read(server: WyrdTestServer, token: str, sql: str) -> list[tuple[int, str]]:
    """Read `sql` back through the async client and flatten it to tuples."""

    async def readback() -> list[tuple[int, str]]:
        client = AsyncBifrost(server_url=server.base_url, credential=token)
        result = await client.sql(sql)
        table = result.to_arrow()
        return list(
            zip(
                table.column("id").to_pylist(),
                table.column("value").to_pylist(),
                strict=True,
            )
        )

    return asyncio.run(readback())


# ---------------------------------------------------------------------------
# bootstrap_service pymethod
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bootstrap_service_mints_api_key(wyrd_server: WyrdTestServer) -> None:
    key = wyrd_server.bootstrap_service(["writer"], name="svc-test")
    assert isinstance(key, str) and len(key) > 0, "expected non-empty API key string"


@pytest.mark.integration
def test_bootstrap_service_empty_permissions_returns_key(wyrd_server: WyrdTestServer) -> None:
    key = wyrd_server.bootstrap_service([], name="svc-no-perms")
    assert isinstance(key, str) and len(key) > 0, "underprivileged principal still gets a key"


# ---------------------------------------------------------------------------
# Transport resolution
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_omitted_transport_resolves_from_the_environment(wyrd_server: WyrdTestServer) -> None:
    """No arguments at all still reaches the harness server and writes.

    ``WyrdTestServer(mutate_env=True)`` exports the endpoint and credential the
    chain reads, so this is the real "just construct it" path a user takes.
    """

    bifrost = Bifrost(TableConfig(Prompt, "genai.resolved"))
    bifrost.insert(_row(0), {"card_ref": CARD_REF})
    assert bifrost.dropped == 0


@pytest.mark.integration
def test_no_resolvable_credential_raises(wyrd_server: WyrdTestServer) -> None:
    """An empty chain names the failure instead of connecting anonymously.

    The harness exports its credential from Rust, so ``os.environ`` does not
    mirror it and ``monkeypatch`` cannot reach it; the process environment is
    cleared and restored directly.
    """

    home = os.environ.get("HOME")
    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"):
        os.unsetenv(name)
        os.environ.pop(name, None)
    os.environ["HOME"] = "/nonexistent-wyrd-home"
    try:
        with pytest.raises(NoCredentialsError) as captured:
            Bifrost()
        assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"
    finally:
        os.environ["WYRD_API_KEY"] = wyrd_server.api_key
        if home is not None:
            os.environ["HOME"] = home


# ---------------------------------------------------------------------------
# Happy-path lifecycle — both write surfaces
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bifrost_insert_no_drops(wyrd_server: WyrdTestServer) -> None:
    bifrost = _client(wyrd_server, "genai.prompts")
    for i in range(100):
        bifrost.insert(_row(i), {"card_ref": CARD_REF})
    assert bifrost.dropped == 0, "happy path: no drops"
    assert bifrost.producer_count == 1


@pytest.mark.integration
def test_insert_without_an_active_table_refuses(wyrd_server: WyrdTestServer) -> None:
    """A write with nothing bound fails rather than landing somewhere."""

    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=wyrd_server.api_key)
    with pytest.raises(BifrostQueryError) as captured:
        bifrost.insert(_row(0), {"card_ref": CARD_REF})
    assert captured.value.code == "WYRD_VALA_412_NO_ACTIVE_TABLE"
    assert captured.value.status == 412
    assert bifrost.producer_count == 0


@pytest.mark.integration
def test_observe_record_no_raises(wyrd_server: WyrdTestServer) -> None:
    bifrost = _client(wyrd_server, "genai.prompts")
    for i in range(100):
        record(
            bifrost,
            "genai.observe",
            SCHEMA_JSON,
            _row(i).model_dump_json(),
            {"card_ref": CARD_REF},
        )
    assert bifrost.dropped == 0


@pytest.mark.integration
def test_register_insert_flush_read_and_swap(wyrd_server: WyrdTestServer) -> None:
    """The whole journey: register, write, flush, read, swap, write, read."""

    # `vala.datasets` is the caller-owned namespace, so this is the one table
    # a user actually registers; the built-in namespaces are server-owned.
    own_fqn = f"vala.datasets.journey_{uuid.uuid4().hex}"
    bifrost = _fixture_client(wyrd_server, own_fqn, wyrd_server.api_key)

    assert bifrost.register() == "created"
    # Re-registering the same columns is a match, not a conflict.
    assert bifrost.register() == "already_exists"
    resolved = bifrost.table.resolved
    assert resolved is not None
    assert resolved["table_uid"] and resolved["fingerprint"]

    written = [(4, "fourth"), (5, "fifth")]
    for row_id, value in written:
        bifrost.insert({"id": row_id, "value": value}, {"card_ref": CARD_REF})

    # Swap the active table before flushing: the swapped-away producer must
    # still drain, so the rows above are not stranded by the rebinding.
    second_fqn, second_token = wyrd_server.prepare_oracle_query_fixture()
    bifrost.use_table_by_name(second_fqn)
    assert bifrost.table is not None
    assert bifrost.table.fqn == second_fqn
    bifrost.insert({"id": 7, "value": "second-table"}, {"card_ref": CARD_REF})
    assert bifrost.producer_count == 2, "the swapped-away producer is still pooled"

    bifrost.flush()
    bifrost.shutdown()
    wyrd_server.flush_bifrost()

    assert (
        _read(
            wyrd_server,
            wyrd_server.api_key,
            f"SELECT id, value FROM {own_fqn} ORDER BY id",
        )
        == written
    )
    assert _read(wyrd_server, second_token, f"SELECT id, value FROM {second_fqn} WHERE id = 7") == [
        (7, "second-table")
    ]


@pytest.mark.integration
def test_sql_and_stream_return_the_same_rows(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    query = f"SELECT id, value FROM {table_fqn} ORDER BY id"

    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=token)
    collected = bifrost.sql(query).to_arrow()
    streamed = list(bifrost.stream(query))

    assert collected.num_rows == sum(batch.num_rows for batch in streamed)
    assert collected.column("id").to_pylist() == [
        value for batch in streamed for value in batch.column("id").to_pylist()
    ]


@pytest.mark.integration
def test_describe_binds_an_existing_table_without_restating_its_schema(
    wyrd_server: WyrdTestServer,
) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()

    described = TableConfig.describe(
        table_fqn,
        server_url=wyrd_server.base_url,
        credential=wyrd_server.api_key,
    )
    assert described.fqn == table_fqn
    assert described.arrow_schema.names == ["id", "value"]
    assert described.resolved is not None

    bifrost = Bifrost(
        described,
        server_url=wyrd_server.base_url,
        credential=wyrd_server.api_key,
    )
    bifrost.insert({"id": 6, "value": "described"}, {"card_ref": CARD_REF})
    bifrost.flush()
    bifrost.shutdown()
    wyrd_server.flush_bifrost()

    assert _read(wyrd_server, token, f"SELECT id, value FROM {table_fqn} WHERE id = 6") == [
        (6, "described")
    ]


@pytest.mark.integration
def test_uncorrelated_row_is_a_valid_write(wyrd_server: WyrdTestServer) -> None:
    """An omitted card_ref writes; the server correlates to the principal."""

    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    bifrost = _fixture_client(wyrd_server, table_fqn, wyrd_server.api_key)
    bifrost.insert({"id": 8, "value": "uncorrelated"})
    bifrost.flush()
    bifrost.shutdown()
    wyrd_server.flush_bifrost()

    assert _read(wyrd_server, token, f"SELECT id, value FROM {table_fqn} WHERE id = 8") == [
        (8, "uncorrelated")
    ]


# ---------------------------------------------------------------------------
# Backpressure contracts
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bifrost_insert_propagates_queue_full(wyrd_server: WyrdTestServer) -> None:
    bifrost = _client(wyrd_server, "genai.backpressure")

    # The public transport drains this small batch quickly; queue-full remains
    # covered by the native queue tests where a stall sink is deterministic.
    for i in range(500):
        bifrost.insert(_row(i), {"card_ref": CARD_REF})
    assert bifrost.dropped == 0, "the explicit insert path never drops silently"


@pytest.mark.integration
def test_observe_record_swallows_and_counts(wyrd_server: WyrdTestServer) -> None:
    bifrost = _client(wyrd_server, "genai.prompts")
    for i in range(200):
        record(
            bifrost,
            "genai.observe_overflow",
            SCHEMA_JSON,
            _row(i).model_dump_json(),
            {"card_ref": CARD_REF},
        )
    assert bifrost.dropped == 0, "public transport drains immediately; no spurious drops expected"


# ---------------------------------------------------------------------------
# Negative journeys
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_negative_bad_card_ref_raises(wyrd_server: WyrdTestServer) -> None:
    bifrost = _client(wyrd_server, "genai.bad")
    with pytest.raises(ValueError):
        bifrost.insert(_row(0), {"card_ref": "not-a-ref"})


@pytest.mark.integration
def test_negative_reserved_column_is_refused_locally() -> None:
    """A server-owned column is named before a round trip, not after."""

    class Reserved(BaseModel):
        card_ref: str

    with pytest.raises(ValueError, match="card_ref"):
        TableConfig(Reserved, "genai.reserved")


@pytest.mark.integration
def test_negative_empty_permissions_denied_rbac_on_write(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    denied_key = wyrd_server.bootstrap_service([], name="bifrost-write-denied")
    bifrost = _fixture_client(wyrd_server, table_fqn, denied_key)
    bifrost.insert({"id": 999, "value": "denied"}, {"card_ref": CARD_REF})
    with pytest.raises(BifrostQueryError, match="WYRD_PERMISSION_403_DENIED_RBAC"):
        bifrost.flush()
    # A denial is terminal, not ambiguous: the refused batch is not retained for
    # retry, so the caller is told once, at the boundary that carried the write,
    # and the shutdown that follows has nothing left to send.
    bifrost.shutdown()

    assert _read(wyrd_server, token, f"SELECT id, value FROM {table_fqn} WHERE id = 999") == [], (
        "denied write must not mutate durable rows"
    )


@pytest.mark.integration
def test_registering_a_different_schema_on_one_name_conflicts(
    wyrd_server: WyrdTestServer,
) -> None:
    """A second declaration of the same table is a stable conflict, not a silent evolution."""

    fqn = f"vala.datasets.conflict_{uuid.uuid4().hex}"
    assert _fixture_client(wyrd_server, fqn, wyrd_server.api_key).register() == "created"

    # `Prompt` declares model/tokens/score where `Fixture` declared id/value:
    # same name, different columns, so the server refuses rather than evolving.
    conflicting = Bifrost(
        TableConfig(Prompt, fqn),
        server_url=wyrd_server.base_url,
        credential=wyrd_server.api_key,
    )
    with pytest.raises(BifrostQueryError) as captured:
        conflicting.register()
    assert captured.value.code == "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
    assert captured.value.status == 409
    assert conflicting.table.resolved is None, "a refused registration mints no identity"


@pytest.mark.integration
def test_negative_non_select_query_is_refused(wyrd_server: WyrdTestServer) -> None:
    """The read plane is read-only: a mutation never reaches execution."""

    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=token)

    with pytest.raises(BifrostQueryError) as captured:
        bifrost.sql(f"DELETE FROM {table_fqn}")
    assert captured.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
    assert captured.value.status == 400


@pytest.mark.integration
def test_negative_oversized_query_is_refused(wyrd_server: WyrdTestServer) -> None:
    """A statement past the SQL byte ceiling is refused before it is planned."""

    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=token)

    # The floor is 64 KiB of SQL; pad a valid SELECT past it with a comment so
    # the refusal is about size rather than syntax.
    oversized = f"SELECT id FROM {table_fqn} -- {'x' * (64 * 1024)}"
    with pytest.raises(BifrostQueryError) as captured:
        bifrost.sql(oversized)
    assert captured.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
    assert captured.value.status == 400


@pytest.mark.integration
def test_negative_invalid_sql_query(wyrd_server: WyrdTestServer) -> None:
    _table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=token)

    with pytest.raises(BifrostQueryError) as captured:
        bifrost.sql("SELECT FROM")
    assert captured.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
    assert captured.value.status == 400
    assert captured.value.detail


# ---------------------------------------------------------------------------
# Audit trail
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_positive_audit_trail(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    before = wyrd_server.bifrost_read_decision_count()

    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=token)
    result = bifrost.sql(f"SELECT id FROM {table_fqn} ORDER BY id")
    assert result.terminal["outcome"] == "success"

    # The read acceptance is fsynced locally and relayed to the canonical
    # outbox by a background task, so converge the relay before counting.
    assert wyrd_server.wait_oracle_audit_relayed() == 0
    assert wyrd_server.bifrost_read_decision_count() > before


# ---------------------------------------------------------------------------
# Analytical read journeys — the shapes a data scientist actually writes
# ---------------------------------------------------------------------------


class Inference(BaseModel):
    """One served inference: the fact table the analytical journeys aggregate."""

    call_id: int
    model: str
    tokens: int
    latency_ms: float
    status: str


class ModelInfo(BaseModel):
    """Model dimension rows, joined against `Inference` by model name."""

    model: str
    vendor: str


INFERENCES = [
    Inference(call_id=1, model="opus", tokens=100, latency_ms=120.5, status="ok"),
    Inference(call_id=2, model="opus", tokens=300, latency_ms=240.0, status="ok"),
    Inference(call_id=3, model="opus", tokens=200, latency_ms=180.25, status="error"),
    Inference(call_id=4, model="haiku", tokens=50, latency_ms=30.0, status="ok"),
    Inference(call_id=5, model="haiku", tokens=150, latency_ms=60.75, status="ok"),
]

MODEL_INFO = [
    ModelInfo(model="opus", vendor="anthropic"),
    ModelInfo(model="haiku", vendor="anthropic"),
]


def _publish(server: WyrdTestServer, model: type[BaseModel], rows: list[BaseModel]) -> str:
    """Register a fresh caller-owned table, write `rows`, and publish them.

    Returns the table's fully qualified name, queryable by the harness key.
    `vala.datasets` is the caller-owned namespace, so it is the one place a
    user registers; the write is only readable after both the client drains
    and the server-owned Scribe publishes.
    """

    fqn = f"vala.datasets.{model.__name__.lower()}_{uuid.uuid4().hex}"
    bifrost = Bifrost(
        TableConfig(model, fqn),
        server_url=server.base_url,
        credential=server.api_key,
    )
    assert bifrost.register() == "created"
    for row in rows:
        bifrost.insert(row, {"card_ref": CARD_REF})
    bifrost.flush()
    bifrost.shutdown()
    server.flush_bifrost()
    return fqn


@pytest.mark.integration
def test_read_back_as_arrow_pandas_and_polars(wyrd_server: WyrdTestServer) -> None:
    """One written table, read back through all three dataframe conversions.

    The batches cross the boundary once as an Arrow IPC stream, so the three
    conversions must agree on rows, column order, and values — a divergence
    here means a conversion, not the query, lost data.
    """

    fqn = _publish(wyrd_server, Inference, list(INFERENCES))
    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=wyrd_server.api_key)
    result = bifrost.sql(
        f"SELECT call_id, model, tokens, latency_ms, status FROM {fqn} ORDER BY call_id"
    )

    arrow = result.to_arrow()
    assert arrow.num_rows == len(INFERENCES)
    assert arrow.column_names == ["call_id", "model", "tokens", "latency_ms", "status"]
    assert arrow.column("call_id").to_pylist() == [row.call_id for row in INFERENCES]
    assert arrow.column("latency_ms").to_pylist() == [row.latency_ms for row in INFERENCES]

    pandas_frame = result.to_pandas()
    assert list(pandas_frame.columns) == arrow.column_names
    assert pandas_frame["call_id"].tolist() == [row.call_id for row in INFERENCES]
    assert pandas_frame["model"].tolist() == [row.model for row in INFERENCES]

    polars_frame = result.to_polars()
    assert polars_frame.columns == arrow.column_names
    assert polars_frame.height == arrow.num_rows
    assert polars_frame.to_dicts() == pandas_frame.to_dict(orient="records")


@pytest.mark.integration
def test_analytical_sql_over_written_tables(wyrd_server: WyrdTestServer) -> None:
    """Group-by, window, join, and scalar SQL over caller-registered tables.

    Representative rather than exhaustive: one query per DataFusion family a
    data scientist reaches for, each asserted on values rather than shape so a
    silently wrong plan fails.
    """

    facts = _publish(wyrd_server, Inference, list(INFERENCES))
    dims = _publish(wyrd_server, ModelInfo, list(MODEL_INFO))
    bifrost = Bifrost(server_url=wyrd_server.base_url, credential=wyrd_server.api_key)

    # Aggregates with a HAVING filter: the per-group summary table.
    grouped = bifrost.sql(
        f"""
        SELECT model,
               COUNT(*) AS runs,
               SUM(tokens) AS total_tokens,
               AVG(tokens) AS avg_tokens,
               MIN(latency_ms) AS fastest,
               MAX(latency_ms) AS slowest
        FROM {facts}
        GROUP BY model
        HAVING COUNT(*) > 1
        ORDER BY total_tokens DESC
        """
    ).to_arrow()
    assert grouped.column("model").to_pylist() == ["opus", "haiku"]
    assert grouped.column("runs").to_pylist() == [3, 2]
    assert grouped.column("total_tokens").to_pylist() == [600, 200]
    assert grouped.column("avg_tokens").to_pylist() == [200.0, 100.0]
    assert grouped.column("fastest").to_pylist() == [120.5, 30.0]
    assert grouped.column("slowest").to_pylist() == [240.0, 60.75]

    # Window functions: rank within a partition, a running total, and a lag.
    windowed = bifrost.sql(
        f"""
        SELECT call_id,
               model,
               ROW_NUMBER() OVER (PARTITION BY model ORDER BY tokens DESC) AS rank_in_model,
               SUM(tokens) OVER (PARTITION BY model ORDER BY call_id) AS running_tokens,
               LAG(tokens) OVER (PARTITION BY model ORDER BY call_id) AS prev_tokens
        FROM {facts}
        ORDER BY call_id
        """
    ).to_arrow()
    assert windowed.column("rank_in_model").to_pylist() == [3, 1, 2, 2, 1]
    assert windowed.column("running_tokens").to_pylist() == [100, 400, 600, 50, 200]
    assert windowed.column("prev_tokens").to_pylist() == [None, 100, 300, None, 50]

    # Join to the dimension table, with a filtering aggregate over the fact side.
    joined = bifrost.sql(
        f"""
        SELECT d.vendor,
               f.model,
               COUNT(*) FILTER (WHERE f.status = 'ok') AS successes,
               COUNT(*) AS attempts
        FROM {facts} AS f
        INNER JOIN {dims} AS d ON f.model = d.model
        GROUP BY d.vendor, f.model
        ORDER BY f.model
        """
    ).to_arrow()
    assert joined.column("vendor").to_pylist() == ["anthropic", "anthropic"]
    assert joined.column("model").to_pylist() == ["haiku", "opus"]
    assert joined.column("successes").to_pylist() == [2, 2]
    assert joined.column("attempts").to_pylist() == [2, 3]

    # Scalar expressions over a CTE: the reshaping step before a chart.
    scalars = bifrost.sql(
        f"""
        WITH labelled AS (
            SELECT call_id,
                   UPPER(model) AS model_label,
                   ROUND(latency_ms) AS latency_whole,
                   CASE WHEN latency_ms > 100 THEN 'slow' ELSE 'fast' END AS bucket,
                   CHARACTER_LENGTH(status) AS status_len
            FROM {facts}
        )
        SELECT * FROM labelled ORDER BY call_id LIMIT 3
        """
    ).to_arrow()
    assert scalars.num_rows == 3
    assert scalars.column("model_label").to_pylist() == ["OPUS", "OPUS", "OPUS"]
    assert scalars.column("latency_whole").to_pylist() == [121.0, 240.0, 180.0]
    assert scalars.column("bucket").to_pylist() == ["slow", "slow", "slow"]
    assert scalars.column("status_len").to_pylist() == [2, 2, 5]
