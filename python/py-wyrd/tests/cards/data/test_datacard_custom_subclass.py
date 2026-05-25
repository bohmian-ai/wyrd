from __future__ import annotations

import json
from hashlib import sha256

import pytest
from wyrd.data import DataCard, DataInterface, DataStats, WyrdError


class JsonInterface(DataInterface):
    def __init__(self, data=None):
        super().__init__()
        self.data = data
        self.super_called = True

    def save(self, path, save_kwargs=None):
        output_dir = path / "data" / "custom"
        output_dir.mkdir(parents=True, exist_ok=True)
        payload = json.dumps(self.data).encode("utf-8")
        (output_dir / "data.json").write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path, load_kwargs=None):
        self.data = json.loads((path / "data" / "custom" / "data.json").read_text())


class MetadataAwareJsonInterface(JsonInterface):
    @classmethod
    def from_metadata(cls, metadata):
        interface = cls()
        interface.metadata_payload = metadata.to_dict()
        return interface


def _resolve_saved_datacard_with_interface_class(path, interface):
    card = DataCard.model_validate_json((path / "card.json").read_text(), interface=interface)
    card.load(path)
    return card


def test_subclass_constructs_and_is_instance_of_base() -> None:
    assert isinstance(JsonInterface({"x": 1}), DataInterface)


def test_subclass_init_calls_super_init() -> None:
    assert JsonInterface({"x": 1}).super_called is True


def test_subclass_kind_is_custom() -> None:
    assert JsonInterface({"x": 1}).kind == "Custom"


def test_subclass_inherits_from_metadata_default() -> None:
    interface = JsonInterface.from_metadata(DataCard(JsonInterface({"x": 1})).metadata)

    assert isinstance(interface, JsonInterface)
    assert interface.data is None


def test_datacard_accepts_custom_subclass_interface(tmp_path) -> None:
    card = DataCard(JsonInterface({"x": 1}))
    card.save(tmp_path)

    assert card.interface.kind == "Custom"


def test_datacard_constructor_rejects_custom_interface_class() -> None:
    with pytest.raises(WyrdError, match="initialized DataInterface instance") as exc:
        DataCard(JsonInterface)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_custom_subclass_save_returns_datastats_and_card_save_records_it(tmp_path) -> None:
    card = DataCard(JsonInterface({"x": 1}))
    card.save(tmp_path)

    assert card.stats.byte_count > 0
    assert len(card.stats.sha256) == 64


def test_custom_subclass_serializes_as_custom_with_empty_loader_fields(tmp_path) -> None:
    card = DataCard(JsonInterface({"x": 1}))
    card.save(tmp_path)
    payload = json.loads(card.model_dump_json())

    assert payload["spec"]["interface"]["kind"] == "Custom"
    assert payload["spec"]["interface"]["meta"]["loader_class"] == ""
    assert payload["spec"]["interface"]["meta"]["loader_module"] == ""


def test_custom_subclass_cards_get_style_resolves_interface_class_and_loads(tmp_path) -> None:
    source_data = {"rows": [{"id": 1, "value": "a"}, {"id": 2, "value": "b"}]}
    card = DataCard(JsonInterface(source_data))
    card.save(tmp_path)

    restored = _resolve_saved_datacard_with_interface_class(tmp_path, interface=JsonInterface)

    assert isinstance(restored.interface, JsonInterface)
    assert restored.interface.kind == "Custom"
    assert restored.data == source_data
    assert restored.interface.data == source_data


def test_custom_subclass_from_metadata_override_receives_card_metadata(tmp_path) -> None:
    source_data = {"rows": [{"id": 1, "value": "a"}]}
    card = DataCard(MetadataAwareJsonInterface(source_data))
    card.save(tmp_path)

    restored = _resolve_saved_datacard_with_interface_class(
        tmp_path,
        interface=MetadataAwareJsonInterface,
    )

    assert isinstance(restored.interface, MetadataAwareJsonInterface)
    assert restored.interface.metadata_payload["interface"]["kind"] == "Custom"
    assert restored.data == source_data


def test_model_validate_json_interface_none_for_subclass_card_raises(tmp_path) -> None:
    card = DataCard(JsonInterface({"x": 1}))
    card.save(tmp_path)

    with pytest.raises(WyrdError, match="pass interface=YourInterface") as exc:
        DataCard.model_validate_json(card.model_dump_json(), interface=None)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_base_save_load_without_override_raise_required_override(tmp_path) -> None:
    interface = DataInterface()

    with pytest.raises(WyrdError) as save_exc:
        DataCard(interface).save(tmp_path)
    with pytest.raises(WyrdError) as load_exc:
        DataCard(interface).load(tmp_path)

    assert save_exc.value.code == "WYRD_DATA_400_VALIDATION"
    assert load_exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_custom_subclass_class_path_is_not_stored_or_reimported(tmp_path) -> None:
    card = DataCard(JsonInterface({"x": 1}))
    card.save(tmp_path)
    payload = json.loads(card.model_dump_json())

    assert "JsonInterface" not in json.dumps(payload)
    assert payload["spec"]["interface"]["meta"]["loader_module"] == ""
