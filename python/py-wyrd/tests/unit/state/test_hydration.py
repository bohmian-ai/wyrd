"""Offline holder hydration and loader-configuration contracts."""

import gc
import weakref
from collections import UserDict
from pathlib import Path

import joblib
import pytest
import wyrd
from wyrd.cards import DataLoadArgs, ModelLoadArgs
from wyrd.model import ModelCard
from wyrd.state import WyrdState

from .support import (
    TinyDataInterface,
    TinyModelInterface,
    build_builtin_model_bundle,
    build_complete_bundle,
    trusted_artifact_hash,
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


def test_builtin_model_loads_from_bundle_artifact_directory(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Joblib-backed built-ins require externally supplied exact manifest trust."""
    bundle = build_builtin_model_bundle(tmp_path)
    original_load = joblib.load
    load_calls = 0

    def tracking_load(*args, **kwargs):
        """Count executable deserialization without changing joblib behavior."""
        nonlocal load_calls
        load_calls += 1
        return original_load(*args, **kwargs)

    monkeypatch.setattr(joblib, "load", tracking_load)
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            bundle,
            interfaces={"backup": TinyModelInterface(), "training_data": TinyDataInterface()},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model"
    assert caught.value.details["stage"] == "artifact_trust"
    assert caught.value.details["reason"] == (
        "executable model requires an exact trusted artifact manifest hash"
    )

    with pytest.raises(wyrd.WyrdError) as wrong_hash:
        WyrdState.from_path(
            bundle,
            interfaces={"backup": TinyModelInterface(), "training_data": TinyDataInterface()},
            trusted_artifact_hashes={"model": "not-the-canonical-hash"},
        )
    assert wrong_hash.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert wrong_hash.value.details["stage"] == "artifact_trust"
    assert wrong_hash.value.details["reason"] == "trusted artifact manifest hash does not match"
    assert load_calls == 0

    state = WyrdState.from_path(
        bundle,
        interfaces={"backup": TinyModelInterface(), "training_data": TinyDataInterface()},
        trusted_artifact_hashes={"model": trusted_artifact_hash(bundle, "model")},
    )
    prediction = state.model("model").model.predict([[0.0]])
    assert prediction.shape == (1,)
    assert load_calls == 1


def test_builtin_override_is_rejected_before_artifact_load(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A contract-changing override cannot reach executable deserialization."""
    bundle = build_builtin_model_bundle(tmp_path)
    load_calls = 0

    def forbidden_load(*args, **kwargs):
        """Record an invalid executable load attempt."""
        del args, kwargs
        nonlocal load_calls
        load_calls += 1
        raise AssertionError("joblib.load must not run")

    monkeypatch.setattr(joblib, "load", forbidden_load)
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            bundle,
            interfaces={
                "model": TinyModelInterface(),
                "backup": TinyModelInterface(),
                "training_data": TinyDataInterface(),
            },
            trusted_artifact_hashes={"model": trusted_artifact_hash(bundle, "model")},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model"
    assert caught.value.details["stage"] == "interface"
    assert load_calls == 0


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


def test_typed_load_args_are_forwarded_as_dicts(tmp_path: Path) -> None:
    """Typed ModelLoadArgs and DataLoadArgs reach custom interfaces as dictionaries."""
    model, data = TinyModelInterface(), TinyDataInterface()
    WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces={"model": model, "backup": TinyModelInterface(), "training_data": data},
        load_kwargs={
            "model": ModelLoadArgs({"seed": 7}),
            "training_data": DataLoadArgs({"split": "validation"}),
        },
    )
    assert model.loaded_kwargs == {"seed": 7}
    assert data.loaded_kwargs == {"split": "validation"}


def test_mapping_load_kwargs_are_materialized_as_dicts(tmp_path: Path) -> None:
    """Non-dict Mapping loader arguments are materialized before forwarding."""
    model = TinyModelInterface()
    WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces={
            "model": model,
            "backup": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
        load_kwargs={"model": UserDict({"seed": 11})},
    )
    assert model.loaded_kwargs == {"seed": 11}


def test_hydrated_objects_survive_gc(tmp_path: Path) -> None:
    """Persistent holders remain usable after temporary aliases and GC are released."""
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=_interfaces())
    model = state.model("model")
    gc.collect()
    assert model.model is not None


def test_state_interface_cycle_is_collected(tmp_path: Path) -> None:
    """State and retained interface cycles release both Python objects."""
    model = TinyModelInterface()
    interfaces = {
        "model": model,
        "backup": TinyModelInterface(),
        "training_data": TinyDataInterface(),
    }
    state = WyrdState.from_path(build_complete_bundle(tmp_path), interfaces=interfaces)
    model.state = state
    state_ref = weakref.ref(state)
    interface_ref = weakref.ref(model)

    del state
    del model
    del interfaces
    gc.collect()

    assert state_ref() is None
    assert interface_ref() is None


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
    assert isinstance(caught.value.__cause__, RuntimeError)
    assert "cannot load" in str(caught.value.__cause__)


def test_late_loader_failure_publishes_no_state(tmp_path: Path) -> None:
    """All Model loaders run before a later Data failure aborts publication."""

    class FailingData(TinyDataInterface):
        """Test-only Data interface that fails after Model hydration."""

        def load(self, path: Path, load_kwargs=None) -> None:
            """Raise at the final runtime-relevant holder stage."""
            del path, load_kwargs
            raise RuntimeError("late data failure")

    model = TinyModelInterface()
    backup = TinyModelInterface()
    state = None
    with pytest.raises(wyrd.WyrdError) as caught:
        state = WyrdState.from_path(
            build_complete_bundle(tmp_path),
            interfaces={
                "model": model,
                "backup": backup,
                "training_data": FailingData(),
            },
        )

    assert state is None
    assert model.loaded_path is not None
    assert backup.loaded_path is not None
    assert caught.value.details["alias"] == "training_data"
    assert caught.value.details["stage"] == "artifact_load"
    assert isinstance(caught.value.__cause__, RuntimeError)
    assert "late data failure" in str(caught.value.__cause__)


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
