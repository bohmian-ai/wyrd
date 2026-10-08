"""A person signs in once with ``wyrd auth login``, and their scripts use the saved logins.

Each test starts from its own configuration home holding bob's reader login
to the fixture tenant; the newest-login test also saves alice's admin login to
a second tenant. ``WYRD_TENANT`` is the only selector, as in a person's shell.
"""

from __future__ import annotations

from collections.abc import Iterator
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from .support import FIXTURES

pytestmark = pytest.mark.integration

FIXTURE_TENANT = "test-tenant-1"
"""The test server's own tenant, where bob is a reader."""

SECOND_TENANT = "saved-login-two"
"""A second tenant on the same server, where alice is an administrator."""

PROMPT = FIXTURES / "cards/gateway_inference/ask-prompt.yaml"


@pytest.fixture(scope="module")
def sso_server() -> Iterator[WyrdTestServer]:
    """A server accepting human sign-in for both tenants; it exports no environment."""
    with WyrdTestServer(mutate_env=False, human_sso=True) as server:
        server.activate_human_sso(server.api_key)
        second = server.seed_tenant(SECOND_TENANT)
        server.activate_human_sso(
            server.bootstrap_service_in_tenant(second, ["admin"], name="second-admin")
        )
        yield server


@pytest.fixture
def config_home(
    sso_server: WyrdTestServer, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> Path:
    """This test's configuration home with bob's saved reader login, and no other credential."""
    for name in ("WYRD_API_KEY", "WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path))
    sso_server.save_human_login(tmp_path, FIXTURE_TENANT, "bob", "wyrd-test")
    return tmp_path


@pytest.mark.identity
def test_newest_saved_login_is_used_without_a_selector(
    sso_server: WyrdTestServer, config_home: Path
) -> None:
    sso_server.save_human_login(config_home, SECOND_TENANT, "alice", "alice-password")

    receipt = Cards(WyrdClient(server_url=sso_server.base_url)).register_from_path(PROMPT)

    assert receipt.root.name == "ask-prompt"


@pytest.mark.identity
def test_saved_reader_login_is_denied_a_write(
    sso_server: WyrdTestServer, config_home: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("WYRD_TENANT", FIXTURE_TENANT)
    reader = Cards(WyrdClient(server_url=sso_server.base_url))

    with pytest.raises(WyrdError) as denied:
        reader.register_from_path(PROMPT)
    assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.mark.identity
def test_stale_login_refreshes_and_saves_the_renewal(
    sso_server: WyrdTestServer, config_home: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("WYRD_TENANT", FIXTURE_TENANT)
    sso_server.expire_saved_login(config_home, FIXTURE_TENANT)
    assert sso_server.saved_login_is_stale(config_home, FIXTURE_TENANT)

    Cards(WyrdClient(server_url=sso_server.base_url)).prompt.list(space="default")

    assert not sso_server.saved_login_is_stale(config_home, FIXTURE_TENANT)


@pytest.mark.identity
def test_revoked_login_is_refused(
    sso_server: WyrdTestServer, config_home: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("WYRD_TENANT", FIXTURE_TENANT)
    sso_server.revoke_saved_login(config_home, FIXTURE_TENANT)
    sso_server.expire_saved_login(config_home, FIXTURE_TENANT)

    with pytest.raises(WyrdError) as refused:
        Cards(WyrdClient(server_url=sso_server.base_url)).prompt.list(space="default")
    assert refused.value.code == "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE"


@pytest.mark.identity
def test_selector_naming_no_saved_login_is_refused(
    sso_server: WyrdTestServer, config_home: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("WYRD_TENANT", "no-such-tenant")

    with pytest.raises(WyrdError) as refused:
        Cards(WyrdClient(server_url=sso_server.base_url))
    assert refused.value.code == "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE"


@pytest.mark.identity
def test_explicit_key_beside_a_tenant_selector_is_refused(
    sso_server: WyrdTestServer, config_home: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("WYRD_TENANT", FIXTURE_TENANT)

    with pytest.raises(WyrdError) as refused:
        Cards(WyrdClient(server_url=sso_server.base_url, credential=sso_server.api_key))
    assert refused.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
