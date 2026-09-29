"""Public import, projection, and stable error contracts for ``wyrd.state``."""

import inspect
from pathlib import Path

import pytest
import wyrd
from wyrd.state import CardEnvelope, HydratedArtifact, WyrdState

from .support import TinyDataInterface, TinyModelInterface, build_complete_bundle


def test_wyrdstate_exports_only_from_state_module() -> None:
    """The public state classes import from ``wyrd.state`` and top-level re-exports."""
    assert wyrd.WyrdState is WyrdState
    assert wyrd.CardEnvelope is CardEnvelope
    assert wyrd.HydratedArtifact is HydratedArtifact


def test_statecard_and_runtime_module_are_removed() -> None:
    """Legacy runtime and StateCard names are absent from the public package."""
    assert not hasattr(wyrd, "StateCard")
    with pytest.raises(ModuleNotFoundError):
        __import__("wyrd.runtime")


def test_public_state_surfaces_have_runtime_docstrings() -> None:
    """Runtime classes and methods expose substantive help text for offline loading."""
    for value in (CardEnvelope, HydratedArtifact, WyrdState):
        doc = inspect.getdoc(value)
        assert doc and len(doc) > 40
    required = {
        CardEnvelope: (
            "card_ref",
            "aliases",
            "kind",
            "metadata",
            "spec",
            "relationships",
            "status",
            "model_dump",
            "model_dump_json",
        ),
        HydratedArtifact: ("relative_path", "local_path", "sha256", "size_bytes", "content_type"),
        WyrdState: (
            "from_path",
            "service",
            "aliases",
            "card",
            "card_ref",
            "artifacts",
            "model",
            "data",
            "agent",
            "prompt",
            "verifier",
            "workflow",
        ),
    }
    for value, names in required.items():
        for name in names:
            member = getattr(value, name)
            doc = inspect.getdoc(member)
            assert doc and len(doc) > 20, f"missing runtime docs for {value.__name__}.{name}"
    doc = inspect.getdoc(WyrdState.from_path)
    assert doc and "offline" in doc.lower() and "stable" in doc.lower() and "error" in doc.lower()


def test_generated_state_stubs_retain_docstrings() -> None:
    """Generated public state stubs retain class docs and the offline example."""
    stub = Path(__file__).parents[3] / "python" / "wyrd" / "state" / "__init__.pyi"
    text = stub.read_text(encoding="utf-8")
    assert "WyrdState.from_path" in text
    assert "offline" in text.lower()
    assert "./service-bundle" in text
    assert "WYRD_SDK_" in text
    assert "class CardEnvelope" in text and "class HydratedArtifact" in text
    assert "class WyrdState" in text
    assert "path: str | Path" in text
    assert "interfaces" in text and "load_kwargs" in text
    assert "Mapping[str, ModelLoadArgs | DataLoadArgs | Mapping[str, object]]" in text
    for class_name, members in {
        "CardEnvelope": (
            "card_ref",
            "aliases",
            "kind",
            "metadata",
            "spec",
            "relationships",
            "status",
            "model_dump",
            "model_dump_json",
        ),
        "HydratedArtifact": ("relative_path", "local_path", "sha256", "size_bytes", "content_type"),
        "WyrdState": (
            "from_path",
            "service",
            "aliases",
            "card",
            "card_ref",
            "artifacts",
            "model",
            "data",
            "agent",
            "prompt",
            "verifier",
            "workflow",
        ),
    }.items():
        assert f"class {class_name}" in text
        for member in members:
            assert member in text
    assert "Mapping[str, ModelLoadArgs | DataLoadArgs | Mapping[str, object]]" in text


def test_card_envelope_exposes_complete_registered_card(tmp_path: Path) -> None:
    """CardEnvelope projects identity, aliases, kind, and complete JSON fields."""
    state = WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces={
            "model": TinyModelInterface(),
            "backup": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
    )
    envelope = state.card("model")
    assert envelope.kind.name == "Model"
    assert envelope.aliases == ("model",)
    assert envelope.card_ref.name == "model"
    assert isinstance(envelope.aliases, tuple)
    assert isinstance(envelope.metadata, dict)
    assert isinstance(envelope.spec, dict)
    assert isinstance(envelope.relationships, dict)
    assert envelope.status is None or isinstance(envelope.status, dict)
    assert set(envelope.model_dump()) >= {"apiVersion", "metadata", "kind", "spec", "relationships"}
    assert isinstance(envelope.model_dump_json(), str)


def test_wrong_kind_accessor_raises_stable_wyrd_error(tmp_path: Path) -> None:
    """Typed accessors reject a Card of another kind with stable mismatch details."""
    state = WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces={
            "model": TinyModelInterface(),
            "backup": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
    )
    with pytest.raises(wyrd.WyrdError) as caught:
        state.model("root")
    assert caught.value.code == "WYRD_SDK_400_CARD_KIND_MISMATCH"
    assert caught.value.details["alias"] == "root"
    assert caught.value.details["expected_kind"] == "Model"
    assert caught.value.details["actual_kind"] == "Service"
    assert caught.value.details["card_ref"]["kind"] == "Service"
    assert caught.value.details["card_ref"]["name"] == "service"
