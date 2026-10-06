"""Tables with free-form, union, and nested fields.

A model field typed ``Any`` or as a union is stored as a Variant column, a
nested model as a Struct column, and a typed list as a List column. Rows and
Arrow tables go in; native Python values come back out.
"""

from __future__ import annotations

import uuid
from typing import TYPE_CHECKING, Any

import pyarrow
import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration


class Point(BaseModel):
    x: int
    label: str | None = None


class Event(BaseModel):
    id: int
    payload: Any = None  # free-form JSON -> Variant
    mixed: int | str | None = None  # union -> Variant
    point: Point | None = None  # nested model -> Struct
    tags: list[str] | None = None  # typed list -> List


@pytest.fixture
def bifrost(wyrd_server: WyrdTestServer) -> Bifrost:
    """A client writing to a freshly registered ``Event`` table."""

    bifrost = Bifrost(
        TableConfig(Event, f"vala.datasets.events_{uuid.uuid4().hex}"),
        server_url=wyrd_server.base_url,
        credential=wyrd_server.api_key,
    )
    bifrost.register()
    return bifrost


def is_variant(field: pyarrow.Field) -> bool:
    return (field.metadata or {}).get(b"ARROW:extension:name") == b"arrow.parquet.variant"


def json_text_schema(bifrost: Bifrost) -> pyarrow.Schema:
    """The table's Arrow schema with its Variant columns written as JSON text."""

    declared = bifrost.table.arrow_schema
    return pyarrow.schema(
        [
            declared.field("id"),
            pyarrow.field("payload", pyarrow.string()),
            pyarrow.field("mixed", pyarrow.string()),
            declared.field("point"),
            declared.field("tags"),
        ]
    )


def test_model_fields_become_variant_struct_and_list_columns(bifrost: Bifrost) -> None:
    schema = bifrost.table.arrow_schema

    assert is_variant(schema.field("payload"))
    assert is_variant(schema.field("mixed"))
    assert pyarrow.types.is_struct(schema.field("point").type)
    assert pyarrow.types.is_list(schema.field("tags").type)


def test_rows_come_back_as_native_values(bifrost: Bifrost, wyrd_server: WyrdTestServer) -> None:
    events = [
        Event(
            id=1,
            payload={"scores": [1, {"z": None}], "big": 2**64 - 1},
            mixed=5,
            point=Point(x=1, label="a"),
            tags=["a", "b"],
        ),
        Event(id=2, payload='{"stays": "a string"}', mixed="five", tags=[]),
    ]
    for event in events:
        bifrost.insert(event)
    bifrost.flush()
    wyrd_server.flush_bifrost()

    table = bifrost.table.fqn
    rows = bifrost.sql(f"SELECT id, payload, mixed, point, tags FROM {table} ORDER BY id", Event)

    assert rows == events


def test_arrow_json_text_is_stored_as_variant(
    bifrost: Bifrost, wyrd_server: WyrdTestServer
) -> None:
    arrow = pyarrow.Table.from_pylist(
        [{"id": 1, "payload": '{"n": 9007199254740993}', "mixed": '"seven"'}],
        schema=json_text_schema(bifrost),
    )
    bifrost.write_batch(bifrost.table.fqn, arrow.to_batches()[0])
    wyrd_server.flush_bifrost()

    rows = bifrost.sql(f"SELECT id, payload, mixed FROM {bifrost.table.fqn}", Event)

    assert rows == [Event(id=1, payload={"n": 9007199254740993}, mixed="seven")]


def test_query_results_copy_into_another_table(
    bifrost: Bifrost, wyrd_server: WyrdTestServer
) -> None:
    archive = TableConfig(Event, f"vala.datasets.archive_{uuid.uuid4().hex}")
    Bifrost(archive, server_url=wyrd_server.base_url, credential=wyrd_server.api_key).register()
    event = Event(id=1, payload={"source": "row"}, mixed=8, point=Point(x=1), tags=["a"])
    bifrost.insert(event)
    bifrost.flush()
    wyrd_server.flush_bifrost()

    copied = bifrost.sql(f"SELECT id, payload, mixed, point, tags FROM {bifrost.table.fqn}")
    bifrost.write_batch(archive.fqn, copied.to_arrow().to_batches()[0])
    wyrd_server.flush_bifrost()

    rows = bifrost.sql(f"SELECT id, payload, mixed, point, tags FROM {archive.fqn}", Event)
    assert rows == [event]


def test_struct_fields_stay_typed_and_variant_paths_stay_variant(
    bifrost: Bifrost, wyrd_server: WyrdTestServer
) -> None:
    bifrost.insert(Event(id=1, payload={"scores": [1, 2]}, point=Point(x=7)))
    bifrost.flush()
    wyrd_server.flush_bifrost()

    result = bifrost.sql(
        f"SELECT point['x'] AS x, payload -> 'scores' AS scores FROM {bifrost.table.fqn}"
    )
    schema = result.to_arrow().schema

    assert schema.field("x").type == pyarrow.int64()
    assert is_variant(schema.field("scores"))
    assert result.to_arrow().column("x").to_pylist() == [7]


def test_undeclared_field_is_refused(bifrost: Bifrost, wyrd_server: WyrdTestServer) -> None:
    with pytest.raises(WyrdError) as error:
        bifrost.insert({"id": 1, "undeclared": True})
    bifrost.flush()
    wyrd_server.flush_bifrost()

    assert error.value.code == "WYRD_VALA_400_BIFROST_UNDECLARED_FIELD"
    assert len(bifrost.sql(f"SELECT id FROM {bifrost.table.fqn}")) == 0


def test_invalid_json_text_is_refused(bifrost: Bifrost, wyrd_server: WyrdTestServer) -> None:
    arrow = pyarrow.Table.from_pylist(
        [{"id": 1, "payload": "{not json"}],
        schema=json_text_schema(bifrost),
    )

    with pytest.raises(WyrdError) as error:
        bifrost.write_batch(bifrost.table.fqn, arrow.to_batches()[0])
    wyrd_server.flush_bifrost()

    assert error.value.code == "WYRD_VALA_400_VARIANT_INVALID_JSON"
    assert len(bifrost.sql(f"SELECT id FROM {bifrost.table.fqn}")) == 0


def test_unsupported_type_is_refused() -> None:
    schema = pyarrow.schema([pyarrow.field("count", pyarrow.uint64())])

    with pytest.raises(WyrdError) as error:
        TableConfig.from_arrow(schema, "vala.datasets.unsigned")

    assert error.value.code == "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE"
