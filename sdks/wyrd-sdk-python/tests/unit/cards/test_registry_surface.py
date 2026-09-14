"""Unit contracts for the Python Card registry boundary."""

from pathlib import Path

import pytest
import wyrd
from wyrd.cards import AgentCard, Cards, VersionBump
from wyrd.prompt import Prompt, PromptCard, PromptReference


def _offline_cards() -> Cards:
    """Construct a credential-complete client whose endpoint must never be used."""
    return Cards(server_url="http://127.0.0.1:1", credential="unit-test-key")


def test_agent_cards_are_not_registerable() -> None:
    """The unscoped registry rejects envelope-only Agent holders locally."""
    card = AgentCard(
        PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="hello")),
        space="unit",
        name="agent",
        version="0.1.0",
    )
    with pytest.raises(wyrd.WyrdError, match="DataCard, ModelCard, or PromptCard"):
        _offline_cards().register(card)


def test_registry_stub_documents_patch_default_and_exact_delete() -> None:
    """Hand-authored registry contracts describe the native version policy."""
    stub = Path(__file__).parents[3] / "python" / "wyrd" / "stubs" / "cards.pyi"
    text = stub.read_text(encoding="utf-8")
    assert "RegisterableCard: TypeAlias = DataCard | ModelCard | PromptCard" in text
    assert "VersionBump.Patch" in text
    assert "name`, and the exact `version`" in text


def test_exact_pin_and_explicit_bump_are_rejected_before_network() -> None:
    """An exact authored pin cannot be combined with a caller bump intent."""
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="hello"),
        space="unit",
        name="prompt",
        version="1.2.3",
    )
    with pytest.raises(wyrd.WyrdError, match="exact metadata.version pin"):
        _offline_cards().prompt.register(card, version_bump=VersionBump.Minor)


def test_data_get_rejects_legacy_load_args_before_network() -> None:
    """The Data registry rejects the retired eager-load keyword locally."""
    with pytest.raises(
        TypeError,
        match="get\\(\\) got an unexpected keyword argument 'load_args'",
    ):
        _offline_cards().data.get(load_args={})


def test_model_get_rejects_legacy_load_args_before_network() -> None:
    """The Model registry rejects the retired eager-load keyword locally."""
    with pytest.raises(
        TypeError,
        match="get\\(\\) got an unexpected keyword argument 'load_args'",
    ):
        _offline_cards().model.get(load_args={})


def test_cards_without_a_credential_raise_the_client_catalog_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Cards reports an empty credential chain with Bifrost's client code."""
    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", "/nonexistent-wyrd-home")

    with pytest.raises(wyrd.WyrdError) as captured:
        Cards(server_url="http://127.0.0.1:1")
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"
    assert captured.value.status == 401
    assert captured.value.details == {}


def test_cards_with_an_empty_server_url_raise_config_invalid_details() -> None:
    """Cards keeps the offending field and reason in the client config error."""
    with pytest.raises(wyrd.WyrdError) as captured:
        Cards(server_url="", credential="wyrd_sk_t_v_s")
    assert captured.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
    assert captured.value.status == 400
    assert captured.value.details == {"field": "server_url", "reason": "must not be empty"}
