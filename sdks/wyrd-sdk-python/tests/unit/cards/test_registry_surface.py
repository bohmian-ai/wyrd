"""Unit contracts for the Python Card registry boundary."""

import pytest
import wyrd
from wyrd.cards import AgentCard, Cards, VersionBump
from wyrd.client import WyrdClient
from wyrd.prompt import Prompt, PromptCard, PromptReference


@pytest.fixture
def offline_cards() -> Cards:
    """A credential-complete client whose endpoint must never be used."""
    return Cards(WyrdClient(server_url="http://127.0.0.1:1", credential="unit-test-key"))


def test_agent_cards_are_not_declarable(offline_cards: Cards) -> None:
    """The unscoped registry rejects envelope-only Agent holders locally."""
    card = AgentCard(
        PromptReference.inline(Prompt.openai_chat("gpt-4o", messages="hello")),
        execution_route={"kind": "native"},
        name="agent",
        version="0.1.0",
    )
    with pytest.raises(wyrd.WyrdError) as captured:
        offline_cards.declare(card)  # ty: ignore[invalid-argument-type]
    assert captured.value.code == "WYRD_REGISTRY_400_INVALID_CARD_SPEC"


def test_exact_pin_and_explicit_bump_are_rejected_before_network(offline_cards: Cards) -> None:
    """An exact authored pin cannot be combined with a caller bump intent."""
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="hello"),
        name="prompt",
        version="1.2.3",
    )
    with pytest.raises(wyrd.WyrdError) as captured:
        offline_cards.prompt.declare(card, version_bump=VersionBump.Minor)
    assert captured.value.code == "WYRD_REGISTRY_400_INVALID_VERSION_BLOCK"


@pytest.mark.parametrize("kind", ["data", "model"])
@pytest.mark.parametrize("option", ["eager_load", "load_kwargs"])
def test_registry_get_rejects_artifact_download_options(
    offline_cards: Cards, kind: str, option: str
) -> None:
    """Typed get reads one envelope; artifact-loading options are refused before any IO."""
    with pytest.raises(TypeError, match=option):
        getattr(offline_cards, kind).get(uid="not-a-uid", **{option: True})


@pytest.mark.usefixtures("no_credentials")
def test_cards_without_a_credential_raise_the_client_catalog_error() -> None:
    """Cards reports an empty credential chain with Bifrost's client code."""
    with pytest.raises(wyrd.WyrdError) as captured:
        Cards.connect()
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"
    assert captured.value.status == 401
    assert captured.value.details == {}


def test_client_with_an_empty_server_url_raises_config_invalid_details() -> None:
    """The client keeps the offending field and reason in its config error."""
    with pytest.raises(wyrd.WyrdError) as captured:
        WyrdClient(server_url="", credential="wyrd_sk_t_v_s")
    assert captured.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
    assert captured.value.status == 400
    assert captured.value.details == {"field": "server_url", "reason": "must not be empty"}


@pytest.mark.parametrize("kind", ["data", "model", "prompt"])
@pytest.mark.parametrize(
    ("method", "kwargs", "field"),
    [
        ("get", {"uid": "not-a-uid"}, "uid"),
        ("get", {"uid": "not-a-uid", "name": "card"}, "uid"),
        ("get", {"name": "Bad Name"}, "name"),
        ("get", {"name": "card", "version": "not-a-version"}, "version"),
        ("get", {}, "name"),
        ("list", {"name": "Bad Name"}, "name"),
        ("resolve_latest", {"name": "Bad Name"}, "name"),
        ("delete", {"uid": "not-a-uid"}, "uid"),
        ("delete", {}, "name"),
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


@pytest.mark.parametrize("kind", ["data", "model", "prompt"])
@pytest.mark.parametrize(
    ("method", "kwargs"),
    [
        ("get", {"space": "unit", "name": "card"}),
        ("list", {"space": "unit"}),
        ("resolve_latest", {"space": "unit", "name": "card"}),
        ("delete", {"space": "unit", "name": "card", "version": "1.0.0"}),
    ],
)
def test_registry_selectors_refuse_a_card_space(
    offline_cards: Cards, kind: str, method: str, kwargs: dict[str, str]
) -> None:
    """Card identity is flat, so a ``space`` selector is an unknown argument."""
    registry = getattr(offline_cards, kind)
    with pytest.raises(TypeError, match="space"):
        getattr(registry, method)(**kwargs)


def test_unscoped_registry_refuses_a_card_space(offline_cards: Cards) -> None:
    """The unscoped list and resolve_latest take no ``space`` either."""
    with pytest.raises(TypeError, match="space"):
        offline_cards.list(space="unit")  # ty: ignore[unknown-argument]
    with pytest.raises(TypeError, match="space"):
        offline_cards.resolve_latest("Prompt", space="unit", name="card")  # ty: ignore[unknown-argument]
