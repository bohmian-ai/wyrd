import pytest


def test_agent_card_is_not_public_root_export() -> None:
    with pytest.raises(ImportError):
        from wyrd import AgentCard  # noqa: F401
