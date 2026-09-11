"""Boundary tests for the Vala bifrost/observe Python surface."""

from __future__ import annotations

import pytest


def test_extension_submodules_import():
    import wyrd._wyrd.bifrost  # noqa: F401
    import wyrd._wyrd.observe  # noqa: F401


def test_bifrost_without_a_resolvable_credential_raises(monkeypatch: pytest.MonkeyPatch):
    """Constructing with nothing names the failure instead of connecting.

    Every transport argument is optional and resolves through the credential
    chain, so the only construction failure left is an empty chain — and it must
    say so through the typed exception rather than a generic error.
    """

    from wyrd import WyrdError
    from wyrd.bifrost import Bifrost

    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY"):
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", "/nonexistent-wyrd-home")

    with pytest.raises(WyrdError) as captured:
        Bifrost()
    assert captured.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"


def test_producer_key_and_client_scope_are_not_importable():
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ProducerKey  # noqa: F401
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ClientScope  # noqa: F401
