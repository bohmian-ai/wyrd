"""Smoke tests for the wyrd.testing.WyrdTestServer context manager."""

import pytest
from wyrd._wyrd import WyrdError
from wyrd.testing import WyrdTestServer


def test_import():
    assert WyrdTestServer is not None


def test_construct_default():
    srv = WyrdTestServer()
    assert srv is not None


def test_construct_no_env_mutation():
    srv = WyrdTestServer(mutate_env=False)
    assert srv is not None


def test_bootstrap_service_raises_without_context_manager():
    srv = WyrdTestServer()
    with pytest.raises(RuntimeError, match="not started"):
        srv.bootstrap_service([])


def test_enter_fails_without_db():
    """__enter__ must raise WyrdError when no Postgres is available.

    Verifies the Rust-to-Python error path compiled correctly after the
    ServerAuth/ServerAuthz state-abstraction refactor.
    """
    import os

    if os.environ.get("WYRD_DATABASE_URL"):
        pytest.skip("DB available — use py:test:testing with db:setup-roles for a full run")

    with pytest.raises(WyrdError):
        with WyrdTestServer():
            pass
