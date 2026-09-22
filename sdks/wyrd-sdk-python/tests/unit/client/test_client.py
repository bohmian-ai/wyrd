"""Public-surface tests for the Python ``WyrdClient`` projection."""

from __future__ import annotations

import pytest


def test_client_without_a_resolvable_credential_raises(monkeypatch: pytest.MonkeyPatch):
    from wyrd import WyrdClient, WyrdError

    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", "/nonexistent-wyrd-home")

    with pytest.raises(WyrdError) as captured:
        WyrdClient()
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_on_behalf_of_rejects_an_unknown_audience():
    from wyrd import WyrdClient, WyrdError

    client = WyrdClient(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
    with pytest.raises(WyrdError) as captured:
        client.on_behalf_of("subject-token", audience="storage")
    assert captured.value.code == "WYRD_SPEC_400_VALIDATION"


def test_on_behalf_of_runs_the_exchange_in_rust():
    """An unreachable server surfaces the Rust transport error, not a Python one."""
    from wyrd import WyrdClient, WyrdError

    client = WyrdClient(server_url="http://127.0.0.1:9", credential="wyrd_test_actor")
    with pytest.raises(WyrdError) as captured:
        client.on_behalf_of("subject-token")
    assert captured.value.code != "WYRD_SPEC_400_VALIDATION"
