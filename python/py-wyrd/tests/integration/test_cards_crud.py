"""End-to-end Card registration, lookup, artifact loading, and deletion journeys."""

from __future__ import annotations

import json
import os
from hashlib import sha256
from pathlib import Path
from uuid import uuid4

import pytest
from wyrd import WyrdError
from wyrd.cards import Cards, DataLoadArgs, DataSaveArgs, ModelLoadArgs, ModelSaveArgs
from wyrd.data import DataCard, DataInterface, DataStats, FieldSpec
from wyrd.model import (
    ModelCard,
    ModelCardMetadata,
    ModelInterface,
    ModelSignature,
)
from wyrd.prompt import Prompt, PromptCard


class JsonDataInterface(DataInterface):
    def __init__(self, value: dict[str, int] | None = None) -> None:
        super().__init__()
        self.value = value
        self.saved_kwargs: dict[str, str | bool] | None = None
        self.loaded_kwargs: dict[str, str | bool] | None = None

    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> DataStats:
        self.saved_kwargs = save_kwargs
        payload = json.dumps(self.value or {}, sort_keys=True).encode("utf-8")
        output = path / "data" / "custom" / "data.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, str | bool] | None = None) -> None:
        self.loaded_kwargs = load_kwargs
        self.value = json.loads((path / "data" / "custom" / "data.json").read_text())


class SymlinkDataInterface(JsonDataInterface):
    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> DataStats:
        stats = super().save(path, save_kwargs)
        source = path / "data" / "custom" / "source.bin"
        source.write_bytes(b"source")
        (path / "data" / "custom" / "linked.bin").symlink_to(source)
        return stats


class TextModelInterface(ModelInterface):
    def __init__(self, value: str = "") -> None:
        super().__init__()
        self.value = value
        self.saved_kwargs: dict[str, str | bool] | None = None
        self.loaded_kwargs: dict[str, str | bool] | None = None

    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> None:
        self.saved_kwargs = save_kwargs
        path.mkdir(parents=True, exist_ok=True)
        (path / "model.txt").write_text(self.value, encoding="utf-8")

    def load(self, path: Path, load_kwargs: dict[str, str | bool] | None = None) -> None:
        self.loaded_kwargs = load_kwargs
        self.value = (path / "model.txt").read_text(encoding="utf-8")

    @property
    def model(self) -> str:
        return self.value


def _name(prefix: str) -> str:
    return f"{prefix}-{uuid4().hex[:12]}"


def _model_metadata() -> ModelCardMetadata:
    signature = ModelSignature(
        [FieldSpec("feature", "float64")],
        [FieldSpec("prediction", "float64")],
    )
    return ModelCardMetadata(task_type="other", signature=signature)


def _cards(wyrd_server) -> Cards:
    api_key = wyrd_server.bootstrap_service(["writer"], name=_name("cards-client"))
    os.environ["WYRD_SERVER_URL"] = wyrd_server.base_url
    os.environ["WYRD_API_KEY"] = api_key
    return Cards()


@pytest.mark.integration
def test_prompt_cards_register_get_list_resolve_latest_and_delete(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    name = _name("prompt")
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
        space="python-e2e",
        name=name,
        version="0.1.0",
    )

    receipt = cards.register(card)

    assert card.uid == receipt.root.uid
    assert receipt.root.name == name
    envelope = cards.prompt.get(uid=card.uid)
    assert isinstance(envelope, PromptCard)
    assert envelope.uid == card.uid
    assert isinstance(envelope.prompt, Prompt)
    assert envelope.prompt.model == "gpt-4o"
    latest = cards.prompt.resolve_latest(space="python-e2e", name=name)
    assert latest.uid == card.uid
    exact = cards.prompt.get(space="python-e2e", name=name, version=latest.version)
    assert exact.uid == card.uid
    page = cards.prompt.list(space="python-e2e", name=name)
    assert any(item.uid == card.uid and item.status == "active" for item in page.items)

    cards.prompt.delete(uid=card.uid)
    with pytest.raises(WyrdError):
        cards.prompt.get(uid=card.uid)


