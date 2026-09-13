from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest
import torch
from _helpers import assert_model_card_json, model_metadata
from torch.utils.data import DataLoader, TensorDataset
from wyrd.model import (
    CatboostInterface,
    HuggingfaceInterface,
    LightgbmInterface,
    LightningInterface,
    ModelCard,
    ModelInterface,
    SampleInput,
    SklearnInterface,
    TorchInterface,
    WyrdError,
    XgboostInterface,
)


def _matrix() -> tuple[np.ndarray, np.ndarray]:
    return (
        np.array([[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]], dtype=np.float32),
        np.array([0, 0, 1, 1], dtype=np.int64),
    )


def _sklearn_model():
    from sklearn.linear_model import LogisticRegression

    features, labels = _matrix()
    return LogisticRegression().fit(features, labels)


def _xgboost_model():
    from xgboost import XGBClassifier

    return XGBClassifier(
        n_estimators=2,
        max_depth=1,
        eval_metric="logloss",
        verbosity=0,
    )


def _lightgbm_model():
    from lightgbm import LGBMClassifier

    return LGBMClassifier(
        n_estimators=2,
        min_data_in_leaf=1,
        min_data_in_bin=1,
        verbose=-1,
    )


def _catboost_model():
    from catboost import CatBoostClassifier

    return CatBoostClassifier(
        iterations=2,
        depth=1,
        verbose=False,
        allow_writing_files=False,
    )


def _torch_model() -> torch.nn.Module:
    model = torch.nn.Linear(2, 1)
    with torch.no_grad():
        model.weight.fill_(0.25)
        model.bias.fill_(0.1)
    return model


def _huggingface_model():
    from transformers import BertConfig, BertForSequenceClassification

    config = BertConfig(
        vocab_size=16,
        hidden_size=8,
        num_hidden_layers=1,
        num_attention_heads=1,
        intermediate_size=16,
        num_labels=2,
    )
    return BertForSequenceClassification(config)


def _round_trip(card: ModelCard, path: Path, expected_kind: str, *, interface=None) -> ModelCard:
    card.save(path)
    restored = ModelCard.model_validate_json((path / "card.json").read_text(), interface=interface)
    restored.load(path)

    assert restored.interface.kind == expected_kind
    assert restored.interface.has_model is True
    assert restored.model is not None
    assert_model_card_json(path, expected_kind)
    return restored


@pytest.mark.wyrd_covers("python:ModelCard.save")
@pytest.mark.wyrd_covers("python:ModelCard.load")
def test_raw_sklearn_model_autodetects_and_round_trips(tmp_path: Path) -> None:
    card = ModelCard(
        _sklearn_model(),
        name="sklearn-churn",
        labels={"domain": "churn"},
        annotations={"acme.com/source": "unit-test"},
        metadata=model_metadata("binary_classification"),
    )

    restored = _round_trip(card, tmp_path / "sklearn", "Sklearn")

    assert restored.name == "sklearn-churn"
    assert restored.labels == {"domain": "churn"}
    assert restored.annotations == {"acme.com/source": "unit-test"}


@pytest.mark.parametrize(
    ("model_factory", "expected_kind"),
    [
        pytest.param(_torch_model, "Torch", id="torch"),
        pytest.param(_xgboost_model, "Xgboost", id="xgboost"),
        pytest.param(_lightgbm_model, "Lightgbm", id="lightgbm"),
        pytest.param(_catboost_model, "Catboost", id="catboost"),
    ],
)
def test_raw_model_autodetection_for_framework_models(
    tmp_path: Path,
    model_factory,
    expected_kind: str,
) -> None:
    card = ModelCard(model_factory(), metadata=model_metadata("binary_classification"))

    restored = _round_trip(card, tmp_path / expected_kind.lower(), expected_kind)

    assert restored.interface.kind == expected_kind


