from __future__ import annotations

import json
from pathlib import Path

import pytest
import yaml
from _helpers import card_payload, model_metadata
from test_modelcard_save_load import _sklearn_model
from wyrd.model import ModelCard, SklearnInterface, WyrdError


@pytest.mark.wyrd_covers("python:ModelCard.model_dump_json")
@pytest.mark.wyrd_covers("python:ModelCard.model_validate_json")
def test_model_dump_json_round_trips_through_public_validator() -> None:
    card = ModelCard(
        SklearnInterface(model=_sklearn_model()),
        labels={"domain": "forecasting"},
        annotations={"acme.com/source": "notebook"},
        metadata=model_metadata("binary_classification"),
    )

    restored = ModelCard.model_validate_json(card.model_dump_json())
    payload = card_payload(restored)

    assert restored.interface.kind == "Sklearn"
    assert restored.labels == {"domain": "forecasting"}
    assert restored.annotations == {"acme.com/source": "notebook"}
    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Model"
    assert payload["spec"]["artifact_refs"] == []
    assert "tags" not in payload["metadata"]


def test_model_validate_json_rejects_wrong_card_kind() -> None:
    payload = {
        "apiVersion": "wyrd/v1",
        "kind": "Data",
        "metadata": {"name": "model", "version": "1.0.0"},
        "spec": {"schema": {"columns": []}},
    }

    with pytest.raises(WyrdError):
        ModelCard.model_validate_json(json.dumps(payload))


def test_modelcard_yaml_fixture_loads_through_public_surface() -> None:
    payload = yaml.safe_load(
        """
apiVersion: wyrd/v1
kind: Model
metadata:
  space: prod
  name: churn-model
  version: "1.0.0"
spec:
  interface:
    kind: Sklearn
    meta:
      framework_version: "1.5.0"
      model_subtype: LogisticRegression
  task_type: BinaryClassification
  signature:
    inputs:
      - name: feature
        dtype: float64
        nullable: false
    outputs:
      - name: prediction
        dtype: float64
        nullable: false
  artifact_refs: []
relationships: []
"""
    )

    card = ModelCard.model_validate_json(json.dumps(payload))

    assert card.space == "prod"
    assert card.name == "churn-model"
    assert card.task_type == "binary_classification"


def test_modelcard_yaml_round_trips_locked_envelope(tmp_path: Path) -> None:
    card = ModelCard(
        SklearnInterface(model=_sklearn_model()),
        metadata=model_metadata("binary_classification"),
    )
    payload = yaml.safe_load(yaml.safe_dump(card_payload(card)))
    restored = ModelCard.model_validate_json(json.dumps(payload))

    assert restored.interface.kind == "Sklearn"
    assert restored.signature.inputs[0].name == "feature"
