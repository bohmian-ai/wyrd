"""Bifrost construction and table configuration, checked before any request."""

import pyarrow
import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig

TABLE = "vala.datasets.events"
TARGET_FILE_SIZE_BYTES = 256 * 1024 * 1024


class Event(BaseModel):
    id: int


@pytest.mark.usefixtures("no_credentials")
def test_bifrost_without_a_resolvable_credential_raises() -> None:
    with pytest.raises(WyrdError) as captured:
        Bifrost()

    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_table_config_has_no_compaction_target_by_default() -> None:
    assert TableConfig(Event, TABLE).compaction_target_file_size_bytes is None


def test_pydantic_table_config_carries_a_compaction_target() -> None:
    config = TableConfig(Event, TABLE, compaction_target_file_size_bytes=TARGET_FILE_SIZE_BYTES)

    assert config.compaction_target_file_size_bytes == TARGET_FILE_SIZE_BYTES


def test_arrow_table_config_carries_a_compaction_target() -> None:
    config = TableConfig.from_arrow(
        TABLE,
        pyarrow.schema([("id", pyarrow.int64())]),
        compaction_target_file_size_bytes=TARGET_FILE_SIZE_BYTES,
    )

    assert config.compaction_target_file_size_bytes == TARGET_FILE_SIZE_BYTES


def test_json_schema_table_config_declares_the_same_columns_as_its_model() -> None:
    schema = {"type": "object", "properties": {"id": {"type": "integer"}}, "required": ["id"]}

    config = TableConfig.from_json_schema(TABLE, schema)

    assert config.fqn == TABLE
    assert config.arrow_schema == TableConfig(Event, TABLE).arrow_schema


def test_table_config_has_no_compaction_type_by_default() -> None:
    assert TableConfig(Event, TABLE).compaction_type is None


@pytest.mark.parametrize("compaction_type", ["auto", "full", "small-files", "files-with-delete"])
def test_table_config_carries_a_compaction_type(compaction_type: str) -> None:
    assert (
        TableConfig(Event, TABLE, compaction_type=compaction_type).compaction_type
        == compaction_type
    )


def test_arrow_table_config_carries_a_compaction_type() -> None:
    config = TableConfig.from_arrow(
        TABLE, pyarrow.schema([("id", pyarrow.int64())]), compaction_type="files-with-delete"
    )

    assert config.compaction_type == "files-with-delete"


@pytest.mark.parametrize("compaction_type", ["small_files", "files_with_delete"])
def test_underscore_compaction_type_is_refused(compaction_type: str) -> None:
    with pytest.raises(WyrdError) as captured:
        TableConfig(Event, TABLE, compaction_type=compaction_type)

    assert captured.value.code == "WYRD_SPEC_400_VALIDATION"
