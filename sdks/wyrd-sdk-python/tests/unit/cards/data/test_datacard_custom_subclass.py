"""A user-defined DataInterface subclass saved and loaded through a DataCard."""

from __future__ import annotations

import json
from hashlib import sha256
from pathlib import Path

import pytest
from wyrd.data import DataCard, DataInterface, DataStats, WyrdError

ROWS = {"rows": [{"id": 1, "value": "a"}, {"id": 2, "value": "b"}]}


class JsonInterface(DataInterface):
    """Stores its data as one JSON file under the card directory."""

    def __init__(self, data=None):
        super().__init__()
        self.data = data

    def save(self, path, save_kwargs=None):
        output_dir = path / "data" / "custom"
        output_dir.mkdir(parents=True, exist_ok=True)
        payload = json.dumps(self.data).encode("utf-8")
        (output_dir / "data.json").write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path, load_kwargs=None):
        self.data = json.loads((path / "data" / "custom" / "data.json").read_text())


class MetadataAwareJsonInterface(JsonInterface):
    """Keeps the stored card metadata it was rebuilt from."""

    @classmethod
    def from_metadata(cls, metadata):
        interface = cls()
        interface.rebuilt_from = metadata
        return interface


@pytest.mark.parametrize(
    ("interface_class", "rebuilt"),
    [(JsonInterface, False), (MetadataAwareJsonInterface, True)],
    ids=["default", "override"],
)
def test_from_metadata_builds_the_interface_class(
    tmp_path: Path, interface_class: type[JsonInterface], rebuilt: bool
) -> None:
    DataCard(interface_class(ROWS)).save(tmp_path)

    card = DataCard.from_path(tmp_path, interface=interface_class)

    assert type(card.interface) is interface_class
    assert hasattr(card.interface, "rebuilt_from") is rebuilt


def test_datacard_constructor_rejects_custom_interface_class() -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(JsonInterface)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_custom_subclass_save_records_its_datastats(tmp_path: Path) -> None:
    card = DataCard(JsonInterface({"x": 1}))

    card.save(tmp_path)

    assert card.stats.byte_count == len(json.dumps({"x": 1}))


def test_custom_subclass_is_stored_without_its_class_path() -> None:
    card = DataCard(JsonInterface({"x": 1}))

    assert card.metadata.to_dict()["interface"] == {
        "kind": "Custom",
        "meta": {"loader_class": "", "loader_module": ""},
    }


def test_custom_subclass_loads_its_data_through_the_interface_class(tmp_path: Path) -> None:
    DataCard(JsonInterface(ROWS)).save(tmp_path)

    card = DataCard.from_path(tmp_path, interface=JsonInterface)

    assert card.data == ROWS


def test_custom_card_without_its_interface_class_raises(tmp_path: Path) -> None:
    DataCard(JsonInterface({"x": 1})).save(tmp_path)

    with pytest.raises(WyrdError) as exc:
        DataCard.from_path(tmp_path)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_base_interface_save_requires_an_override(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(DataInterface()).save(tmp_path)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_base_interface_load_requires_an_override(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(DataInterface()).load(tmp_path)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"
