import importlib

import pytest


def test_agent_card_is_not_public_root_export() -> None:
    module = importlib.import_module("wyrd")
    forbidden_name = "Agent" + "Card"

    assert not hasattr(module, forbidden_name)
    with pytest.raises(AttributeError):
        getattr(module, forbidden_name)
