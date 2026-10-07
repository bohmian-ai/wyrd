"""Small trained models and Model Card metadata shared by the ModelCard unit tests."""

from typing import Any

import numpy as np
import pytest
import torch
from wyrd.data import FieldSpec
from wyrd.model import ModelCardMetadata, ModelSignature

FEATURES = np.array([[0.0, 0.0], [0.0, 1.0], [1.0, 0.0], [1.0, 1.0]], dtype=np.float32)
LABELS = np.array([0, 0, 1, 1], dtype=np.int64)


@pytest.fixture
def signature() -> ModelSignature:
    """One float feature in, one float prediction out."""
    return ModelSignature([FieldSpec("feature", "float64")], [FieldSpec("prediction", "float64")])


@pytest.fixture
def classifier_metadata(signature: ModelSignature) -> ModelCardMetadata:
    """Metadata for a binary classifier."""
    return ModelCardMetadata(task_type="binary_classification", signature=signature)


@pytest.fixture
def regressor_metadata(signature: ModelSignature) -> ModelCardMetadata:
    """Metadata for a regressor."""
    return ModelCardMetadata(task_type="regression", signature=signature)


@pytest.fixture
def sklearn_model() -> Any:
    """A logistic regression fitted on four rows."""
    from sklearn.linear_model import LogisticRegression

    return LogisticRegression().fit(FEATURES, LABELS)


@pytest.fixture
def xgboost_model() -> Any:
    """An unfitted two-tree XGBoost classifier."""
    from xgboost import XGBClassifier

    return XGBClassifier(n_estimators=2, max_depth=1, eval_metric="logloss", verbosity=0)


@pytest.fixture
def lightgbm_model() -> Any:
    """An unfitted two-tree LightGBM classifier."""
    from lightgbm import LGBMClassifier

    return LGBMClassifier(n_estimators=2, min_data_in_leaf=1, min_data_in_bin=1, verbose=-1)


@pytest.fixture
def catboost_model() -> Any:
    """An unfitted two-iteration CatBoost classifier."""
    from catboost import CatBoostClassifier

    return CatBoostClassifier(iterations=2, depth=1, verbose=False, allow_writing_files=False)


@pytest.fixture
def torch_model() -> torch.nn.Module:
    """A linear layer with fixed weights."""
    model = torch.nn.Linear(2, 1)
    with torch.no_grad():
        model.weight.fill_(0.25)
        model.bias.fill_(0.1)
    return model


@pytest.fixture
def huggingface_model() -> Any:
    """A one-layer BERT sequence classifier."""
    from transformers import BertConfig, BertForSequenceClassification

    return BertForSequenceClassification(
        BertConfig(
            vocab_size=16,
            hidden_size=8,
            num_hidden_layers=1,
            num_attention_heads=1,
            intermediate_size=16,
            num_labels=2,
        )
    )


@pytest.fixture
def lightning_module_class() -> type:
    """A trainable one-layer Lightning module class."""
    import pytorch_lightning as pl

    class TinyLightning(pl.LightningModule):
        def __init__(self) -> None:
            super().__init__()
            self.layer = torch.nn.Linear(2, 1)

        def forward(self, value: torch.Tensor) -> torch.Tensor:
            return self.layer(value)

        def training_step(
            self, batch: tuple[torch.Tensor, torch.Tensor], batch_idx: int
        ) -> torch.Tensor:
            features, targets = batch
            return torch.nn.functional.mse_loss(self(features), targets)

        def configure_optimizers(self) -> torch.optim.Optimizer:
            return torch.optim.SGD(self.parameters(), lr=0.01)

    return TinyLightning