def test_sklearn_interface_loads_a_local_materialization(tmp_path: Path) -> None:
    source = tmp_path / "source-sklearn"
    ModelCard(
        _sklearn_model(),
        metadata=model_metadata("binary_classification"),
    ).save(source)

    card = ModelCard(
        SklearnInterface(),
        metadata=model_metadata(
            "binary_classification",
        ),
    )
    card.load(source)

    assert card.interface.has_model is True
    assert card.model is not None


def test_modelcard_projects_model_and_preprocessor_directly() -> None:
    from sklearn.preprocessing import StandardScaler

    model = _sklearn_model()
    preprocessor = StandardScaler().fit(_matrix()[0])
    card = ModelCard(
        SklearnInterface(model=model, preprocessor=preprocessor),
        metadata=model_metadata("binary_classification"),
    )

    assert card.model is model
    assert card.preprocessor is preprocessor
    assert card.processor is None


def test_xgboost_interface_round_trips(tmp_path: Path) -> None:
    card = ModelCard(
        XgboostInterface(model=_xgboost_model()),
        metadata=model_metadata("binary_classification"),
    )

    _round_trip(card, tmp_path / "xgboost", "Xgboost")


def test_lightgbm_interface_round_trips(tmp_path: Path) -> None:
    card = ModelCard(
        LightgbmInterface(model=_lightgbm_model()),
        metadata=model_metadata("binary_classification"),
    )

    _round_trip(card, tmp_path / "lightgbm", "Lightgbm")


def test_catboost_interface_round_trips(tmp_path: Path) -> None:
    card = ModelCard(
        CatboostInterface(model=_catboost_model()),
        metadata=model_metadata("binary_classification"),
    )

    _round_trip(card, tmp_path / "catboost", "Catboost")


def test_torch_interface_round_trips_with_safetensors(tmp_path: Path) -> None:
    card = ModelCard(
        TorchInterface(model=_torch_model(), save_format="safetensors"),
        metadata=model_metadata("regression"),
    )
    interface = TorchInterface(model=_torch_model(), save_format="safetensors")

    restored = _round_trip(card, tmp_path / "torch", "Torch", interface=interface)

    assert restored.interface.save_format == "safetensors"
    assert (tmp_path / "torch" / "model.safetensors").exists()


def test_torch_interface_round_trips_with_pickle(tmp_path: Path) -> None:
    card = ModelCard(
        TorchInterface(model=_torch_model(), save_format="pickle"),
        metadata=model_metadata("regression"),
    )

    restored = _round_trip(card, tmp_path / "torch-pickle", "Torch")

    assert restored.interface.save_format == "pickle"
    assert (tmp_path / "torch-pickle" / "model.pt").exists()


def test_lightning_interface_round_trips_with_checkpoint(tmp_path: Path) -> None:
    import pytorch_lightning as pl

    class TinyLightning(pl.LightningModule):
        def __init__(self) -> None:
            super().__init__()
            self.layer = torch.nn.Linear(2, 1)

        def forward(self, value):
            return self.layer(value)

        def training_step(self, batch, batch_idx):
            features, targets = batch
            return torch.nn.functional.mse_loss(self(features), targets)

        def configure_optimizers(self):
            return torch.optim.SGD(self.parameters(), lr=0.01)

    features = torch.tensor([[0.0, 0.0], [1.0, 1.0]], dtype=torch.float32)
    targets = torch.tensor([[0.0], [1.0]], dtype=torch.float32)
    loader = DataLoader(TensorDataset(features, targets), batch_size=1)
    model = TinyLightning()
    trainer = pl.Trainer(
        max_epochs=1,
        logger=False,
        enable_checkpointing=False,
        enable_model_summary=False,
        enable_progress_bar=False,
        accelerator="cpu",
        devices=1,
    )
    trainer.fit(model, loader)
    card = ModelCard(
        LightningInterface(model=model, trainer=trainer),
        metadata=model_metadata("regression"),
    )
    interface = LightningInterface(model=TinyLightning)

    _round_trip(card, tmp_path / "lightning", "Lightning", interface=interface)


