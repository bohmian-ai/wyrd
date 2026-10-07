"""The one public Bifrost client: resolve, register, write, read back, and refuse.

Every client resolves the deployment's address and key from the environment
the session ``WyrdTestServer`` exports; each story writes its own fixed
``vala.datasets`` table.
"""

import asyncio
import json
from pathlib import Path

import pyarrow
import pytest
from pydantic import BaseModel, ValidationError
from wyrd import WyrdError
from wyrd.bifrost import AsyncBifrost, Bifrost, TableConfig
from wyrd.client import WyrdClient
from wyrd.observe import record
from wyrd.testing import WyrdTestServer


class Item(BaseModel):
    """The user columns of every story's caller-owned table."""

    id: int
    value: str


class Inference(BaseModel):
    """One served inference, read back through every result conversion."""

    call_id: int
    model: str
    tokens: int
    latency_ms: float
    status: str


INFERENCES = [
    Inference(call_id=1, model="opus", tokens=100, latency_ms=120.5, status="ok"),
    Inference(call_id=2, model="opus", tokens=300, latency_ms=240.0, status="ok"),
    Inference(call_id=3, model="haiku", tokens=50, latency_ms=30.0, status="error"),
]


class MistypedInference(BaseModel):
    """``call_id`` declared as a string, so every real row fails validation."""

    call_id: str


def items(bifrost: Bifrost, table: str) -> list[Item]:
    """Every row of ``table``, in id order."""
    return bifrost.sql(f"SELECT id, value FROM {table} ORDER BY id", model=Item)


@pytest.fixture(scope="module")
def inferences(wyrd_server: WyrdTestServer) -> str:
    """A published ``vala.datasets.inferences`` table holding ``INFERENCES``."""
    writer = Bifrost(TableConfig(Inference, "vala.datasets.inferences"))
    assert writer.register() == "created"
    for row in INFERENCES:
        writer.insert(row)
    writer.shutdown()
    wyrd_server.flush_bifrost()
    return "vala.datasets.inferences"


@pytest.mark.integration
def test_omitted_transport_resolves_from_the_environment(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    writer = Bifrost(TableConfig(Item, "vala.datasets.resolved"))
    assert writer.register() == "created"
    writer.insert(Item(id=1, value="resolved"))
    writer.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.resolved") == [Item(id=1, value="resolved")]


@pytest.mark.integration
def test_no_resolvable_credential_raises(
    wyrd_server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path))
    with pytest.raises(WyrdError) as raised:
        Bifrost()
    assert raised.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


@pytest.mark.integration
def test_insert_without_an_active_table_refuses(wyrd_server: WyrdTestServer) -> None:
    with pytest.raises(WyrdError) as raised:
        Bifrost().insert(Item(id=1, value="nowhere"))
    assert raised.value.code == "WYRD_VALA_412_NO_ACTIVE_TABLE"


@pytest.mark.integration
def test_recorded_telemetry_row_reads_back(wyrd_server: WyrdTestServer, bifrost: Bifrost) -> None:
    writer = Bifrost(TableConfig(Item, "vala.datasets.recorded"))
    assert writer.register() == "created"
    record(
        writer,
        "vala.datasets.recorded",
        json.dumps(Item.model_json_schema()),
        Item(id=1, value="recorded").model_dump_json(),
    )
    writer.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.recorded") == [Item(id=1, value="recorded")]


@pytest.mark.integration
def test_register_insert_flush_and_read(wyrd_server: WyrdTestServer, bifrost: Bifrost) -> None:
    writer = Bifrost(TableConfig(Item, "vala.datasets.journey"))
    assert writer.register() == "created"
    assert writer.register() == "already_exists"
    resolved = writer.table.resolved
    assert resolved is not None and resolved["table_uid"] and resolved["fingerprint"]
    writer.insert(Item(id=4, value="fourth"))
    writer.insert(Item(id=5, value="fifth"))
    writer.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.journey") == [
        Item(id=4, value="fourth"),
        Item(id=5, value="fifth"),
    ]


