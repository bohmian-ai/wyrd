"""Python SDK journey over a CLI-established saved user login."""

from __future__ import annotations

from pathlib import Path
from uuid import uuid4

import pytest
from wyrd import WyrdError
from wyrd.cards import Cards
from wyrd.prompt import Prompt, PromptCard
from wyrd.testing import WyrdTestServer

FIXTURE_TENANT = "test-tenant-1"
SECOND_TENANT = "saved-login-two"


def _prompt() -> PromptCard:
    return PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello"),
        space="saved-login",
        name=f"saved-login-{uuid4().hex[:8]}",
    )


def _reason(error: pytest.ExceptionInfo[WyrdError]) -> str:
    assert error.value.code == "WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE"
    assert error.value.details is not None
    return error.value.details["reason"]


@pytest.mark.integration
@pytest.mark.identity
def test_saved_user_auth_journey(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    for name in ("WYRD_API_KEY", "WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path))
    with WyrdTestServer(mutate_env=False, human_sso=True) as server:
        url = server.base_url
        server.activate_human_sso(server.api_key)
        second = server.seed_tenant(SECOND_TENANT)
        server.activate_human_sso(server.bootstrap_service_in_tenant(second, ["admin"]))
        server.save_human_login(tmp_path, FIXTURE_TENANT, "bob", "wyrd-test")
        server.save_human_login(tmp_path, SECOND_TENANT, "alice", "alice-password")

        # Without a selector the newest login, alice's admin login, is used; a
        # selector must name a saved login.
        Cards(server_url=url).register(_prompt())
        with pytest.raises(WyrdError) as unmatched:
            Cards(server_url=url, tenant="no-such-tenant")
        assert _reason(unmatched) == "tenant_mismatch"

        # Bob is a reader: the read is allowed and the write denied.
        reader = Cards(server_url=url, tenant=FIXTURE_TENANT)
        reader.prompt.list(space="saved-login")
        with pytest.raises(WyrdError) as denied:
            reader.register(_prompt())
        assert denied.value.status == 403
        Cards(server_url=url, tenant=SECOND_TENANT).prompt.list(space="saved-login")

        # A stale login renews through Wyrd and the renewal is saved.
        server.expire_saved_login(tmp_path, FIXTURE_TENANT)
        assert server.saved_login_is_stale(tmp_path, FIXTURE_TENANT)
        Cards(server_url=url, tenant=FIXTURE_TENANT).prompt.list(space="saved-login")
        assert not server.saved_login_is_stale(tmp_path, FIXTURE_TENANT)

        # An explicit machine credential overrides the saved reader; it names
        # its own tenant, so a selector beside it is refused.
        with pytest.raises(WyrdError) as selected:
            Cards(server_url=url, credential=server.api_key, tenant=FIXTURE_TENANT)
        assert selected.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
        assert "already names its tenant" in str(selected.value)
        Cards(server_url=url, credential=server.api_key).register(_prompt())

        # Once the chain is revoked the login fails closed and asks for a new login.
        server.revoke_saved_login(tmp_path, FIXTURE_TENANT)
        server.expire_saved_login(tmp_path, FIXTURE_TENANT)
        with pytest.raises(WyrdError) as refused:
            Cards(server_url=url, tenant=FIXTURE_TENANT).prompt.list(space="saved-login")
        assert _reason(refused) == "refresh_refused"
