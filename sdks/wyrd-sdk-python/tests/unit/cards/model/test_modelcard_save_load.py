"""A ModelCard saves its model and loads it back through its interface."""

from pathlib import Path
from typing import Any

import joblib
import pytest
import torch
from torch.utils.data import DataLoader, TensorDataset
from wyrd.model import (
    CatboostInterface,
    HuggingfaceInterface,
    LightgbmInterface,
    LightningInterface,
    ModelCard,
    ModelCardMetadata,
    ModelInterface,
    ModelSignature,
    SampleInput,
    SklearnInterface,
    TorchInterface,
    WyrdError,
    XgboostInterface,
)


class CustomMemoryInterface(ModelInterface):
    """A user interface that stores its model as one text file."""

    def __init__(self, value: str = "") -> None:
        super().__init__()
        self.value = value

    def save(self, path: Path, save_kwargs: Any = None) -> None:
        path.mkdir(parents=True, exist_ok=True)
        (path / "custom.txt").write_text(self.value, encoding="utf-8")

    def load(self, path: Path, load_kwargs: Any = None) -> None:
        self.value = (path / "custom.txt").read_text(encoding="utf-8")

    @property
    def model(self) -> str:
        return self.value


def test_raw_sklearn_model_round_trips_with_its_identity(
    tmp_path: Path, sklearn_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    ModelCard(
        sklearn_model,
        name="sklearn-churn",
        labels={"domain": "churn"},
        annotations={"acme.com/source": "unit-test"},
        metadata=classifier_metadata,
    ).save(tmp_path)

    restored = ModelCard.from_path(tmp_path)

    assert (restored.name, restored.labels, restored.annotations) == (
        "sklearn-churn",
        {"domain": "churn"},
        {"acme.com/source": "unit-test"},
    )


@pytest.mark.parametrize(
    ("model_fixture", "interface", "kind"),
    [
        pytest.param("sklearn_model", None, "Sklearn", id="sklearn-detected"),
        pytest.param("torch_model", None, "Torch", id="torch-detected"),
        pytest.param("xgboost_model", None, "Xgboost", id="xgboost-detected"),
        pytest.param("lightgbm_model", None, "Lightgbm", id="lightgbm-detected"),
        pytest.param("catboost_model", None, "Catboost", id="catboost-detected"),
        pytest.param("xgboost_model", XgboostInterface, "Xgboost", id="xgboost-explicit"),
        pytest.param("lightgbm_model", LightgbmInterface, "Lightgbm", id="lightgbm-explicit"),
        pytest.param("catboost_model", CatboostInterface, "Catboost", id="catboost-explicit"),
    ],
)
def test_model_round_trips_through_its_interface(
    request: pytest.FixtureRequest,
    tmp_path: Path,
    classifier_metadata: ModelCardMetadata,
    model_fixture: str,
    interface: type[ModelInterface] | None,
    kind: str,
) -> None:
    model = request.getfixturevalue(model_fixture)
    ModelCard(
        model if interface is None else interface(model=model), metadata=classifier_metadata
    ).save(tmp_path)

    restored = ModelCard.from_path(tmp_path)

    assert (restored.interface.kind, restored.interface.has_model) == (kind, True)


def test_sklearn_interface_loads_a_local_materialization(
    tmp_path: Path, sklearn_model: Any, signature: ModelSignature
) -> None:
    ModelCard(
        sklearn_model,
        metadata=ModelCardMetadata(task_type="binary_classification", signature=signature),
    ).save(tmp_path)
    card = ModelCard(
        SklearnInterface(),
        metadata=ModelCardMetadata(task_type="binary_classification", signature=signature),
    )

    card.load(tmp_path)

    assert card.interface.has_model is True


def test_modelcard_projects_model_and_preprocessor_directly(
    sklearn_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    from sklearn.preprocessing import StandardScaler

    preprocessor = StandardScaler()
    card = ModelCard(
        SklearnInterface(model=sklearn_model, preprocessor=preprocessor),
        metadata=classifier_metadata,
    )

    assert (card.model, card.preprocessor, card.processor) == (sklearn_model, preprocessor, None)


@pytest.mark.parametrize("save_format", ["safetensors", "pickle"])
def test_torch_model_round_trips_in_its_save_format(
    tmp_path: Path,
    torch_model: torch.nn.Module,
    regressor_metadata: ModelCardMetadata,
    save_format: str,
) -> None:
    ModelCard(
        TorchInterface(model=torch_model, save_format=save_format), metadata=regressor_metadata
    ).save(tmp_path)

    restored = ModelCard.from_path(
        tmp_path, interface=TorchInterface(model=torch.nn.Linear(2, 1), save_format=save_format)
    )

    assert (restored.interface.has_model, restored.interface.save_format) == (True, save_format)


def test_trained_lightning_model_round_trips_from_its_checkpoint(
    tmp_path: Path, lightning_module_class: type, regressor_metadata: ModelCardMetadata
) -> None:
    import pytorch_lightning as pl

    model = lightning_module_class()
    trainer = pl.Trainer(
        max_epochs=1,
        logger=False,
        enable_checkpointing=False,
        enable_model_summary=False,
        enable_progress_bar=False,
        accelerator="cpu",
        devices=1,
    )
    rows = TensorDataset(torch.tensor([[0.0, 0.0], [1.0, 1.0]]), torch.tensor([[0.0], [1.0]]))
    trainer.fit(model, DataLoader(rows, batch_size=1))
    ModelCard(LightningInterface(model=model, trainer=trainer), metadata=regressor_metadata).save(
        tmp_path
    )

    restored = ModelCard.from_path(
        tmp_path, interface=LightningInterface(model=lightning_module_class)
    )

    assert (restored.interface.kind, restored.interface.has_model) == ("Lightning", True)


def test_untrained_lightning_model_round_trips_without_a_trainer(
    tmp_path: Path, lightning_module_class: type, regressor_metadata: ModelCardMetadata
) -> None:
    ModelCard(LightningInterface(model=lightning_module_class()), metadata=regressor_metadata).save(
        tmp_path
    )

    restored = ModelCard.from_path(
        tmp_path, interface=LightningInterface(model=lightning_module_class())
    )

    assert (restored.interface.kind, restored.interface.has_model) == ("Lightning", True)


def test_huggingface_model_round_trips_with_its_task(
    tmp_path: Path, huggingface_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    ModelCard(
        HuggingfaceInterface(model=huggingface_model, hf_task="text-classification"),
        metadata=classifier_metadata,
    ).save(tmp_path)

    restored = ModelCard.from_path(tmp_path)

    assert (restored.interface.has_model, restored.interface.hf_task) == (
        True,
        "text-classification",
    )


def test_custom_model_round_trips_through_its_interface(
    tmp_path: Path, signature: ModelSignature
) -> None:
    ModelCard(
        CustomMemoryInterface("saved"),
        metadata=ModelCardMetadata(task_type="other", signature=signature),
    ).save(tmp_path)

    restored = ModelCard.from_path(tmp_path, interface=CustomMemoryInterface())

    assert (restored.interface.kind, restored.model) == ("Custom", "saved")


def test_joblib_none_model_artifact_raises_model_validation_error(tmp_path: Path) -> None:
    joblib.dump(None, tmp_path / "model.joblib")

    with pytest.raises(WyrdError) as exc:
        SklearnInterface().load(tmp_path)

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_modelcard_save_without_live_model_raises_model_error(
    tmp_path: Path, classifier_metadata: ModelCardMetadata
) -> None:
    card = ModelCard(SklearnInterface(), metadata=classifier_metadata)

    with pytest.raises(WyrdError) as exc:
        card.save(tmp_path)

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_modelcard_metadata_reads_back_its_interface_and_task_type(
    signature: ModelSignature,
) -> None:
    metadata = ModelCardMetadata(
        interface=SklearnInterface(), task_type="binary_classification", signature=signature
    )

    assert (metadata.interface_kind, metadata.task_type) == ("Sklearn", "binary_classification")


def test_sample_input_survives_save_and_load(
    tmp_path: Path, sklearn_model: Any, signature: ModelSignature
) -> None:
    ModelCard(
        SklearnInterface(model=sklearn_model),
        metadata=ModelCardMetadata(
            task_type="binary_classification",
            signature=signature,
            sample_input=SampleInput.from_python_object({"feature": [1.0, 2.0]}),
        ),
    ).save(tmp_path)

    restored = ModelCard.from_path(tmp_path)

    assert restored.sample_input is not None
    assert restored.sample_input.kind_token == "dict"


def test_model_from_path_reads_card_file_without_loading_model(
    tmp_path: Path, sklearn_model: Any, classifier_metadata: ModelCardMetadata
) -> None:
    saved = ModelCard(sklearn_model, name="churn", metadata=classifier_metadata)
    saved.save(tmp_path)

    card = ModelCard.from_path(tmp_path / "card.json")

    assert (card.uid, card.interface.kind, card.interface.has_model) == (
        saved.uid,
        "Sklearn",
        False,
    )


def test_model_from_path_missing_file_raises_loader_error(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as error:
        ModelCard.from_path(tmp_path / "absent.yaml")

    assert error.value.code == "WYRD_LOADER_400_IO"
