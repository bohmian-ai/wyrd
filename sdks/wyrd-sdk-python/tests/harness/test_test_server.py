"""The ``wyrd.testing.WyrdTestServer`` harness refuses use outside its lifetime."""

import os

import pytest
from wyrd import WyrdError
from wyrd.testing import WyrdTestServer


def test_bootstrap_service_raises_without_context_manager() -> None:
    with pytest.raises(WyrdError) as raised:
        WyrdTestServer().bootstrap_service([])
    assert raised.value.code == "WYRD_TESTING_500_HARNESS_START"


@pytest.mark.skipif(
    bool(os.environ.get("WYRD_DATABASE_URL")),
    reason="needs no Postgres; py:test:testing runs it without one",
)
def test_enter_fails_without_db() -> None:
    """Entering without a reachable Postgres raises ``WyrdError`` instead of hanging."""
    with pytest.raises(WyrdError) as raised, WyrdTestServer():
        pass
    assert raised.value.code == "WYRD_TESTING_500_HARNESS_START"
