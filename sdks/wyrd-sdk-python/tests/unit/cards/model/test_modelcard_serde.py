"""A ModelCard's JSON envelope round-trips, and a Model Card file loads."""

import json
from pathlib import Path
from typing import Any

import pytest
from wyrd.model import ModelCard, ModelCardMetadata, SklearnInterface, WyrdError


def test_json_envelope_round_trips_the_card(
    sklearn_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    card = ModelCard(
        SklearnInterface(model=sklearn_model),
        labels={"domain": "forecasting"},
        annotations={"acme.com/source": "notebook"},
        metadata=classifier_metadata,
    )

    restored = ModelCard.model_validate_json(card.model_dump_json())

    assert (
        restored.interface.kind,
        restored.labels,
        restored.annotations,
        restored.signature.inputs[0].name,
    ) == (
        "Sklearn",
        {"domain": "forecasting"},
        {"acme.com/source": "notebook"},
        "feature",
    )


def test_modelcard_str_is_pretty_card_json(
    sklearn_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    payload = json.loads(
        str(ModelCard(SklearnInterface(model=sklearn_model), metadata=classifier_metadata))
    )

    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Model"


def test_data_card_file_is_refused_as_a_model_card(fixtures_dir: Path) -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard.from_path(fixtures_dir / "authoring" / "data" / "churn-features.yaml")

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_model_card_file_loads_its_identity_and_task(fixtures_dir: Path) -> None:
    card = ModelCard.from_path(fixtures_dir / "authoring" / "model" / "churn-model.yaml")

    assert (card.space, card.name, card.task_type) == (
        "prod",
        "churn-model",
        "binary_classification",
    )
