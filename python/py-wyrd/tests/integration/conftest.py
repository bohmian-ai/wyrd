"""Session-scoped fixtures for wyrd integration tests."""

import pytest
from wyrd.testing import WyrdTestServer


@pytest.fixture(scope="session")
def wyrd_server():
    with WyrdTestServer(mutate_env=True) as srv:
        yield srv
