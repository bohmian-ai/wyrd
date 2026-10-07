"""A downloaded Service bundle hydrates into an offline ``WyrdState``, and broken bundles do not."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import numpy as np
import pandas as pd
import pytest
import wyrd
from sklearn.linear_model import LogisticRegression
from wyrd import cli
from wyrd.cards import CardRef, Cards
from wyrd.data import DataCard, FieldSpec, PandasInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface
from wyrd.state import WyrdState

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


@dataclass(frozen=True)
class TypedService:
    """The registered ``typed_state`` Service and the trusted hash of each executable Model alias."""

    ref: CardRef
    trusted: dict[str, str]


def register_model(cards: Cards, name: str) -> CardRef:
    """Register one executable scikit-learn Model the Service graph references."""
    model = LogisticRegression(random_state=0).fit(
        TRAINING_DATA[["feature"]].to_numpy(), TRAINING_DATA["label"].to_numpy()
    )
    card = ModelCard(
        SklearnInterface(model=model),
        space="default",
        name=name,
        version="1.0.0",
        metadata=ModelCardMetadata(
            task_type="other",
            signature=ModelSignature(
                [FieldSpec("feature", "float64")], [FieldSpec("prediction", "float64")]
            ),
        ),
    )
    return cards.model.register(card).root


@pytest.fixture(scope="module")
def typed_service(cards: Cards) -> TypedService:
    """Register the Models, Data, and the ``typed_state`` Service graph once for the module.

    The trusted hashes come from the registered Model Cards, not the download.
    """
    primary = register_model(cards, "primary")
    shadow = register_model(cards, "shadow")
    cards.data.register(
        DataCard(
            PandasInterface(data=TRAINING_DATA), space="default", name="training", version="1.0.0"
        )
    )
    service = cards.register_from_path(str(TYPED_STATE_SOURCE / "typed-service.yaml")).root
    trusted: dict[str, str] = {}
    for alias, ref in (("model_primary", primary), ("model_shadow", shadow)):
        artifact_hash = cards.get(ref)["metadata"]["artifact_hash"]
        assert artifact_hash, f"the registry derived no artifact hash for {alias}"
        trusted[alias] = artifact_hash
    return TypedService(service, trusted)


def download(service: TypedService, output_dir: Path, *, metadata_only: bool = False) -> Path:
    """Download the registered Service's graph into ``output_dir`` with in-process ``wyrd get``."""
    cli.get(
        output_dir=output_dir,
        kind="Service",
        uid=str(service.ref.uid),
        metadata_only=metadata_only,
    )
    return output_dir


@pytest.mark.integration
def test_service_bundle_hydrates_complete_python_runtime_offline(
    typed_service: TypedService, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Hydrate the downloaded bundle into usable Python objects with no server reachable."""
    bundle = download(typed_service, tmp_path)
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:1")

    state = WyrdState.from_path(bundle, trusted_artifact_hashes=typed_service.trusted)

    assert state.service.card_ref == typed_service.ref
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


@pytest.mark.integration
def test_metadata_only_bundle_is_rejected_by_python_state(
    typed_service: TypedService, tmp_path: Path
) -> None:
    """A metadata-only download is inspectable but never hydrates."""
    bundle = download(typed_service, tmp_path, metadata_only=True)
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"


@pytest.mark.integration
def test_tampered_downloaded_artifact_is_rejected_offline(
    typed_service: TypedService, tmp_path: Path
) -> None:
    """Downloaded artifact bytes that no longer match the inventory are refused."""
    bundle = download(typed_service, tmp_path)
    artifacts = bundle / "cards" / "model_primary" / "artifacts"
    next(path for path in artifacts.rglob("*") if path.is_file()).write_bytes(b"tampered")
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle, trusted_artifact_hashes=typed_service.trusted)
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"


@pytest.mark.integration
def test_missing_model_trust_returns_recoverable_runtime_error(
    typed_service: TypedService, tmp_path: Path
) -> None:
    """An executable built-in Model does not hydrate without an external trusted hash."""
    bundle = download(typed_service, tmp_path)
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model_primary"
    assert caught.value.details["stage"] == "artifact_trust"


@pytest.mark.integration
def test_missing_relationship_projection_is_rejected_offline(
    typed_service: TypedService, tmp_path: Path
) -> None:
    """A bundle whose root relationship projection is absent is refused."""
    bundle = download(typed_service, tmp_path)
    (bundle / "cards" / "root" / "relationships.yaml").unlink()
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle, trusted_artifact_hashes=typed_service.trusted)
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
