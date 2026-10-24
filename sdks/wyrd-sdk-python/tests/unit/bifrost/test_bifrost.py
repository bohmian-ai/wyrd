"""Bifrost construction and table configuration, checked before any request."""

import ast
import inspect
from pathlib import Path

import pyarrow
import pytest
import wyrd
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd._wyrd import bifrost as native_bifrost
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.bifrost._client import _description

TABLE = "bifrost.datasets.events"
TARGET_FILE_SIZE_BYTES = 256 * 1024 * 1024


class Event(BaseModel):
    id: int


@pytest.mark.usefixtures("no_credentials")
def test_bifrost_without_a_resolvable_credential_raises() -> None:
    with pytest.raises(WyrdError) as captured:
        Bifrost.connect()

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


@pytest.mark.parametrize("version", [1, 2**53 + 1, 2**63 - 1])
def test_a_table_description_projects_its_version_as_an_exact_int(version: int) -> None:
    described = _description({"table_version": str(version)})

    assert type(described["table_version"]) is int
    assert described["table_version"] == version


def _stub_parameters(function: ast.FunctionDef) -> list[tuple[str, bool]]:
    arguments = function.args
    names = [arg.arg for arg in arguments.args if arg.arg != "self"]
    defaulted = len(arguments.defaults)
    return [(name, index >= len(names) - defaulted) for index, name in enumerate(names)]


def test_native_bifrost_stub_matches_runtime_signatures() -> None:
    stub = ast.parse((Path(wyrd.__file__).parent / "_wyrd" / "bifrost.pyi").read_text())
    declared = {
        (cls.name, method.name): _stub_parameters(method)
        for cls in stub.body
        if isinstance(cls, ast.ClassDef)
        for method in cls.body
        if isinstance(method, ast.FunctionDef) and method.name != "__getattr__"
    }
    assert ("Bifrost", "alter_table") in declared
    for (owner, name), parameters in declared.items():
        runtime = getattr(native_bifrost, owner)
        signature = inspect.signature(runtime if name == "__init__" else getattr(runtime, name))
        assert [
            (parameter.name, parameter.default is not inspect.Parameter.empty)
            for parameter in signature.parameters.values()
            if parameter.name != "self"
        ] == parameters, f"{owner}.{name}"
