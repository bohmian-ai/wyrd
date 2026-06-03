from wyrd import Prompt
from wyrd.providers import mock_registry


def test_mock_registry_uses_mock_provider_identity() -> None:
    assert mock_registry().names == ["mock"]
    assert Prompt(["hello"], "mock-model", provider="mock").provider == "mock"
