"""Boundary tests for as_card_ref() on card holders."""

import numpy as np
import pandas as pd

from wyrd.cards import CardKind, CardRef
from wyrd.data import DataCard, FieldSpec, PandasInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface
from wyrd.prompt import Prompt, PromptCard


def test_prompt_card_as_card_ref() -> None:
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello"),
        name="support-prompt",
        version="1.0.0",
    )

    ref = card.as_card_ref()

    assert isinstance(ref, CardRef)
    assert ref.kind == CardKind.Prompt
    assert ref.name == card.name
    assert ref.version == card.version


def test_model_card_as_card_ref() -> None:
    from sklearn.linear_model import LogisticRegression

    features = np.array([[0.0], [1.0]], dtype=np.float32)
    labels = np.array([0, 1], dtype=np.int64)
    model = LogisticRegression().fit(features, labels)
    signature = ModelSignature(
        [FieldSpec("feature", "float64")],
        [FieldSpec("prediction", "float64")],
    )
    card = ModelCard(
        SklearnInterface(model=model),
        name="churn-model",
        version="1.0.0",
        metadata=ModelCardMetadata(task_type="binary_classification", signature=signature),
    )

    ref = card.as_card_ref()

    assert isinstance(ref, CardRef)
    assert ref.kind == CardKind.Model
    assert ref.name == card.name
    assert ref.version == card.version


def test_data_card_as_card_ref() -> None:
    card = DataCard(
        PandasInterface(data=pd.DataFrame({"feature": [1]})),
        name="training-data",
        version="1.0.0",
    )

    ref = card.as_card_ref()

    assert isinstance(ref, CardRef)
    assert ref.kind == CardKind.Data
    assert ref.name == card.name
    assert ref.version == card.version
