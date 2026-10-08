"""Invalid ModelCard inputs are refused with a stable model error code."""

from pathlib import Path
from typing import Any

import pytest
from wyrd.data import Dim, FieldSpec
from wyrd.model import (
    HuggingfaceInterface,
    ModelCard,
    ModelCardMetadata,
    ModelSignature,
    SklearnInterface,
    WyrdError,
)


def test_missing_signature_raises_stable_model_error_code(sklearn_model: Any) -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard(sklearn_model)

    assert exc.value.code == "WYRD_MODEL_400_MISSING_SIGNATURE"


def test_invalid_signature_dtype_raises_stable_model_error_code() -> None:
    with pytest.raises(WyrdError) as exc:
        ModelSignature(
            [FieldSpec("feature", "not_a_dtype")],
            [FieldSpec("prediction", "float64")],
        )

    assert exc.value.code == "WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED"


def test_zero_length_signature_dimension_is_refused() -> None:
    with pytest.raises(WyrdError) as exc:
        ModelSignature(
            [FieldSpec("feature", "float64", shape=[Dim.fixed(0)])],
            [FieldSpec("prediction", "float64")],
        )

    assert exc.value.code == "WYRD_MODEL_400_SHAPE_INVALID"


def test_unsupported_raw_model_object_raises_stable_model_error_code(
    regressor_metadata: ModelCardMetadata,
) -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard(object(), metadata=regressor_metadata)

    assert exc.value.code == "WYRD_MODEL_400_UNKNOWN_MODEL_TYPE"


def test_interface_class_passed_to_constructor_raises_model_error(
    regressor_metadata: ModelCardMetadata,
) -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard(SklearnInterface, metadata=regressor_metadata)

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_artifact_path_is_refused_without_leaking_it(
    tmp_path: Path, regressor_metadata: ModelCardMetadata
) -> None:
    artifact = tmp_path / "model.joblib"
    artifact.write_bytes(b"placeholder")

    with pytest.raises(WyrdError) as exc:
        ModelCard(artifact, metadata=regressor_metadata)

    assert exc.value.code == "WYRD_MODEL_400_UNKNOWN_MODEL_TYPE"
    assert str(tmp_path) not in str(exc.value)


def test_invalid_huggingface_revision_raises_stable_model_error_code(
    huggingface_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    interface = HuggingfaceInterface(
        model=huggingface_model,
        hf_task="text-classification",
        revision="bad-rev",
    )

    with pytest.raises(WyrdError) as exc:
        ModelCard(interface, metadata=classifier_metadata)

    assert exc.value.code == "WYRD_MODEL_400_HF_REVISION_INVALID"


def test_invalid_huggingface_repo_id_raises_model_validation_error(
    huggingface_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    interface = HuggingfaceInterface(
        model=huggingface_model,
        hf_task="text-classification",
        repo_id="",
    )

    with pytest.raises(WyrdError) as exc:
        ModelCard(interface, metadata=classifier_metadata)

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_invalid_huggingface_task_raises_stable_option_error(huggingface_model: Any) -> None:
    with pytest.raises(WyrdError) as exc:
        HuggingfaceInterface(model=huggingface_model, hf_task="not-a-task")

    assert exc.value.code == "WYRD_DATA_400_INVALID_INTERFACE_OPTION"


def test_custom_model_card_without_a_loader_is_refused(fixtures_dir: Path) -> None:
    with pytest.raises(WyrdError) as exc:
        ModelCard.from_path(fixtures_dir / "invalid" / "model" / "custom-without-loader.yaml")

    assert exc.value.code == "WYRD_MODEL_400_CUSTOM_LOADER_INVALID"
