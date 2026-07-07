"""Boundary tests for the Vala bifrost/observe Python surface."""

from __future__ import annotations

import pytest


def test_extension_submodules_import():
    import wyrd._wyrd.bifrost  # noqa: F401
    import wyrd._wyrd.observe  # noqa: F401


def test_bifrost_construction_raises_transport_unavailable():
    from wyrd.bifrost import Bifrost

    with pytest.raises(RuntimeError, match="bifrost transport unavailable"):
        Bifrost("http://localhost", "secret")


def test_producer_key_and_client_scope_are_not_importable():
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ProducerKey  # noqa: F401
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ClientScope  # noqa: F401
