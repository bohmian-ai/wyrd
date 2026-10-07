"""Unit contracts for the Python Card registry boundary."""

import pytest
import wyrd
from wyrd.cards import AgentCard, Cards, VersionBump
from wyrd.prompt import Prompt, PromptCard, PromptReference


@pytest.fixture
def offline_cards() -> Cards:
    """A credential-complete client whose endpoint must never be used."""
    return Cards(server_url="http://127.0.0.1:1", credential="unit-test-key")


def test_agent_cards_are_not_registerable(offline_cards: Cards) -> None:
    """The unscoped registry rejects envelope-only Agent holders locally."""
    card = AgentCard(
        PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="hello")),
        space="unit",
        name="agent",
        version="0.1.0",
    )
    with pytest.raises(wyrd.WyrdError) as captured:
        offline_cards.register(card)
    assert captured.value.code == "WYRD_DATA_400_VALIDATION"


def test_exact_pin_and_explicit_bump_are_rejected_before_network(offline_cards: Cards) -> None:
    """An exact authored pin cannot be combined with a caller bump intent."""
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="hello"),
        space="unit",
        name="prompt",
        version="1.2.3",
    )
    with pytest.raises(wyrd.WyrdError) as captured:
        offline_cards.prompt.register(card, version_bump=VersionBump.Minor)
    assert captured.value.code == "WYRD_DATA_400_VALIDATION"


@pytest.mark.usefixtures("no_credentials")
def test_cards_without_a_credential_raise_the_client_catalog_error() -> None:
    """Cards reports an empty credential chain with Bifrost's client code."""
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


@pytest.mark.parametrize("kind", ["data", "model", "prompt"])
@pytest.mark.parametrize(
    ("method", "kwargs", "field"),
    [
        ("get", {"uid": "not-a-uid"}, "uid"),
        ("get", {"uid": "not-a-uid", "space": "unit", "name": "card"}, "uid"),
        ("get", {"space": "Bad Space", "name": "card"}, "space"),
        ("get", {"space": "unit", "name": "Bad Name"}, "name"),
        ("get", {"space": "unit", "name": "card", "version": "not-a-version"}, "version"),
        ("get", {}, "space"),
        ("get", {"space": "unit"}, "name"),
        ("list", {"space": "Bad Space"}, "space"),
        ("list", {"name": "Bad Name"}, "name"),
        ("resolve_latest", {"space": "unit", "name": "Bad Name"}, "name"),
        ("delete", {"uid": "not-a-uid"}, "uid"),
        ("delete", {"space": "unit"}, "name"),
    ],
)
def test_registry_selector_errors_use_request_validation(
    offline_cards: Cards, kind: str, method: str, kwargs: dict[str, str], field: str
) -> None:
    """Malformed registry selectors raise request validation before any IO."""
    registry = getattr(offline_cards, kind)
    with pytest.raises(wyrd.WyrdError) as captured:
        getattr(registry, method)(**kwargs)
    assert captured.value.code == "WYRD_SPEC_400_VALIDATION"
    assert captured.value.status == 400
    assert captured.value.title == "Validation failed"
    assert captured.value.remediation == (
        "Check the submitted Wyrd request fields against the published schema and retry."
    )
    assert captured.value.details == {"field": field}
