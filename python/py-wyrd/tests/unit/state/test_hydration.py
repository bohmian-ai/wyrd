"""Offline holder hydration and loader-configuration contracts."""

import gc
from pathlib import Path

import pytest
import wyrd
from wyrd.model import ModelCard
from wyrd.state import WyrdState

from .support import (
    TinyDataInterface,
    TinyModelInterface,
    build_builtin_model_bundle,
    build_complete_bundle,
)


def _interfaces() -> dict[str, object]:
    """Build fresh custom interfaces for the fixture's Model and Data Cards.

    Each call returns independent objects so tests can assert identity sharing
    without leaking loader state between bundle hydrations.
    """
    return {
        "model": TinyModelInterface(),
        "backup": TinyModelInterface(),
        "training_data": TinyDataInterface(),
    }


def test_two_model_aliases_return_distinct_modelcards(tmp_path: Path) -> None:
    """Distinct exact Model Cards produce distinct persistent ModelCard objects."""
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=_interfaces())
    primary = state.model("model")
    backup = state.model("backup")
    assert isinstance(primary, ModelCard)
    assert isinstance(backup, ModelCard)
    assert primary is not backup


def test_duplicate_aliases_return_identical_python_object(tmp_path: Path) -> None:
    """Aliases for one exact CardRef return one shared Python holder."""
    state = WyrdState.from_path(
        build_complete_bundle(tmp_path, duplicate_model_alias=True), interfaces=_interfaces()
    )
    assert state.model("model") is state.model("primary_model")


def test_promptcard_exposes_typed_prompt(tmp_path: Path) -> None:
    """Prompt access returns a usable typed PromptCard prompt value."""
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=_interfaces())
    assert state.prompt("triage_prompt").prompt.model == "gpt-4o"


def test_agentcard_resolves_inline_prompt(tmp_path: Path) -> None:
    """Inline Agent prompt bodies hydrate into a typed prompt without a registry."""
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=_interfaces())
    assert state.agent("agent_inline").prompt.model == "gpt-4o"


def test_agentcard_resolves_registered_prompt(tmp_path: Path) -> None:
    """Referenced Agent prompt bodies resolve through the local graph."""
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=_interfaces())
    assert state.agent("agent_triage").prompt.model == "gpt-4o"


def test_builtin_model_loads_from_bundle_artifact_directory(tmp_path: Path) -> None:
    """Built-in Sklearn hydration loads a usable model from local joblib bytes."""
    bundle = build_builtin_model_bundle(tmp_path)
    state = WyrdState.from_path(
        bundle, interfaces={"backup": TinyModelInterface(), "training_data": TinyDataInterface()}
    )
    assert state.model("model").interface.kind == "Sklearn"
    assert state.model("model").model.predict([[0.0]]) is not None
    artifact = state.artifacts("model")[0]
    assert artifact.local_path.is_file()
    assert artifact.local_path.parent == bundle / "cards/model/artifacts"
    assert artifact.relative_path == "model.joblib"
    assert artifact.size_bytes > 0
    assert artifact.content_type == "application/octet-stream"
    assert artifact.sha256


def test_custom_model_exposes_model_and_preprocessor(tmp_path: Path) -> None:
    """Custom model interfaces expose loaded model and preprocessor objects."""
    interface = TinyModelInterface()
    bundle = build_complete_bundle(tmp_path)
    state = WyrdState.from_path(bundle, interfaces={**_interfaces(), "model": interface})
    assert state.model("model").model is not None
    assert state.model("model").preprocessor is not None
    assert interface.loaded_path == bundle / "cards/model/artifacts"


def test_custom_data_exposes_loaded_data(tmp_path: Path) -> None:
    """Custom data interfaces expose their loaded local data value."""
    interface = TinyDataInterface()
    bundle = build_complete_bundle(tmp_path)
    state = WyrdState.from_path(bundle, interfaces={**_interfaces(), "training_data": interface})
    assert state.data("training_data").data == {"rows": [{"value": 1}]}
    assert interface.loaded_path == bundle / "cards/training/artifacts"


def test_model_and_data_load_kwargs_are_forwarded_by_alias(tmp_path: Path) -> None:
    """Alias-specific loader kwargs reach each selected custom interface."""
    model, data = TinyModelInterface(), TinyDataInterface()
    bundle = build_complete_bundle(tmp_path)
    WyrdState.from_path(
        bundle,
        interfaces={"model": model, "backup": TinyModelInterface(), "training_data": data},
        load_kwargs={"model": {"seed": 1}, "training_data": {"split": "train"}},
    )
    assert model.loaded_kwargs == {"seed": 1}
    assert data.loaded_kwargs == {"split": "train"}
    assert model.loaded_path == bundle / "cards/model/artifacts"
    assert data.loaded_path == bundle / "cards/training/artifacts"


