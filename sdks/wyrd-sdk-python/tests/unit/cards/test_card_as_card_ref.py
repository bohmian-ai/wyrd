"""Each Card holder names itself as a CardRef of its own kind."""

from typing import Any

import pandas as pd
import pytest
from wyrd.cards import CardKind
from wyrd.data import DataCard, FieldSpec, PandasInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface
from wyrd.prompt import Prompt, PromptCard

SIGNATURE = ModelSignature([FieldSpec("feature", "float64")], [FieldSpec("prediction", "float64")])


@pytest.mark.parametrize(
    ("card", "kind"),
    [
        pytest.param(
            PromptCard(
                Prompt.openai_chat("gpt-4o", messages="Hello"),
                name="support-prompt",
                version="1.0.0",
            ),
            CardKind.Prompt,
            id="prompt",
        ),
        pytest.param(
            ModelCard(
                SklearnInterface(),
                name="churn-model",
                version="1.0.0",
                metadata=ModelCardMetadata(task_type="binary_classification", signature=SIGNATURE),
            ),
            CardKind.Model,
            id="model",
        ),
        pytest.param(
            DataCard(
                PandasInterface(data=pd.DataFrame({"feature": [1]})),
                name="training-data",
                version="1.0.0",
            ),
            CardKind.Data,
            id="data",
        ),
    ],
)
def test_card_names_itself_as_a_card_ref(card: Any, kind: CardKind) -> None:
    ref = card.as_card_ref()

    assert (ref.kind, ref.name, ref.version) == (kind, card.name, card.version)
