"""End-to-end Card declaration, artifact-free lookup, and deletion journeys."""

from __future__ import annotations

import json
import threading
from hashlib import sha256
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import (
    Cards,
    DataSaveArgs,
    ModelSaveArgs,
    VersionBump,
)
from wyrd.client import WyrdClient
from wyrd.data import DataCard, DataInterface, DataStats, FieldSpec
from wyrd.model import (
    ModelCard,
    ModelCardMetadata,
    ModelInterface,
    ModelSignature,
)
from wyrd.prompt import Prompt, PromptCard
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration


class JsonDataInterface(DataInterface):
    def __init__(self, value: dict[str, int] | None = None) -> None:
        super().__init__()
        self.value = value
        self.saved_kwargs: dict[str, str | bool] | None = None
        self.loaded_kwargs: dict[str, str | bool] | None = None
        self.loaded_path: Path | None = None

    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> DataStats:
        self.saved_kwargs = save_kwargs
        payload = json.dumps(self.value or {}, sort_keys=True).encode("utf-8")
        output = path / "data" / "custom" / "data.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, str | bool] | None = None) -> None:
        self.loaded_path = path
        self.loaded_kwargs = load_kwargs
        self.value = json.loads((path / "data" / "custom" / "data.json").read_text())


class SymlinkDataInterface(JsonDataInterface):
    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> DataStats:
        stats = super().save(path, save_kwargs)
        source = path / "data" / "custom" / "source.bin"
        source.write_bytes(b"source")
        (path / "data" / "custom" / "linked.bin").symlink_to(source)
        return stats


class LargeDataInterface(DataInterface):
    """Write a bounded artifact and mark the start of native preparation."""

    def __init__(self, counter: list[int]) -> None:
        super().__init__()
        self.counter = counter

    def save(self, path: Path, save_kwargs=None) -> DataStats:
        """Write deterministic bytes, then reset progress before native hashing."""
        del save_kwargs
        payload = b"x" * (512 * 1024)
        output = path / "data" / "large.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(payload)
        self.counter[0] = 0
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs=None) -> None:
        """Provide the abstract loader without use in this declaration test."""
        del path, load_kwargs


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


def _model_metadata() -> ModelCardMetadata:
    signature = ModelSignature(
        [FieldSpec("feature", "float64")],
        [FieldSpec("prediction", "float64")],
    )
    return ModelCardMetadata(task_type="other", signature=signature)


def test_prompt_cards_declare_get_list_resolve_latest_and_delete(cards: Cards) -> None:
    name = "crud-prompt"
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
        name=name,
        version="0.1.0",
    )

    receipt = cards.declare(card)

    assert card.uid == receipt.root.uid
    assert receipt.root.name == name
    envelope = cards.prompt.get(uid=card.uid)
    assert isinstance(envelope, PromptCard)
    assert envelope.uid == card.uid
    assert isinstance(envelope.prompt, Prompt)
    assert envelope.prompt.model == "gpt-4o"
    latest = cards.prompt.resolve_latest(name=name)
    assert latest.uid == card.uid
    exact = cards.prompt.get(name=name, version=latest.version)
    assert exact.uid == card.uid
    page = cards.prompt.list(name=name)
    assert any(item.uid == card.uid and item.status == "active" for item in page.items)

    cards.prompt.delete(uid=card.uid)
    with pytest.raises(WyrdError) as deleted:
        cards.prompt.get(uid=card.uid)
    assert deleted.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"


def test_data_card_custom_interface_get_requires_interface_and_downloads_no_artifacts(
    cards: Cards,
) -> None:
    name = "custom-data"
    interface = JsonDataInterface({"rows": 2})
    card = DataCard(interface, name=name, version="0.1.0")

    cards.data.declare(
        card,
        save_args=DataSaveArgs({"compression": "none"}, copy_bytes=True),
    )

    assert card.uid
    assert interface.saved_kwargs == {"compression": "none", "copy_bytes": True}
    with pytest.raises(WyrdError) as no_interface:
        cards.data.get(uid=card.uid)
    assert no_interface.value.code == "WYRD_DATA_400_VALIDATION"

    unloaded = cards.data.get(uid=card.uid, interface=JsonDataInterface)
    assert unloaded.uid == card.uid
    assert unloaded.interface.value is None
    with pytest.raises(TypeError):
        unloaded.load()  # ty: ignore[missing-argument]
    with pytest.raises(TypeError):
        cards.data.get(uid=card.uid, interface=JsonDataInterface, eager_load=True)  # ty: ignore[no-matching-overload]

    cards.data.delete(uid=card.uid)