def test_hydrated_objects_survive_gc(tmp_path: Path) -> None:
    """Persistent holders remain usable after temporary aliases and GC are released."""
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=_interfaces())
    model = state.model("model")
    gc.collect()
    assert model.model is not None


def test_missing_custom_interface_names_alias_and_card_ref(tmp_path: Path) -> None:
    """Missing custom interface errors identify the alias and exact CardRef."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(build_complete_bundle(tmp_path))
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "backup"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["card_ref"]["name"] == "backup"


def test_unknown_interface_alias_is_rejected(tmp_path: Path) -> None:
    """Unknown interface aliases fail with the stable alias error details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path),
            interfaces={**_interfaces(), "missing": TinyModelInterface()},
        )
    assert caught.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    assert caught.value.details["alias"] == "missing"
    assert "available_aliases" in caught.value.details


def test_wrong_kind_interface_alias_is_rejected(tmp_path: Path) -> None:
    """A Model interface assigned to Data fails with hydration-stage details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path),
            interfaces={**_interfaces(), "training_data": TinyModelInterface()},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "training_data"
    assert caught.value.details["card_ref"]["kind"] == "Data"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["reason"] == "data interface hydration failed"


def test_unknown_load_kwargs_alias_is_rejected(tmp_path: Path) -> None:
    """Unknown loader-kwargs aliases fail with the stable alias details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path), interfaces=_interfaces(), load_kwargs={"missing": {}}
        )
    assert caught.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    assert caught.value.details["alias"] == "missing"
    assert "available_aliases" in caught.value.details


def test_non_model_data_interface_alias_is_rejected(tmp_path: Path) -> None:
    """Interface mappings cannot target Service or Agent aliases."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path),
            interfaces={**_interfaces(), "root": TinyModelInterface()},
        )
    assert caught.value.code == "WYRD_SDK_400_CARD_KIND_MISMATCH"
    assert caught.value.details["alias"] == "root"
    assert caught.value.details["card_ref"]["kind"] == "Service"
    assert caught.value.details["expected_kind"] == "Model|Data"
    assert caught.value.details["actual_kind"] == "Service"


def test_conflicting_loader_aliases_for_same_card_are_rejected(tmp_path: Path) -> None:
    """Duplicate aliases for one Card reject conflicting interface objects."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path, duplicate_model_alias=True),
            interfaces={**_interfaces(), "primary_model": TinyModelInterface()},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "primary_model"
    assert caught.value.details["card_ref"]["name"] == "model"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["reason"] == (
        "aliases resolving to one Card must use the identical interface object"
    )


def test_equivalent_duplicate_alias_kwargs_are_accepted(tmp_path: Path) -> None:
    """Equivalent normalized kwargs may configure duplicate aliases together."""
    state = WyrdState.from_path(
        build_complete_bundle(tmp_path, duplicate_model_alias=True),
        interfaces=_interfaces(),
        load_kwargs={"model": {"seed": 1}, "primary_model": {"seed": 1}},
    )
    assert state.model("model") is state.model("primary_model")


def test_conflicting_duplicate_alias_kwargs_are_rejected(tmp_path: Path) -> None:
    """Different normalized kwargs for duplicate aliases fail before loading."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path, duplicate_model_alias=True),
            interfaces=_interfaces(),
            load_kwargs={"model": {"seed": 1}, "primary_model": {"seed": 2}},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "primary_model"
    assert caught.value.details["card_ref"]["name"] == "model"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["reason"] == (
        "aliases resolving to one Card must use equivalent loader kwargs"
    )


def test_interface_load_failure_maps_to_runtime_hydration_error(tmp_path: Path) -> None:
    """Interface load failures map to the stable runtime hydration error code."""

    class Failing(TinyModelInterface):
        """Test-only interface that raises during local loading."""

        def load(self, path: Path, load_kwargs=None) -> None:
            """Raise a deterministic local failure for stable error mapping."""
            raise RuntimeError(f"cannot load {path}")

    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            build_complete_bundle(tmp_path), interfaces={**_interfaces(), "model": Failing()}
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model"
    assert caught.value.details["card_ref"]["name"] == "model"
    assert caught.value.details["stage"] == "artifact_load"
    assert caught.value.details["reason"] == "model artifact load failed"


def test_from_path_does_not_read_registry_configuration(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Complete local hydration succeeds without a server URL or registry access."""
    monkeypatch.delenv("WYRD_SERVER_URL", raising=False)
    state = WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces=_interfaces(),
    )
    assert state.card("root").kind.name == "Service"


def test_malformed_bundle_retains_stable_error(tmp_path: Path) -> None:
    """Malformed complete-bundle metadata maps to one stable bundle error code."""
    bundle = build_complete_bundle(tmp_path)
    (bundle / "metadata.yaml").write_text("not: a valid manifest", encoding="utf-8")
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle, interfaces=_interfaces())
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
    assert caught.value.details["path"].endswith("metadata.yaml")
    assert "source" in caught.value.details
