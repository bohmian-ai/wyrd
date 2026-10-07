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

from .support import TinyDataInterface, TinyModelInterface


@pytest.fixture
def joblib_loads(monkeypatch: pytest.MonkeyPatch) -> list[str]:
    """Record every executable ``joblib.load`` without changing its behavior."""
    calls: list[str] = []
    original_load = joblib.load

    def tracking_load(filename, *args, **kwargs):
        calls.append(str(filename))
        return original_load(filename, *args, **kwargs)

    monkeypatch.setattr(joblib, "load", tracking_load)
    return calls


@pytest.fixture
def builtin_interfaces() -> dict[str, object]:
    """Custom interfaces for every builtin-model alias except the sklearn ``model``."""
    return {"backup": TinyModelInterface(), "training_data": TinyDataInterface()}


def test_two_model_aliases_return_distinct_modelcards(state: WyrdState) -> None:
    """Distinct exact Model Cards produce distinct persistent ModelCard objects."""
    primary = state.model("model")
    backup = state.model("backup")
    assert isinstance(primary, ModelCard)
    assert isinstance(backup, ModelCard)
    assert primary is not backup


def test_duplicate_aliases_return_identical_python_object(state: WyrdState) -> None:
    """Aliases for one exact CardRef return one shared Python holder."""
    assert state.model("model") is state.model("primary_model")


def test_promptcard_exposes_typed_prompt(state: WyrdState) -> None:
    """Prompt access returns a usable typed PromptCard prompt value."""
    assert state.prompt("triage_prompt").prompt.model == "gpt-4o"


def test_agentcard_resolves_inline_prompt(state: WyrdState) -> None:
    """Inline Agent prompt bodies hydrate into a typed prompt without a registry."""
    prompt = state.agent("agent_inline").prompt
    assert prompt is not None
    assert prompt.model == "gpt-4o"


def test_agentcard_resolves_registered_prompt(state: WyrdState) -> None:
    """Referenced Agent prompt bodies resolve through the local graph."""
    prompt = state.agent("agent_triage").prompt
    assert prompt is not None
    assert prompt.model == "gpt-4o"


def test_builtin_model_without_a_trusted_hash_is_refused_before_load(
    builtin_model_bundle: Path,
    builtin_interfaces: dict[str, TinyModelInterface | TinyDataInterface],
    joblib_loads: list[str],
) -> None:
    """An executable built-in Model needs an externally supplied manifest hash."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(builtin_model_bundle, interfaces=builtin_interfaces)
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model"
    assert caught.value.details["stage"] == "artifact_trust"
    assert caught.value.details["reason"] == (
        "executable model requires an exact trusted artifact manifest hash"
    )
    assert joblib_loads == []


def test_builtin_model_with_a_wrong_trusted_hash_is_refused_before_load(
    builtin_model_bundle: Path,
    builtin_interfaces: dict[str, TinyModelInterface | TinyDataInterface],
    joblib_loads: list[str],
) -> None:
    """A trusted hash that does not match the manifest never reaches joblib."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            builtin_model_bundle,
            interfaces=builtin_interfaces,
            trusted_artifact_hashes={"model": "not-the-canonical-hash"},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["stage"] == "artifact_trust"
    assert caught.value.details["reason"] == "trusted artifact manifest hash does not match"
    assert joblib_loads == []


def test_builtin_model_with_its_trusted_hash_loads_and_predicts(
    builtin_model_bundle: Path,
    builtin_interfaces: dict[str, TinyModelInterface | TinyDataInterface],
    trusted_model_hash: str,
    joblib_loads: list[str],
) -> None:
    """The exact trusted hash loads the sklearn artifact once, ready to predict."""
    state = WyrdState.from_path(
        builtin_model_bundle,
        interfaces=builtin_interfaces,
        trusted_artifact_hashes={"model": trusted_model_hash},
    )
    assert state.model("model").model.predict([[0.0]]).shape == (1,)
    assert len(joblib_loads) == 1


def test_builtin_override_is_rejected_before_artifact_load(
    builtin_model_bundle: Path,
    interfaces: dict[str, TinyModelInterface | TinyDataInterface],
    trusted_model_hash: str,
    joblib_loads: list[str],
) -> None:
    """A contract-changing override cannot reach executable deserialization."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            builtin_model_bundle,
            interfaces=interfaces,
            trusted_artifact_hashes={"model": trusted_model_hash},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model"
    assert caught.value.details["stage"] == "interface"
    assert joblib_loads == []


def test_custom_model_exposes_model_and_preprocessor(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Custom model interfaces expose loaded model and preprocessor objects."""
    state = WyrdState.from_path(complete_bundle, interfaces=interfaces)
    assert state.model("model").model is not None
    assert state.model("model").preprocessor is not None
    assert interfaces["model"].loaded_path == complete_bundle / "cards/model/artifacts"


