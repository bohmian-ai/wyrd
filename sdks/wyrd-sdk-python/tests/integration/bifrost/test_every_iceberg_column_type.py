"""Declare every Iceberg column type from an Arrow schema, write it, and read it back.

``TableConfig.from_arrow`` declares the types a Pydantic model cannot, such as
``decimal128``, ``int32``, maps, and nanosecond timestamps. Types with no
Iceberg column are refused before any table exists. Every SDK reads the same
``fixtures/bifrost`` batches, written by ``fixtures/bifrost/every_iceberg_type.py``.
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import pyarrow
import pyarrow.ipc
import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

ALL_TYPES = "vala.datasets.all_types"
NARROW_TYPES = "vala.datasets.narrow_types"
UNSUPPORTED_TYPE = "WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE"

FIXTURES = Path(__file__).parents[5] / "fixtures" / "bifrost"
EVERY_TYPE = pyarrow.ipc.open_file(FIXTURES / "every_iceberg_type.arrow").get_batch(0)
NARROW = pyarrow.ipc.open_file(FIXTURES / "narrow_types.arrow").get_batch(0)


@pytest.fixture(scope="module")
def all_types(wyrd_server: WyrdTestServer) -> pyarrow.Table:
    """Register ``ALL_TYPES``, write ``EVERY_TYPE``, and read it back."""

    all_types = Bifrost(TableConfig.from_arrow(EVERY_TYPE.schema, ALL_TYPES))
    all_types.register()
    all_types.write_batch(ALL_TYPES, EVERY_TYPE)
    wyrd_server.flush_bifrost()
    return all_types.sql(f"SELECT * FROM {ALL_TYPES} ORDER BY id").to_arrow()


@pytest.fixture(scope="module")
def narrow_types(wyrd_server: WyrdTestServer) -> pyarrow.Table:
    """Register ``NARROW_TYPES``, write ``NARROW``, and read it back."""

    narrow_types = Bifrost(TableConfig.from_arrow(NARROW.schema, NARROW_TYPES))
    narrow_types.register()
    narrow_types.write_batch(NARROW_TYPES, NARROW)
    wyrd_server.flush_bifrost()
    return narrow_types.sql(f"SELECT * FROM {NARROW_TYPES} ORDER BY int8").to_arrow()


@pytest.mark.integration
@pytest.mark.parametrize("column", EVERY_TYPE.schema.names)
def test_every_iceberg_type_round_trips_with_nested_nulls(
    all_types: pyarrow.Table, column: str
) -> None:
    written = EVERY_TYPE.column(column)

    assert all_types.column(column).type == written.type
    assert all_types.column(column).combine_chunks().equals(written)


@pytest.mark.integration
@pytest.mark.parametrize("column", NARROW.schema.names)
def test_a_narrower_type_reads_back_the_same_values(
    narrow_types: pyarrow.Table, column: str
) -> None:
    written = NARROW.column(column)

    assert narrow_types.column(column).combine_chunks().cast(written.type).equals(written)


@pytest.mark.integration
def test_a_narrower_declaration_is_the_table_it_describes(narrow_types: pyarrow.Table) -> None:
    described = Bifrost(TableConfig.describe(NARROW_TYPES))

    assert described.register() == "already_exists"


@pytest.mark.integration
@pytest.mark.parametrize(
    "column_type",
    [
        pyarrow.dense_union([pyarrow.field("i", pyarrow.int32())]),
        pyarrow.duration("s"),
        pyarrow.month_day_nano_interval(),
        pyarrow.float16(),
        pyarrow.map_(pyarrow.string(), pyarrow.float16()),
    ],
    ids=str,
)
def test_a_type_with_no_wire_form_is_refused_when_declared(column_type: pyarrow.DataType) -> None:
    with pytest.raises(WyrdError) as refused:
        TableConfig.from_arrow(pyarrow.schema([("c", column_type)]), "vala.datasets.no_wire_form")

    assert refused.value.code == UNSUPPORTED_TYPE


@pytest.mark.integration
@pytest.mark.parametrize(
    "column_type",
    [pyarrow.uint64(), pyarrow.date64(), pyarrow.map_(pyarrow.string(), pyarrow.uint64())],
    ids=str,
)
def test_a_type_with_no_iceberg_column_is_refused_at_registration(
    wyrd_server: WyrdTestServer, column_type: pyarrow.DataType
) -> None:
    refused_table = Bifrost(
        TableConfig.from_arrow(pyarrow.schema([("c", column_type)]), "vala.datasets.no_column")
    )

    with pytest.raises(WyrdError) as refused:
        refused_table.register()

    assert refused.value.code == UNSUPPORTED_TYPE


@pytest.mark.integration
def test_a_refused_registration_creates_no_table(wyrd_server: WyrdTestServer) -> None:
    refused_table = Bifrost(
        TableConfig.from_arrow(pyarrow.schema([("c", pyarrow.uint64())]), "vala.datasets.never")
    )
    with pytest.raises(WyrdError):
        refused_table.register()

    with pytest.raises(WyrdError) as missing:
        TableConfig.describe("vala.datasets.never")

    assert missing.value.code == "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"
