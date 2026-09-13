import importlib


def test_agent_card_is_public_root_export() -> None:
    module = importlib.import_module("wyrd")

    assert hasattr(module, "AgentCard")
