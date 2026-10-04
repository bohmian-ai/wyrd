"""Boundary tests for the Vala bifrost/observe Python surface."""

from __future__ import annotations

import pytest


def test_extension_submodules_import():
    import wyrd._wyrd.bifrost  # noqa: F401
    import wyrd._wyrd.observe  # noqa: F401


def test_bifrost_without_a_resolvable_credential_raises(monkeypatch: pytest.MonkeyPatch):
    """Constructing with nothing names the failure instead of connecting.

    Every transport argument is optional and resolves through the credential
    chain, so the only construction failure left is an empty chain — and it must
    say so through the typed exception rather than a generic error.
    """

    from wyrd import WyrdError
    from wyrd.bifrost import Bifrost

    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", "/nonexistent-wyrd-home")

    with pytest.raises(WyrdError) as captured:
        Bifrost()
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_producer_key_and_client_scope_are_not_importable():
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ProducerKey  # noqa: F401
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ClientScope  # noqa: F401


def test_table_config_carries_an_optional_compaction_target():
    """The explicit Forge file target reaches the native config from both doors.

    Omitted, it stays ``None`` so the register request carries no target and the
    table follows the server's deployment default.
    """

    import pyarrow
    from pydantic import BaseModel
    from wyrd.bifrost import TableConfig

    class Row(BaseModel):
        id: int

    target = 256 * 1024 * 1024
    assert TableConfig(Row, "vala.datasets.t").compaction_target_file_size_bytes is None
    declared = TableConfig(Row, "vala.datasets.t", compaction_target_file_size_bytes=target)
    assert declared.compaction_target_file_size_bytes == target
    arrow = TableConfig.from_arrow(
        pyarrow.schema([("id", pyarrow.int64())]),
        "vala.datasets.t",
        compaction_target_file_size_bytes=target,
    )
    assert arrow.compaction_target_file_size_bytes == target


def test_table_config_carries_an_optional_compaction_type():
    """The explicit Forge compaction type reaches the native config from both
    doors in its ``snake_case`` wire spelling; an unknown spelling, including
    the hyphenated Iceberg property form, is refused at the boundary."""

    import pyarrow
    from pydantic import BaseModel
    from wyrd import WyrdError
    from wyrd.bifrost import TableConfig

    class Row(BaseModel):
        id: int

    assert TableConfig(Row, "vala.datasets.t").compaction_type is None
    declared = TableConfig(Row, "vala.datasets.t", compaction_type="small_files")
    assert declared.compaction_type == "small_files"
    arrow = TableConfig.from_arrow(
        pyarrow.schema([("id", pyarrow.int64())]),
        "vala.datasets.t",
        compaction_type="files_with_delete",
    )
    assert arrow.compaction_type == "files_with_delete"
    with pytest.raises(WyrdError):
        TableConfig(Row, "vala.datasets.t", compaction_type="small-files")