@pytest.mark.integration
def test_data_card_custom_interface_get_requires_interface_and_loads_artifacts(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    name = _name("data")
    interface = JsonDataInterface({"rows": 2})
    card = DataCard(interface, space="python-e2e", name=name, version="0.1.0")

    cards.data.register(
        card,
        save_args=DataSaveArgs({"compression": "none"}, copy_bytes=True),
    )

    assert card.uid
    assert interface.saved_kwargs == {"compression": "none", "copy_bytes": True}
    with pytest.raises(WyrdError):
        cards.data.get(uid=card.uid)

    loaded = cards.data.get(uid=card.uid, interface=JsonDataInterface)
    loaded.load(load_kwargs=DataLoadArgs({"strict": True}))
    assert loaded.uid == card.uid
    assert loaded.interface.value == {"rows": 2}
    assert loaded.interface.loaded_kwargs == {"strict": True}

    cards.data.delete(uid=card.uid)


@pytest.mark.integration
def test_model_card_custom_interface_get_requires_interface_and_loads_artifacts(
    wyrd_server,
) -> None:
    cards = _cards(wyrd_server)
    name = _name("model")
    interface = TextModelInterface("model-bytes")
    card = ModelCard(
        interface,
        space="python-e2e",
        name=name,
        version="0.1.0",
        metadata=_model_metadata(),
    )

    cards.model.register(card, save_args=ModelSaveArgs({"format": "text"}))

    assert card.uid
    assert interface.saved_kwargs == {"format": "text"}
    with pytest.raises(WyrdError):
        cards.model.get(uid=card.uid)

    loaded = cards.model.get(uid=card.uid, interface=TextModelInterface)
    loaded.load(load_kwargs=ModelLoadArgs({"strict": True}))
    assert loaded.interface.value == "model-bytes"
    assert loaded.interface.loaded_kwargs == {"strict": True}
    assert loaded.model == "model-bytes"
    assert loaded.preprocessor is None
    assert loaded.processor is None

    cards.model.delete(uid=card.uid)


@pytest.mark.integration
def test_typed_registry_rejects_a_card_of_the_wrong_kind(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"), name=_name("wrong"))

    with pytest.raises(WyrdError):
        cards.data.register(card)


@pytest.mark.integration
def test_card_registration_replay_is_idempotent(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Replay me"),
        space="python-e2e",
        name=_name("replay"),
        version="0.1.0",
    )

    first = cards.prompt.register(card)
    second = cards.prompt.register(card)

    assert first.root.uid == second.root.uid == card.uid
    assert second.outcomes[0].outcome == "idempotent_noop"
    cards.prompt.delete(uid=card.uid)


@pytest.mark.integration
def test_card_registration_rejects_invalid_artifact_layout(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = DataCard(
        SymlinkDataInterface({"rows": 1}),
        space="python-e2e",
        name=_name("symlink"),
        version="0.1.0",
    )

    with pytest.raises(WyrdError):
        cards.data.register(card)


@pytest.mark.integration
def test_card_registration_rejects_underprivileged_writer(wyrd_server) -> None:
    api_key = wyrd_server.bootstrap_service([], name=_name("read-only"))
    cards = Cards(server_url=wyrd_server.base_url, api_key=api_key)
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="No write permission"),
        space="python-e2e",
        name=_name("forbidden"),
        version="0.1.0",
    )

    with pytest.raises(WyrdError):
        cards.prompt.register(card)


@pytest.mark.integration
def test_card_registry_enforces_cross_tenant_isolation(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Tenant A"),
        space="python-e2e",
        name=_name("tenant"),
        version="0.1.0",
    )
    cards.prompt.register(card)

    tenant_b = wyrd_server.seed_tenant(_name("tenant-b"))
    tenant_b_key = wyrd_server.bootstrap_service_in_tenant(
        tenant_b,
        ["writer"],
        name=_name("tenant-b-client"),
    )
    cards_b = Cards(server_url=wyrd_server.base_url, api_key=tenant_b_key)

    with pytest.raises(WyrdError):
        cards_b.prompt.get(uid=card.uid)
    with pytest.raises(WyrdError):
        cards_b.prompt.delete(uid=card.uid)

    cards.prompt.delete(uid=card.uid)