def test_model_card_custom_interface_get_requires_interface_and_downloads_no_artifacts(
    cards: Cards,
) -> None:
    name = "custom-model"
    interface = TextModelInterface("model-bytes")
    card = ModelCard(
        interface,
        name=name,
        version="0.1.0",
        metadata=_model_metadata(),
    )

    cards.model.declare(card, save_args=ModelSaveArgs({"format": "text"}))

    assert card.uid
    assert interface.saved_kwargs == {"format": "text"}
    with pytest.raises(WyrdError) as no_interface:
        cards.model.get(uid=card.uid)
    assert no_interface.value.code == "WYRD_MODEL_400_VALIDATION"

    unloaded = cards.model.get(uid=card.uid, interface=TextModelInterface)
    assert unloaded.uid == card.uid
    assert unloaded.interface.value == ""
    with pytest.raises(TypeError):
        unloaded.load()  # ty: ignore[missing-argument]
    with pytest.raises(TypeError):
        cards.model.get(uid=card.uid, interface=TextModelInterface, eager_load=True)  # ty: ignore[no-matching-overload]

    cards.model.delete(uid=card.uid)


def test_typed_registry_rejects_a_card_of_the_wrong_kind(cards: Cards) -> None:
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"), name="wrong-kind")

    with pytest.raises(WyrdError) as raised:
        cards.data.declare(card)  # ty: ignore[invalid-argument-type]
    assert raised.value.code == "WYRD_REGISTRY_400_INVALID_CARD_SPEC"


def test_card_declaration_replay_is_idempotent(cards: Cards) -> None:
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Replay me"),
        name="replay",
        version="0.1.0",
    )

    first = cards.prompt.declare(card)
    second = cards.prompt.declare(card)

    assert first.root.uid == second.root.uid == card.uid
    assert second.outcomes[0].outcome == "idempotent_noop"
    cards.prompt.delete(uid=card.uid)


def test_declaration_version_modes_match_server_contract(cards: Cards) -> None:
    """Omitted/Patch/Minor bumps and exact pins resolve without mixed intent."""
    name = "versions"

    omitted = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="version"),
        name=name,
        version="1",
    )
    assert cards.prompt.declare(omitted).root.version == "1.0.0"
    assert omitted.version == "1.0.0"

    patch = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="version changed"),
        name=name,
        version="1",
    )
    assert cards.prompt.declare(patch, version_bump=VersionBump.Patch).root.version == "1.0.1"
    assert patch.version == "1.0.1"

    minor = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="version changed again"),
        name=name,
        version="1",
    )
    assert cards.prompt.declare(minor, version_bump=VersionBump.Minor).root.version == "1.1.0"
    assert minor.version == "1.1.0"

    exact = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="exact"),
        name=name,
        version="2.3.4",
    )
    assert cards.prompt.declare(exact).root.version == "2.3.4"
    assert exact.version == "2.3.4"


def test_declaration_releases_the_gil(cards: Cards) -> None:
    """Native artifact hashing and upload let another Python thread make progress."""
    counter = [0]
    stop = threading.Event()

    def compete() -> None:
        """Increment while declaration releases the interpreter lock."""
        while not stop.is_set():
            counter[0] += 1

    thread = threading.Thread(target=compete)
    thread.start()
    card = DataCard(LargeDataInterface(counter), name="gil-data", version="0.1.0")
    try:
        cards.data.declare(card)
    finally:
        stop.set()
        thread.join()
    assert counter[0] > 0


def test_card_declaration_rejects_invalid_artifact_layout(cards: Cards) -> None:
    card = DataCard(
        SymlinkDataInterface({"rows": 1}),
        name="symlink-data",
        version="0.1.0",
    )
    with pytest.raises(WyrdError) as raised:
        cards.data.declare(card)
    assert raised.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"


def test_card_declaration_rejects_underprivileged_writer(wyrd_server: WyrdTestServer) -> None:
    denied = Cards(WyrdClient(credential=wyrd_server.bootstrap_service([], name="no-roles")))
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="No write permission"),
        name="forbidden",
        version="3.2.1",
    )
    original = (card.uid, card.version)
    with pytest.raises(WyrdError) as raised:
        denied.prompt.declare(card)
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert (card.uid, card.version) == original


def test_card_registry_enforces_cross_tenant_isolation(
    cards: Cards, wyrd_server: WyrdTestServer
) -> None:
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Tenant A"),
        name="tenant-a-prompt",
        version="0.1.0",
    )
    cards.prompt.declare(card)
    tenant_b = wyrd_server.seed_tenant("tenant-b")
    cards_b = Cards(
        WyrdClient(
            credential=wyrd_server.bootstrap_service_in_tenant(
                tenant_b, ["editor"], name="tenant-b"
            )
        )
    )

    with pytest.raises(WyrdError) as read:
        cards_b.prompt.get(uid=card.uid)
    with pytest.raises(WyrdError) as deleted:
        cards_b.prompt.delete(uid=card.uid)
    assert (read.value.code, deleted.value.code) == (
        "WYRD_REGISTRY_404_CARD_NOT_FOUND",
        "WYRD_REGISTRY_404_CARD_NOT_FOUND",
    )
