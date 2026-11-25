"""Shared setup for the Python unit tests."""

from pathlib import Path

import pytest

CREDENTIAL_VARIABLES = ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT", "WYRD_API_KEY")


@pytest.fixture
def fixtures_dir() -> Path:
    """The Python SDK's offline ``tests/fixtures/``: bundles, authoring Cards, and invalid inputs."""
    return Path(__file__).resolve().parents[1] / "fixtures"


@pytest.fixture
def no_credentials(monkeypatch: pytest.MonkeyPatch) -> None:
    """Empty the credential chain: no token, key, or tenant variable, and no saved login."""
    for name in CREDENTIAL_VARIABLES:
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", "/nonexistent-wyrd-home")


@pytest.fixture
def unreachable_server(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    """One API key and no saved login, aimed at a port nothing serves, so a call fails at transport."""
    for name in CREDENTIAL_VARIABLES:
        monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "wyrd"))
    monkeypatch.setenv("WYRD_API_KEY", "wyrd_unit_test_key")
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:9")
