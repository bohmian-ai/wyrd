from __future__ import annotations

from _helpers import model_metadata
from test_modelcard_save_load import _sklearn_model
from wyrd.model import ModelCard, SklearnInterface


def test_modelcard_has_no_register_instance_method() -> None:
    card = ModelCard(
        SklearnInterface(model=_sklearn_model()),
        metadata=model_metadata("binary_classification"),
    )

    assert not hasattr(card, "register")


def test_modelcard_has_no_envelope_only_save_method() -> None:
    card = ModelCard(
        SklearnInterface(model=_sklearn_model()),
        metadata=model_metadata("binary_classification"),
    )

    assert not hasattr(card, "save_card")
