from __future__ import annotations

import json

import pytest
from _helpers import model_metadata, model_signature
from test_modelcard_save_load import _huggingface_model, _sklearn_model
from wyrd.data import FieldSpec
from wyrd.model import (
    HuggingfaceInterface,
    ModelCard,
    ModelCardMetadata,
    ModelSignature,
    SklearnInterface,
    WyrdError,
)


def test_missing_signature_raises_stable_model_error_code() -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard(_sklearn_model())

    assert exc.value.code == "WYRD_MODEL_400_MISSING_SIGNATURE"


def test_invalid_signature_dtype_raises_stable_model_error_code() -> None:
    with pytest.raises(WyrdError) as exc:
        ModelSignature(
            [FieldSpec("feature", "not_a_dtype")],
            [FieldSpec("prediction", "float64")],
        )

    assert exc.value.code == "WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED"


def test_invalid_signature_shape_raises_stable_model_error_code() -> None:
    signature = {
        "inputs": [
            {
                "name": "feature",
                "dtype": "float64",
                "nullable": False,
                "shape": [{"kind": "Fixed", "value": 0}],
            }
        ],
        "outputs": [{"name": "prediction", "dtype": "float64", "nullable": False}],
    }

    with pytest.raises(WyrdError) as exc:
        ModelCard(
            _sklearn_model(),
            metadata=ModelCardMetadata(task_type="regression", signature=signature),
        )

    assert exc.value.code == "WYRD_MODEL_400_SHAPE_INVALID"


def test_unsupported_raw_model_object_raises_stable_model_error_code() -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard(object(), metadata=model_metadata("regression"))

    assert exc.value.code == "WYRD_MODEL_400_UNKNOWN_MODEL_TYPE"


def test_interface_class_passed_to_constructor_raises_model_error() -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard(SklearnInterface, metadata=model_metadata("regression"))

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"
    assert "DataCard" not in str(exc.value)


@pytest.mark.parametrize(
    ("filename", "message"),
    [
        ("model.safetensors", "explicit TorchInterface"),
        ("model.ckpt", "explicit LightningInterface"),
        ("model.joblib", "requires metadata.interface"),
    ],
)
def test_ambiguous_model_artifact_paths_require_explicit_interface(
    tmp_path,
    filename: str,
    message: str,
) -> None:
    artifact = tmp_path / filename
    artifact.write_bytes(b"placeholder")

    with pytest.raises(WyrdError) as exc:
        ModelCard(artifact, metadata=model_metadata("regression"))

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"
    assert message in str(exc.value)


def test_unknown_model_artifact_path_does_not_leak_absolute_path(tmp_path) -> None:
    artifact = tmp_path / "private-model.bin"
    artifact.write_bytes(b"placeholder")

    with pytest.raises(WyrdError) as exc:
        ModelCard(artifact, metadata=model_metadata("regression"))

    assert exc.value.code == "WYRD_MODEL_400_UNKNOWN_MODEL_TYPE"
    assert "private-model.bin" in str(exc.value)
    assert str(tmp_path) not in str(exc.value)


def test_invalid_huggingface_revision_raises_stable_model_error_code() -> None:
    interface = HuggingfaceInterface(
        model=_huggingface_model(),
        hf_task="text-classification",
        revision="bad-rev",
    )

    with pytest.raises(WyrdError) as exc:
        ModelCard(interface, metadata=model_metadata("binary_classification"))

    assert exc.value.code == "WYRD_MODEL_400_HF_REVISION_INVALID"


def test_invalid_huggingface_repo_id_raises_model_validation_error() -> None:
    interface = HuggingfaceInterface(
        model=_huggingface_model(),
        hf_task="text-classification",
        repo_id="",
    )

    with pytest.raises(WyrdError) as exc:
        ModelCard(interface, metadata=model_metadata("binary_classification"))

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_invalid_huggingface_task_raises_stable_option_error() -> None:
    with pytest.raises(WyrdError) as exc:
        HuggingfaceInterface(model=_huggingface_model(), hf_task="not-a-task")

    assert exc.value.code == "WYRD_DATA_400_INVALID_INTERFACE_OPTION"


def test_custom_json_without_explicit_interface_raises_stable_model_error_code() -> None:
    payload = {
        "apiVersion": "wyrd/v1",
        "kind": "Model",
        "metadata": {"name": "custom", "version": "1.0.0"},
        "spec": {
            "interface": {
                "kind": "Custom",
                "meta": {
                    "framework_version": "python",
                    "model_subtype": None,
                    "loader_module": "",
                    "loader_class": "",
                    "extra": {},
                },
            },
            "task_type": "Other",
            "signature": model_signature().to_dict(),
            "card_refs": [],
        },
        "relationships": [],
    }

    with pytest.raises(WyrdError) as exc:
        ModelCard.model_validate_json(json.dumps(payload))

    assert exc.value.code == "WYRD_MODEL_400_CUSTOM_LOADER_INVALID"
