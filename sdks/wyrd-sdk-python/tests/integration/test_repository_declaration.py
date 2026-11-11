"""A tenant declares its connected repository's review policy from ``wyrd.toml`` or YAML.

Wyrd verifies the repository against the GitHub connection, registers only
the listed Verifiers, and selects the declared version only once the
declaration completes.
"""

import shutil
from collections.abc import Iterator
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import CardKind, CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.github import GithubConnection
from wyrd.testing import GithubAppMock, GithubBrowser, WyrdTestServer, cli

from .support import FIXTURES

pytestmark = pytest.mark.integration

STORY = FIXTURES / "cards/repository_declaration"


@pytest.fixture
def mock() -> GithubAppMock:
    """A GitHub where ``octo`` installs ``101`` with ``acme/api`` and ``acme/web``."""
    mock = GithubAppMock()
    mock.add_installation(101, [(1, "acme/api"), (2, "acme/web")])
    mock.add_user("octo", [101])
    mock.sign_in("octo")
    mock.install_target(101)
    return mock


@pytest.fixture
def server(mock: GithubAppMock) -> Iterator[WyrdTestServer]:
    """A deployment whose GitHub App is configured against ``mock``."""
    with WyrdTestServer(mutate_env=False, github=mock) as server:
        yield server


@pytest.fixture
def admin(server: WyrdTestServer) -> WyrdClient:
    """A tenant administrator who connected installation ``101``."""
    key = server.bootstrap_service(["admin"], name="repository-admin")
    admin = WyrdClient(server_url=server.base_url, credential=key, grpc_url=server.grpc_url)
    receipt = GithubConnection(admin).begin()
    browser = GithubBrowser.sign_in(server.base_url, server.tenant_slug, key)
    status, view, _ = browser.open(receipt["authorization_url"])
    assert status == 200, view
    return admin


def checkout(mock: GithubAppMock, full_name: str, destination: Path) -> Path:
    """A copy of the story fixtures whose origin is ``full_name`` on the mock's host."""
    shutil.copytree(STORY, destination)
    (destination / ".git").mkdir()
    (destination / ".git/config").write_text(
        f'[remote "origin"]\n\turl = http://{mock.host}/{full_name}.git\n'
    )
    return destination


def policy(root: Path, variant: str) -> Path:
    """Make ``policies/<variant>.toml`` the checkout's ``wyrd.toml`` and return its path."""
    toml = root / "wyrd.toml"
    shutil.copyfile(root / "policies" / f"{variant}.toml", toml)
    return toml


def repository(cards: Cards, ref: CardRef) -> tuple[dict, int]:
    """The served Repository selection and binding count of ``ref``."""
    status = cards.get(ref).status or {}
    return status["repository"], len((status.get("verification") or {}).get("binding_ids", []))


def test_toml_policy_is_selected_only_once_declared(
    admin: WyrdClient, mock: GithubAppMock, tmp_path: Path
) -> None:
    cards = Cards(admin)
    cards.declare_from_path(FIXTURES / "cards/verify_in_real_time/answer-check.yaml")
    root = checkout(mock, "acme/api", tmp_path / "api")

    first = cards.declare_from_path(policy(root, "minimal"))
    assert first.root.kind == CardKind.Repository
    assert first.root.name == "api"
    assert [outcome.card_ref.name for outcome in first.outcomes] == ["code-review", "api"]
    status, bindings = repository(cards, first.root)
    assert status["selected"] is True
    assert status["selected_version"] == first.root.version
    assert status["full_name"] == "acme/api"
    assert bindings == 1
    assert cards.declare_from_path(policy(root, "minimal")).root == first.root

    changed = cards.declare_from_path(policy(root, "changed"))
    assert changed.root.version != first.root.version
    assert repository(cards, changed.root) == (
        {"selected": True, "selected_version": changed.root.version, "full_name": "acme/api"},
        2,
    )
    assert repository(cards, first.root)[0]["selected"] is False

    for variant, code in (
        ("pinned", "WYRD_REGISTRY_409_SPEC_DRIFT"),
        ("missing", "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"),
    ):
        with pytest.raises(WyrdError) as refused:
            cards.declare_from_path(policy(root, variant))
        assert refused.value.code == code
    assert repository(cards, changed.root)[0]["selected"] is True

    assert cards.declare_from_path(policy(root, "rollback")).root == first.root
    assert repository(cards, first.root)[0]["selected"] is True

    empty = cards.declare_from_path(policy(root, "empty"))
    assert repository(cards, empty.root) == (
        {"selected": True, "selected_version": empty.root.version, "full_name": "acme/api"},
        0,
    )
    assert GithubConnection(admin).get()["state"] == "active"


def test_yaml_repository_declares_through_the_same_lifecycle(admin: WyrdClient) -> None:
    cards = Cards(admin)
    receipt = cards.declare_from_path(STORY / "repository.yaml")
    assert receipt.root.kind == CardKind.Repository
    assert receipt.root.name == "web"
    status, bindings = repository(cards, receipt.root)
    assert (status["selected"], status["full_name"], bindings) == (True, "acme/web", 1)


def test_repository_identity_is_verified_by_the_connection(
    admin: WyrdClient, mock: GithubAppMock, tmp_path: Path
) -> None:
    cards = Cards(admin)
    with pytest.raises(WyrdError) as unselected:
        cards.declare_from_path(policy(checkout(mock, "acme/vault", tmp_path / "vault"), "minimal"))
    assert unselected.value.code == "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE"

    root = checkout(mock, "acme/api", tmp_path / "api")
    cards.declare_from_path(policy(root, "minimal"))
    with pytest.raises(WyrdError) as named:
        cards.declare_from_path(policy(root, "named"))
    assert named.value.code == "WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT"


def test_cli_checks_offline_and_declares_the_policy(
    admin: WyrdClient, mock: GithubAppMock, tmp_path: Path
) -> None:
    root = checkout(mock, "acme/api", tmp_path / "api")

    report = cli.declare_check(policy(root, "minimal"))
    assert [card.name for card in report.cards] == ["code-review", "api"]
    assert report.deferred == [
        "repository_identity",
        "registered_references",
        "authorization",
        "version_assignment",
    ]
    bare = tmp_path / "bare"
    shutil.copytree(STORY, bare)
    with pytest.raises(WyrdError) as missing:
        cli.declare_check(policy(bare, "minimal"))
    assert missing.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"
    assert missing.value.details["diagnostics"][0]["code"] == (
        "WYRD_REGISTRY_400_REPOSITORY_IDENTITY_MISSING"
    )
    with pytest.raises(WyrdError) as broken:
        cli.declare(policy(root, "broken"))
    assert broken.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"

    receipt = cli.declare(policy(root, "minimal"), client=admin)
    assert receipt.root.kind == CardKind.Repository
    assert repository(Cards(admin), receipt.root)[0]["selected"] is True
