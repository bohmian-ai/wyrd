"""A tenant administrator connects the deployment's GitHub App from Python.

Wyrd associates an installation only after the browser signed in as the same
principal completes GitHub's install and authorization pages, refuses every
other browser, and never returns a token, code, or key.
"""

from collections.abc import Iterator

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient
from wyrd.github import GithubConnection
from wyrd.testing import GithubAppMock, GithubBrowser, WyrdTestServer

pytestmark = pytest.mark.integration

ACME = [(1, "acme/api"), (2, "acme/web"), (3, "acme/vault")]
"""Installation ``101``'s repositories; ``octo`` cannot reach ``acme/vault``."""

REFUSED = "WYRD_GITHUB_403_CONNECTION_REFUSED"


@pytest.fixture(scope="module")
def mock() -> GithubAppMock:
    """A GitHub where ``octo`` is signed in and installs ``101``."""
    mock = GithubAppMock()
    mock.add_installation(101, ACME)
    mock.add_user("octo", [101])
    mock.hide_repository("octo", 3)
    mock.sign_in("octo")
    mock.install_target(101)
    return mock


@pytest.fixture(scope="module")
def server(mock: GithubAppMock) -> Iterator[WyrdTestServer]:
    """A deployment whose GitHub App is configured against ``mock``."""
    with WyrdTestServer(mutate_env=False, github=mock) as server:
        yield server


def handle(server: WyrdTestServer, key: str) -> GithubConnection:
    """A connection handle acting as ``key``."""
    return GithubConnection(
        WyrdClient(server_url=server.base_url, credential=key, grpc_url=server.grpc_url)
    )


def browser(server: WyrdTestServer, tenant: str, key: str) -> GithubBrowser:
    """A browser signed in to ``tenant`` as ``key``."""
    return GithubBrowser.sign_in(server.base_url, tenant, key)


def assert_refused(answer: tuple[int, object, list[str]], code: str, reason: str) -> None:
    """Fail unless the browser's final answer is a 403 problem with ``code`` and ``reason``."""
    status, body, _ = answer
    assert isinstance(body, dict), body
    assert (status, body.get("code"), body.get("details", {}).get("reason")) == (
        403,
        code,
        reason,
    ), body


def test_admin_connects_reads_and_disconnects(server: WyrdTestServer, mock: GithubAppMock) -> None:
    key = server.bootstrap_service(["admin"], name="github-admin")
    github = handle(server, key)
    admin = browser(server, server.tenant_slug, key)
    assert github.get()["state"] == "not_connected"

    receipt = github.begin()
    status, view, visited = admin.open(receipt["authorization_url"])

    assert status == 200, view
    assert view["host"] == mock.host
    assert view["app"] == {"id": 4242, "slug": "wyrd-test-app"}
    assert view["state"] == "active"
    assert view["installation_id"] == 101
    assert view["repositories"] == [
        {"repository_id": "1", "full_name": "acme/api"},
        {"repository_id": "2", "full_name": "acme/web"},
    ]
    assert view["connected_at"] and view["updated_at"]
    assert github.get() == view

    callback = next(url for url in reversed(visited) if "/v1/integrations/github/callback" in url)
    assert_refused(admin.open(callback), REFUSED, "transaction_invalid")

    released = github.disconnect()
    assert released["state"] == "not_connected"
    assert released["installation_id"] is None
    assert released["repositories"] == []
    with pytest.raises(WyrdError) as again:
        github.disconnect()
    assert again.value.code == "WYRD_GITHUB_404_NOT_CONNECTED"

    for secret in mock.secrets():
        assert secret not in repr((receipt, view, released, again.value))


def test_other_browsers_and_viewers_are_refused(
    server: WyrdTestServer, mock: GithubAppMock
) -> None:
    key = server.bootstrap_service(["admin"], name="github-owner")
    github = handle(server, key)
    other_principal = server.bootstrap_service(["admin"], name="github-other")
    other_tenant = "github-python-b"
    other_tenant_key = server.bootstrap_service_in_tenant(
        server.seed_tenant(other_tenant), ["admin"], name="github-b-admin"
    )

    for intruder in (
        browser(server, server.tenant_slug, other_principal),
        browser(server, other_tenant, other_tenant_key),
    ):
        receipt = github.begin()
        assert_refused(intruder.open(receipt["authorization_url"]), REFUSED, "transaction_invalid")
    assert github.get()["state"] == "not_connected"

    viewer = handle(server, server.bootstrap_service(["viewer"], name="github-viewer"))
    for call in (viewer.begin, viewer.get, viewer.disconnect):
        with pytest.raises(WyrdError) as denied:
            call()
        assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_unconfigured_server_refuses_github(wyrd_server: WyrdTestServer) -> None:
    github = GithubConnection()
    for call in (github.begin, github.get, github.disconnect):
        with pytest.raises(WyrdError) as refused:
            call()
        assert refused.value.code == "WYRD_GITHUB_503_NOT_CONFIGURED"