def test_lightning_interface_round_trips_trainerless_checkpoint(tmp_path: Path) -> None:
    import pytorch_lightning as pl

    class TinyLightning(pl.LightningModule):
        def __init__(self) -> None:
            super().__init__()
            self.layer = torch.nn.Linear(2, 1)

        def forward(self, value):
            return self.layer(value)

    card = ModelCard(
        LightningInterface(model=TinyLightning()),
        metadata=model_metadata("regression"),
    )
    interface = LightningInterface(model=TinyLightning())

    _round_trip(card, tmp_path / "lightning-trainerless", "Lightning", interface=interface)


def test_huggingface_interface_round_trips_from_local_pretrained_dir(tmp_path: Path) -> None:
    card = ModelCard(
        HuggingfaceInterface(model=_huggingface_model(), hf_task="text-classification"),
        metadata=model_metadata("binary_classification"),
    )

    restored = _round_trip(card, tmp_path / "huggingface", "Huggingface")

    assert restored.interface.hf_task == "text-classification"
    assert (tmp_path / "huggingface" / "model" / "config.json").exists()


def test_custom_python_model_interface_round_trips_with_explicit_loader(tmp_path: Path) -> None:
    class CustomMemoryInterface(ModelInterface):
        def __init__(self, value: str = "") -> None:
            super().__init__()
            self.value = value

        def save(self, path, save_kwargs=None):
            path.mkdir(parents=True, exist_ok=True)
            (path / "custom.txt").write_text(self.value, encoding="utf-8")

        def load(self, path, load_kwargs=None):
            self.value = (path / "custom.txt").read_text(encoding="utf-8")

        @property
        def model(self):
            return self.value

    card = ModelCard(CustomMemoryInterface("saved"), metadata=model_metadata("other"))
    path = tmp_path / "custom"
    card.save(path)

    restored = ModelCard.model_validate_json(
        (path / "card.json").read_text(),
        interface=CustomMemoryInterface(),
    )
    restored.load(path)

    assert restored.interface.kind == "Custom"
    assert restored.interface.value == "saved"
    assert restored.model == "saved"
    assert_model_card_json(path, "Custom")


def test_joblib_none_model_artifact_raises_model_validation_error(tmp_path: Path) -> None:
    import joblib

    path = tmp_path / "joblib-none"
    path.mkdir()
    joblib.dump(None, path / "model.joblib")
    interface = SklearnInterface()

    with pytest.raises(WyrdError) as exc:
        interface.load(path)

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"


def test_modelcard_save_without_live_model_raises_model_error(tmp_path: Path) -> None:
    card = ModelCard(
        SklearnInterface(),
        metadata=model_metadata("binary_classification"),
    )

    with pytest.raises(WyrdError) as exc:
        card.save(tmp_path / "no-live-model")

    assert exc.value.code == "WYRD_MODEL_400_VALIDATION"
    assert "DataCard" not in str(exc.value)


def test_modelcard_metadata_to_dict_accepts_interface_instance() -> None:
    metadata = model_metadata(
        "binary_classification",
        interface=SklearnInterface(),
    )

    payload = metadata.to_dict()

    assert payload["interface"]["kind"] == "Sklearn"
    assert payload["task_type"] == "BinaryClassification"


def test_sample_input_round_trips_through_modelcard(tmp_path: Path) -> None:
    sample = SampleInput.from_python_object({"feature": [1.0, 2.0]})
    card = ModelCard(
        SklearnInterface(model=_sklearn_model()),
        metadata=model_metadata("binary_classification", sample_input=sample),
    )
    path = tmp_path / "sample-input"

    card.save(path)
    restored = ModelCard.model_validate_json((path / "card.json").read_text())
    restored_sample = restored.sample_input

    assert restored_sample is not None
    assert restored_sample.kind_token == "dict"
    assert restored_sample.to_dict()["kind"] == "Dict"