def test_custom_data_exposes_loaded_data(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Custom data interfaces expose their loaded local data value."""
    state = WyrdState.from_path(complete_bundle, interfaces=interfaces)
    assert state.data("training_data").data == {"rows": [{"value": 1}]}
    assert interfaces["training_data"].loaded_path == complete_bundle / "cards/training/artifacts"


def test_model_and_data_load_kwargs_are_forwarded_by_alias(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Alias-specific loader kwargs reach each selected custom interface."""
    WyrdState.from_path(
        complete_bundle,
        interfaces=interfaces,
        load_kwargs={"model": {"seed": 1}, "training_data": {"split": "train"}},
    )
    assert interfaces["model"].loaded_kwargs == {"seed": 1}
    assert interfaces["training_data"].loaded_kwargs == {"split": "train"}


def test_typed_load_args_are_forwarded_as_dicts(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Typed ModelLoadArgs and DataLoadArgs reach custom interfaces as dictionaries."""
    WyrdState.from_path(
        complete_bundle,
        interfaces=interfaces,
        load_kwargs={
            "model": ModelLoadArgs({"seed": 7}),
            "training_data": DataLoadArgs({"split": "validation"}),
        },
    )
    assert interfaces["model"].loaded_kwargs == {"seed": 7}
    assert interfaces["training_data"].loaded_kwargs == {"split": "validation"}


def test_mapping_load_kwargs_are_materialized_as_dicts(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Non-dict Mapping loader arguments are materialized before forwarding."""
    WyrdState.from_path(
        complete_bundle, interfaces=interfaces, load_kwargs={"model": UserDict({"seed": 11})}
    )
    assert interfaces["model"].loaded_kwargs == {"seed": 11}


def test_hydrated_objects_survive_gc(state: WyrdState) -> None:
    """Persistent holders remain usable after temporary aliases and GC are released."""
    model = state.model("model")
    gc.collect()
    assert model.model is not None


def test_state_interface_cycle_is_collected(complete_bundle: Path) -> None:
    """State and retained interface cycles release both Python objects."""
    model = TinyModelInterface()
    interfaces = {
        "model": model,
        "backup": TinyModelInterface(),
        "training_data": TinyDataInterface(),
    }
    state = WyrdState.from_path(complete_bundle, interfaces=interfaces)
    model.state = state
    state_ref = weakref.ref(state)
    interface_ref = weakref.ref(model)

    del state
    del model
    del interfaces
    gc.collect()

    assert state_ref() is None
    assert interface_ref() is None


def test_missing_custom_interface_names_alias_and_card_ref(complete_bundle: Path) -> None:
    """Missing custom interface errors identify the alias and exact CardRef."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(complete_bundle)
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "backup"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["card_ref"]["name"] == "backup"


def test_unknown_interface_alias_is_rejected(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Unknown interface aliases fail with the stable alias error details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            complete_bundle, interfaces={**interfaces, "missing": TinyModelInterface()}
        )
    assert caught.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    assert caught.value.details["alias"] == "missing"
    assert "available_aliases" in caught.value.details


def test_wrong_kind_interface_alias_is_rejected(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """A Model interface assigned to Data fails with hydration-stage details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            complete_bundle, interfaces={**interfaces, "training_data": TinyModelInterface()}
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "training_data"
    assert caught.value.details["card_ref"]["kind"] == "Data"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["reason"] == "data interface hydration failed"


def test_unknown_load_kwargs_alias_is_rejected(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Unknown loader-kwargs aliases fail with the stable alias details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(complete_bundle, interfaces=interfaces, load_kwargs={"missing": {}})
    assert caught.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    assert caught.value.details["alias"] == "missing"
    assert "available_aliases" in caught.value.details


def test_non_model_data_interface_alias_is_rejected(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Interface mappings cannot target Service or Agent aliases."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            complete_bundle, interfaces={**interfaces, "root": TinyModelInterface()}
        )
    assert caught.value.code == "WYRD_SDK_400_CARD_KIND_MISMATCH"
    assert caught.value.details["alias"] == "root"
    assert caught.value.details["card_ref"]["kind"] == "Service"
    assert caught.value.details["expected_kind"] == "Model|Data"
    assert caught.value.details["actual_kind"] == "Service"


def test_conflicting_loader_aliases_for_same_card_are_rejected(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Duplicate aliases for one Card reject conflicting interface objects."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            complete_bundle, interfaces={**interfaces, "primary_model": TinyModelInterface()}
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "primary_model"
    assert caught.value.details["card_ref"]["name"] == "model"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["reason"] == (
        "aliases resolving to one Card must use the identical interface object"
    )


def test_equivalent_duplicate_alias_kwargs_are_accepted(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Equivalent normalized kwargs may configure duplicate aliases together."""
    state = WyrdState.from_path(
        complete_bundle,
        interfaces=interfaces,
        load_kwargs={"model": {"seed": 1}, "primary_model": {"seed": 1}},
    )
    assert state.model("model") is state.model("primary_model")


def test_conflicting_duplicate_alias_kwargs_are_rejected(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Different normalized kwargs for duplicate aliases fail before loading."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            complete_bundle,
            interfaces=interfaces,
            load_kwargs={"model": {"seed": 1}, "primary_model": {"seed": 2}},
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "primary_model"
    assert caught.value.details["card_ref"]["name"] == "model"
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["reason"] == (
        "aliases resolving to one Card must use equivalent loader kwargs"
    )


class FailingModelInterface(TinyModelInterface):
    """Test-only Model interface that raises during local loading."""

    def load(self, path: Path, load_kwargs=None) -> None:
        """Raise a deterministic local failure for stable error mapping."""
        raise RuntimeError(f"cannot load {path}")


class FailingDataInterface(TinyDataInterface):
    """Test-only Data interface that fails after Model hydration."""

    def load(self, path: Path, load_kwargs=None) -> None:
        """Raise at the final runtime-relevant holder stage."""
        del path, load_kwargs
        raise RuntimeError("late data failure")


def test_interface_load_failure_maps_to_runtime_hydration_error(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Interface load failures map to the stable runtime hydration error code."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            complete_bundle, interfaces={**interfaces, "model": FailingModelInterface()}
        )
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model"
    assert caught.value.details["card_ref"]["name"] == "model"
    assert caught.value.details["stage"] == "artifact_load"
    assert caught.value.details["reason"] == "model artifact load failed"
    assert isinstance(caught.value.__cause__, RuntimeError)
    assert "cannot load" in str(caught.value.__cause__)


def test_late_loader_failure_publishes_no_state(
    complete_bundle: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """All Model loaders run before a later Data failure aborts publication."""
    state = None
    with pytest.raises(wyrd.WyrdError) as caught:
        state = WyrdState.from_path(
            complete_bundle, interfaces={**interfaces, "training_data": FailingDataInterface()}
        )

    assert state is None
    assert interfaces["model"].loaded_path is not None
    assert interfaces["backup"].loaded_path is not None
    assert caught.value.details["alias"] == "training_data"
    assert caught.value.details["stage"] == "artifact_load"
    assert isinstance(caught.value.__cause__, RuntimeError)
    assert "late data failure" in str(caught.value.__cause__)


def test_from_path_does_not_read_registry_configuration(
    complete_bundle: Path,
    interfaces: dict[str, TinyModelInterface | TinyDataInterface],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Complete local hydration succeeds without a server URL or registry access."""
    monkeypatch.delenv("WYRD_SERVER_URL", raising=False)
    state = WyrdState.from_path(complete_bundle, interfaces=interfaces)
    assert state.card("root").kind.name == "Service"


def test_malformed_bundle_retains_stable_error(
    fixtures_dir: Path, interfaces: dict[str, TinyModelInterface | TinyDataInterface]
) -> None:
    """Malformed complete-bundle metadata maps to one stable bundle error code."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            fixtures_dir / "invalid" / "bundles" / "malformed-manifest", interfaces=interfaces
        )
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
    assert caught.value.details["path"].endswith("metadata.yaml")
    assert "source" in caught.value.details


def test_metadata_only_bundle_is_rejected_with_stable_error(fixtures_dir: Path) -> None:
    """A metadata-only bundle carries no artifacts, so it cannot hydrate."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(fixtures_dir / "bundles" / "metadata-only")
    assert caught.value.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"
