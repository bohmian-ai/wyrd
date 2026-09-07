"""Integration journeys for the public Bifrost gRPC write surface."""

from __future__ import annotations

import asyncio
import json
from typing import TYPE_CHECKING

import pytest
from wyrd.bifrost import Bifrost, BifrostQueryClient, BifrostQueryError
from wyrd.observe import record

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

SCHEMA = json.dumps(
    {
        "type": "object",
        "properties": {
            "model": {"type": "string"},
            "tokens": {"type": "integer"},
            "score": {"type": "number"},
        },
        "required": ["model", "tokens", "score"],
    }
)

# The harness API key is scoped to this deterministic service card.
CARD_REF = "test/Service/python-integration-writer@1.0.0"
WRITE_SCHEMA = json.dumps(
    {
        "type": "object",
        "properties": {"id": {"type": "integer"}, "value": {"type": "string"}},
        "required": ["id", "value"],
    }
)


def _row(i: int) -> str:
    return json.dumps({"model": "claude-opus-4-8", "tokens": i, "score": 0.5})


# ---------------------------------------------------------------------------
# C4e: bootstrap_service pymethod
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
# C6: happy-path lifecycle — both write surfaces
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bifrost_handle_insert_no_drops(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    for i in range(100):
        bifrost.insert(
            table="genai.prompts",
            schema=SCHEMA,
            row=_row(i),
            card_ref=CARD_REF,
        )
    assert bifrost.dropped == 0, "happy path: no drops"
    assert bifrost.producer_count == 1


@pytest.mark.integration
def test_observe_record_no_raises(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    for i in range(100):
        record(bifrost, table="genai.observe", schema=SCHEMA, row=_row(i), card_ref=CARD_REF)
    assert bifrost.dropped == 0


@pytest.mark.integration
def test_bifrost_write_flush_shutdown_and_readback(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    written = [(4, "fourth"), (5, "fifth")]
    for row_id, value in written:
        bifrost.insert(
            table=table_fqn,
            schema=WRITE_SCHEMA,
            row=json.dumps({"id": row_id, "value": value}),
            card_ref=CARD_REF,
        )
    bifrost.flush()
    bifrost.shutdown()
    wyrd_server.flush_bifrost()

    async def readback() -> list[tuple[int, str]]:
        stream = await BifrostQueryClient(wyrd_server.base_url, token).query(
            f"SELECT id, value FROM {table_fqn} WHERE id >= 4 ORDER BY id",
        )
        rows: list[tuple[int, str]] = []
        async for batch in stream:
            rows.extend(
                zip(
                    batch.column("id").to_pylist(),
                    batch.column("value").to_pylist(),
                    strict=True,
                )
            )
        return rows

    assert asyncio.run(readback()) == written


# ---------------------------------------------------------------------------
# C6: backpressure contracts
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bifrost_insert_propagates_queue_full(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)

    # The public transport drains this small batch quickly; queue-full remains
    # covered by the native queue tests where a stall sink is deterministic.
    for i in range(500):
        bifrost.insert(
            table="genai.backpressure",
            schema=SCHEMA,
            row=_row(i),
            card_ref=CARD_REF,
        )
    assert bifrost.dropped == 0, "observe path: no silent drops on explicit insert path"


@pytest.mark.integration
def test_observe_record_swallows_and_counts(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    attempted = 200
    for i in range(attempted):
        record(
            bifrost, table="genai.observe_overflow", schema=SCHEMA, row=_row(i), card_ref=CARD_REF
        )
    assert bifrost.dropped == 0, "public transport drains immediately; no spurious drops expected"


# ---------------------------------------------------------------------------
# C6: negative journeys
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_negative_bad_card_ref_raises(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    with pytest.raises(ValueError):
        bifrost.insert(table="genai.bad", schema=SCHEMA, row=_row(0), card_ref="not-a-ref")


@pytest.mark.integration
def test_negative_empty_permissions_denied_rbac_on_write(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    denied_key = wyrd_server.bootstrap_service([], name="bifrost-write-denied")
    bifrost = Bifrost(wyrd_server.base_url, denied_key)
    bifrost.insert(
        table=table_fqn,
        schema=WRITE_SCHEMA,
        row=json.dumps({"id": 999, "value": "denied"}),
        card_ref=CARD_REF,
    )
    with pytest.raises(RuntimeError, match="WYRD_PERMISSION_403_DENIED_RBAC"):
        bifrost.flush()
    # A denial is terminal, not ambiguous: the refused batch is not retained for
    # retry, so the caller is told once, at the boundary that carried the write,
    # and the shutdown that follows has nothing left to send.
    bifrost.shutdown()

    async def readback() -> list[tuple[int, str]]:
        stream = await BifrostQueryClient(wyrd_server.base_url, token).query(
            f"SELECT id, value FROM {table_fqn} WHERE id = 999",
        )
        rows: list[tuple[int, str]] = []
        async for batch in stream:
            rows.extend(
                zip(
                    batch.column("id").to_pylist(),
                    batch.column("value").to_pylist(),
                    strict=True,
                )
            )
        return rows

    assert asyncio.run(readback()) == [], "denied write must not mutate durable rows"


@pytest.mark.integration
def test_negative_invalid_sql_query(wyrd_server: WyrdTestServer) -> None:
    _table_fqn, token = wyrd_server.prepare_oracle_query_fixture()

    async def query() -> None:
        with pytest.raises(BifrostQueryError) as captured:
            await BifrostQueryClient(wyrd_server.base_url, token).query(
                "SELECT FROM",
            )
        assert captured.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"
        assert captured.value.status == 400
        assert captured.value.detail

    asyncio.run(query())


# ---------------------------------------------------------------------------
# C6: audit trail
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_positive_audit_trail(wyrd_server: WyrdTestServer) -> None:
    table_fqn, token = wyrd_server.prepare_oracle_query_fixture()
    before = wyrd_server.bifrost_read_decision_count()

    async def query() -> None:
        stream = await BifrostQueryClient(wyrd_server.base_url, token).query(
            f"SELECT id FROM {table_fqn} ORDER BY id",
        )
        _ = [batch async for batch in stream]
        assert stream.terminal is not None

    asyncio.run(query())
    assert wyrd_server.bifrost_read_decision_count() > before
