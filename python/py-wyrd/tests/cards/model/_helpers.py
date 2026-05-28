from __future__ import annotations

import json
from pathlib import Path

from wyrd.data import FieldSpec
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SampleInput


def model_signature(
    input_name: str = "feature",
    input_dtype: str = "float64",
    output_name: str = "prediction",
    output_dtype: str = "float64",
) -> ModelSignature:
    return ModelSignature(
        [FieldSpec(input_name, input_dtype)],
        [FieldSpec(output_name, output_dtype)],
    )


def model_metadata(
    task_type: str = "regression",
    interface=None,
    signature: ModelSignature | dict | None = None,
    sample_input: SampleInput | None = None,
) -> ModelCardMetadata:
    return ModelCardMetadata(
        interface=interface,
        task_type=task_type,
        signature=signature or model_signature(),
        sample_input=sample_input,
    )


def card_payload(card: ModelCard) -> dict:
    return json.loads(card.model_dump_json())


def saved_card_payload(path: Path) -> dict:
    return json.loads((path / "card.json").read_text(encoding="utf-8"))


def assert_model_card_json(path: Path, interface_kind: str) -> None:
    payload = saved_card_payload(path)
    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Model"
    assert payload["spec"]["interface"]["kind"] == interface_kind
    assert payload["spec"]["artifact_refs"] == []
    assert payload["relationships"] == []
