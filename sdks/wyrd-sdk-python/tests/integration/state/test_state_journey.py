"""A downloaded Service bundle hydrates into an offline ``WyrdState``, and broken bundles do not."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd
import pytest
import wyrd
from sklearn.linear_model import LogisticRegression
from wyrd.cards import CardRef, Cards
from wyrd.data import DataCard, FieldSpec, PandasInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface
from wyrd.state import WyrdState
from wyrd.testing import cli

pytestmark = pytest.mark.integration

TYPED_STATE_SOURCE = (
    Path(__file__).resolve().parents[5]
    / "crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle/typed_state"
)
EXPECTED_ALIASES = (
    "agent_inline",
    "agent_triage",
    "default-Agent-triage-1.0.0",
    "default-Data-training-1.0.0",
    "default-Prompt-triage-prompt-1.0.0",
    "default-Verifier-model-drift-1.0.0",
    "default-Verifier-quality-1.0.0",
    "model_primary",
    "model_shadow",
    "root",
    "runtime_workflow",
    "shared_prompt",
    "triage_prompt",
)
TRAINING_DATA = pd.DataFrame({"feature": [0.0, 1.0], "label": [0, 1]})


@pytest.fixture(scope="module")
def models(cards: Cards) -> dict[str, CardRef]:
    """The two executable scikit-learn Models the Service graph references, by alias."""
    model = LogisticRegression(random_state=0).fit(
        TRAINING_DATA[["feature"]].to_numpy(), TRAINING_DATA["label"].to_numpy()
    )
    metadata = ModelCardMetadata(
        task_type="other",
        signature=ModelSignature(
            [FieldSpec("feature", "float64")], [FieldSpec("prediction", "float64")]
        ),
    )
    return {
        alias: cards.model.register(
            ModelCard(
                SklearnInterface(model=model),
                space="default",
                name=name,
                version="1.0.0",
                metadata=metadata,
            )
        ).root
        for alias, name in (("model_primary", "primary"), ("model_shadow", "shadow"))
    }


@pytest.fixture(scope="module")
def service(cards: Cards, models: dict[str, CardRef]) -> CardRef:
    """The registered ``typed_state`` Service, after its Models and training Data."""
    cards.data.register(
        DataCard(
            PandasInterface(data=TRAINING_DATA), space="default", name="training", version="1.0.0"
        )
    )
    return cards.register_from_path(TYPED_STATE_SOURCE / "typed-service.yaml").root


@pytest.fixture(scope="module")
def trusted(cards: Cards, models: dict[str, CardRef]) -> dict[str, str]:
    """The trusted artifact hash of each executable Model alias, read from the registry, not the download."""
    hashes: dict[str, str] = {}
    for alias, ref in models.items():
        artifact_hash = cards.get(ref).metadata.artifact_hash
        assert artifact_hash, f"the registry derived no artifact hash for {alias}"
        hashes[alias] = artifact_hash
    return hashes


@pytest.fixture
def bundle(service: CardRef, tmp_path: Path) -> Path:
    """The Service's complete graph, downloaded with ``wyrd get``."""
    cli.get(output_dir=tmp_path, kind="Service", uid=str(service.uid))
    return tmp_path


@pytest.fixture
def metadata_bundle(service: CardRef, tmp_path: Path) -> Path:
    """The Service's graph downloaded metadata-only, without artifact bytes."""
    cli.get(output_dir=tmp_path, kind="Service", uid=str(service.uid), metadata_only=True)
    return tmp_path


def test_service_bundle_hydrates_complete_python_runtime_offline(
    service: CardRef, trusted: dict[str, str], bundle: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Hydrate the downloaded bundle into usable Python objects with no server reachable."""
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:1")

    state = WyrdState.from_path(bundle, trusted_artifact_hashes=trusted)

    assert state.root_ref == state.service.card_ref == service
    assert state.aliases == EXPECTED_ALIASES
    for alias in ("model_primary", "model_shadow"):
        np.testing.assert_array_equal(
            state.model(alias).model.predict([[0.0], [1.0]]), np.array([0, 1])
        )
    assert state.model("model_primary") is not state.model("model_shadow")
    pd.testing.assert_frame_equal(state.data("default-Data-training-1.0.0").data, TRAINING_DATA)
    assert state.agent("agent_triage").prompt is not None
    assert state.agent("agent_inline").prompt is not None
    assert state.prompt("triage_prompt") is state.prompt("default-Prompt-triage-prompt-1.0.0")
    assert state.prompt("triage_prompt") is state.prompt("shared_prompt")
    quality = state.verifier("default-Verifier-quality-1.0.0")
    assert quality.kind is wyrd.CardKind.Verifier
    assert quality.spec["implementation"]["kind"] == "eval"
    drift = state.verifier("default-Verifier-model-drift-1.0.0")
    assert drift.kind is wyrd.CardKind.Verifier
    assert drift.spec["implementation"]["kind"] == "drift"
    assert state.workflow("runtime_workflow").spec["outputs"] == {
        "answer": "steps.triage.output.text"
    }
    for alias in state.aliases:
        ref = state.card_ref(alias)
        assert ref.uid is not None and ref.version
    for alias in ("model_primary", "model_shadow", "default-Data-training-1.0.0"):
        artifact = state.artifacts(alias)[0]
        assert artifact.local_path.is_file()
        assert artifact.local_path.is_relative_to(bundle.resolve())


def test_metadata_only_bundle_is_rejected_by_python_state(metadata_bundle: Path) -> None:
    """A metadata-only download is inspectable but never hydrates."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(metadata_bundle)
    assert caught.value.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"


def test_tampered_downloaded_artifact_is_rejected_offline(
    trusted: dict[str, str], bundle: Path
) -> None:
    """Downloaded artifact bytes that no longer match the inventory are refused."""
    artifacts = bundle / "cards" / "model_primary" / "artifacts"
    next(path for path in artifacts.rglob("*") if path.is_file()).write_bytes(b"tampered")
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle, trusted_artifact_hashes=trusted)
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"


def test_missing_model_trust_returns_recoverable_runtime_error(bundle: Path) -> None:
    """An executable built-in Model does not hydrate without an external trusted hash."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model_primary"
    assert caught.value.details["stage"] == "artifact_trust"


def test_missing_relationship_projection_is_rejected_offline(
    trusted: dict[str, str], bundle: Path
) -> None:
    """A bundle whose root relationship projection is absent is refused."""
    (bundle / "cards" / "root" / "relationships.yaml").unlink()
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle, trusted_artifact_hashes=trusted)
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