@pytest.mark.integration
def test_swapped_away_table_still_drains(wyrd_server: WyrdTestServer, bifrost: Bifrost) -> None:
    for table in ("vala.datasets.swap_from", "vala.datasets.swap_to"):
        assert Bifrost(TableConfig(Item, table)).register() == "created"
    writer = Bifrost(TableConfig(Item, "vala.datasets.swap_from"))
    writer.insert(Item(id=1, value="before"))
    writer.use_table_by_name("vala.datasets.swap_to")
    writer.insert(Item(id=2, value="after"))
    writer.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.swap_from") == [Item(id=1, value="before")]
    assert items(bifrost, "vala.datasets.swap_to") == [Item(id=2, value="after")]


@pytest.mark.integration
def test_sql_and_stream_return_the_same_rows(bifrost: Bifrost, inferences: str) -> None:
    query = f"SELECT call_id FROM {inferences} ORDER BY call_id"
    collected = bifrost.sql(query).to_arrow().column("call_id").to_pylist()
    streamed = [id for batch in bifrost.stream(query) for id in batch.column("call_id").to_pylist()]
    assert collected == streamed == [1, 2, 3]


@pytest.mark.integration
def test_describe_binds_an_existing_table_without_restating_its_schema(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    assert Bifrost(TableConfig(Item, "vala.datasets.described")).register() == "created"
    described = TableConfig.describe("vala.datasets.described")
    assert described.arrow_schema.names == ["id", "value"]
    writer = Bifrost(described)
    writer.insert({"id": 6, "value": "described"})
    writer.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.described") == [Item(id=6, value="described")]


@pytest.mark.integration
def test_uncorrelated_row_is_a_valid_write(wyrd_server: WyrdTestServer, bifrost: Bifrost) -> None:
    writer = Bifrost(TableConfig(Item, "vala.datasets.uncorrelated"))
    assert writer.register() == "created"
    writer.insert(Item(id=8, value="uncorrelated"))
    writer.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.uncorrelated") == [Item(id=8, value="uncorrelated")]


@pytest.mark.integration
def test_negative_bad_card_ref_raises(wyrd_server: WyrdTestServer) -> None:
    writer = Bifrost(TableConfig(Item, "vala.datasets.bad_card_ref"))
    with pytest.raises(WyrdError) as raised:
        writer.insert(Item(id=1, value="bad"), {"card_ref": "not-a-ref"})
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


@pytest.mark.integration
def test_negative_empty_permissions_denied_rbac_on_write(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    assert Bifrost(TableConfig(Item, "vala.datasets.denied")).register() == "created"
    denied = Bifrost(
        TableConfig(Item, "vala.datasets.denied"),
        credential=wyrd_server.bootstrap_service([], name="bifrost-write-denied"),
    )
    denied.insert(Item(id=999, value="denied"))
    with pytest.raises(WyrdError) as raised:
        denied.flush()
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    denied.shutdown()
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.denied") == []


@pytest.mark.integration
def test_registering_a_different_schema_on_one_name_conflicts(wyrd_server: WyrdTestServer) -> None:
    assert Bifrost(TableConfig(Item, "vala.datasets.conflict")).register() == "created"
    conflicting = Bifrost(TableConfig(Inference, "vala.datasets.conflict"))
    with pytest.raises(WyrdError) as raised:
        conflicting.register()
    assert raised.value.code == "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
    assert conflicting.table.resolved is None


@pytest.mark.integration
def test_compaction_target_registers_describes_and_conflicts(wyrd_server: WyrdTestServer) -> None:
    table, target = "vala.datasets.compaction_target", 256 * 1024 * 1024

    def writer(target_bytes: int | None) -> Bifrost:
        return Bifrost(TableConfig(Item, table, compaction_target_file_size_bytes=target_bytes))

    assert writer(target).register() == "created"
    assert writer(target).register() == "already_exists"
    assert writer(None).register() == "already_exists"
    with pytest.raises(WyrdError) as raised:
        writer(target * 2).register()
    assert raised.value.code == "WYRD_VALA_409_BIFROST_COMPACTION_TARGET_MISMATCH"
    assert TableConfig.describe(table).compaction_target_file_size_bytes == target


@pytest.mark.integration
def test_compaction_type_registers_describes_and_conflicts(wyrd_server: WyrdTestServer) -> None:
    table = "vala.datasets.compaction_type"

    def writer(compaction_type: str | None) -> Bifrost:
        return Bifrost(TableConfig(Item, table, compaction_type=compaction_type))

    assert writer("small-files").register() == "created"
    assert writer("small-files").register() == "already_exists"
    assert writer(None).register() == "already_exists"
    with pytest.raises(WyrdError) as raised:
        writer("full").register()
    assert raised.value.code == "WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH"
    assert TableConfig.describe(table).compaction_type == "small-files"


@pytest.mark.integration
@pytest.mark.parametrize(
    "query",
    [
        pytest.param("DELETE FROM vala.datasets.inferences", id="non-select"),
        pytest.param(f"SELECT 1 -- {'x' * 64 * 1024}", id="oversized"),
        pytest.param("SELECT FROM", id="invalid"),
    ],
)
def test_query_outside_the_read_floor_is_refused(bifrost: Bifrost, query: str) -> None:
    with pytest.raises(WyrdError) as raised:
        bifrost.sql(query)
    assert raised.value.code == "WYRD_VALA_400_QUERY_INVALID_SQL"


@pytest.mark.integration
def test_read_back_as_arrow_pandas_and_polars(bifrost: Bifrost, inferences: str) -> None:
    result = bifrost.sql(
        f"SELECT call_id, model, tokens, latency_ms, status FROM {inferences} ORDER BY call_id"
    )
    expected = [row.model_dump() for row in INFERENCES]
    assert result.to_arrow().to_pylist() == expected
    assert result.to_pandas().to_dict(orient="records") == expected
    assert result.to_polars().to_dicts() == expected


@pytest.mark.integration
def test_sql_returns_model_instances_when_a_model_is_supplied(
    bifrost: Bifrost, inferences: str
) -> None:
    select = f"SELECT call_id, model, tokens, latency_ms, status FROM {inferences} ORDER BY call_id"

    async def read_async() -> list[Inference]:
        return await AsyncBifrost().sql(select, model=Inference)

    assert bifrost.sql(select, model=Inference) == INFERENCES
    assert asyncio.run(read_async()) == INFERENCES
    assert bifrost.sql(f"{select} LIMIT 0", model=Inference) == []
    with pytest.raises(ValidationError):
        bifrost.sql(f"SELECT call_id FROM {inferences}", model=MistypedInference)


@pytest.mark.integration
def test_delegated_client_reads_as_a_and_cannot_write_with_b_authority(
    wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    assert Bifrost(TableConfig(Item, "vala.datasets.delegated")).register() == "created"
    a_key = wyrd_server.scoped_api_key("delegating_reader", ["bifrost_query:read"])
    b_key = wyrd_server.scoped_api_key(
        "delegated_writer", ["bifrost_query:read", "bifrost_record:write", "bifrost_table:read"]
    )
    service_b = WyrdClient(credential=b_key)
    as_a = Bifrost(client=service_b.on_behalf_of(WyrdClient(credential=a_key).access_token()))
    batch = pyarrow.RecordBatch.from_pylist(
        [{"id": 1, "value": "as-a"}],
        schema=pyarrow.schema(
            [
                pyarrow.field("id", pyarrow.int64(), False),
                pyarrow.field("value", pyarrow.string(), False),
            ]
        ),
    )

    assert items(as_a, "vala.datasets.delegated") == []
    with pytest.raises(WyrdError) as raised:
        as_a.write_batch("vala.datasets.delegated", batch)
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    Bifrost(client=service_b).write_batch("vala.datasets.delegated", batch)
    wyrd_server.flush_bifrost()
    assert items(bifrost, "vala.datasets.delegated") == [Item(id=1, value="as-a")]


@pytest.mark.integration
def test_client_cannot_be_combined_with_transport_options(wyrd_server: WyrdTestServer) -> None:
    with pytest.raises(WyrdError) as raised:
        Bifrost(client=WyrdClient(), credential=wyrd_server.api_key)
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"
