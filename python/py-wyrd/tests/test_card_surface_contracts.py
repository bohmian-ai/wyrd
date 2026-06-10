from __future__ import annotations

from collections.abc import Callable

import pytest
from wyrd.data import DataCard, FieldSpec, SqlInterface
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature, SklearnInterface
from wyrd.prompt import Prompt, PromptCard


def _data_card() -> DataCard:
    return DataCard(SqlInterface(data={"queries": {"q": "select 1"}}, dialect="duckdb"))


def _model_card() -> ModelCard:
    return ModelCard(
        SklearnInterface(),
        metadata=ModelCardMetadata(
            task_type="binary_classification",
            signature=ModelSignature(
                [FieldSpec("feature", "float64")],
                [FieldSpec("prediction", "float64")],
            ),
        ),
    )


def _prompt_card() -> PromptCard:
    return PromptCard(Prompt.openai_chat("gpt-4o", messages="hi"))


@pytest.mark.parametrize(
    ("surface_factory", "forbidden_attributes"),
    [
        pytest.param(_data_card, ("register", "save_card"), id="data-card"),
        pytest.param(_model_card, ("register", "save_card"), id="model-card"),
        pytest.param(_prompt_card, ("register",), id="prompt-card"),
        pytest.param(lambda: Prompt, ("register",), id="prompt-builder"),
    ],
)
def test_card_surfaces_do_not_expose_server_registration(
    surface_factory: Callable[[], object],
    forbidden_attributes: tuple[str, ...],
) -> None:
    surface = surface_factory()

    for attribute in forbidden_attributes:
        assert not hasattr(surface, attribute)
