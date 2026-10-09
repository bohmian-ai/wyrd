"""A person signs in once with the CLI and completes the local workflow from the saved login.

On the Keycloak identity lane, the telemetry exporter and Gateway client built
from that login keep working after its first access token expires.
"""

from __future__ import annotations

import time
from collections.abc import Iterator
from pathlib import Path

import httpx
import pytest
from opentelemetry import trace
from wyrd.bifrost import Bifrost
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from .test_local_development import Attributed, export, invoke, local_server, work

pytestmark = pytest.mark.integration

ACCESS_TTL_SECONDS = 2
"""Access-token lifetime the workflow outlives."""

FIXTURE_TENANT = "test-tenant-1"
"""The test server's own tenant, where alice is an administrator."""


@pytest.fixture(scope="module")
def server() -> Iterator[WyrdTestServer]:
    """The local deployment, accepting human sign-in with short-lived access tokens."""
    for server in local_server(human_sso=True, access_ttl_seconds=ACCESS_TTL_SECONDS):
        server.activate_human_sso(server.api_key)
        yield server


@pytest.mark.identity
@pytest.mark.usefixtures("fresh_tracer_provider")
def test_saved_login_completes_the_workflow_past_token_expiry(
    server: WyrdTestServer, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    for name in ("WYRD_API_KEY", "WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN"):
        monkeypatch.delenv(name, raising=False)
    config_home = tmp_path / "config"
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(config_home))
    monkeypatch.setenv("WYRD_TENANT", FIXTURE_TENANT)
    server.save_human_login(config_home, FIXTURE_TENANT, "alice", "alice-password")
    client = WyrdClient(server_url=server.base_url, grpc_url=server.grpc_url)
    original = client.access_token()

    worked = work(client, tmp_path / "bundle")
    time.sleep(ACCESS_TTL_SECONDS + 1)

    lapsed = httpx.get(
        f"{server.base_url}/v1/cards", headers={"Authorization": f"Bearer {original}"}
    )
    assert lapsed.status_code == 401
    assert invoke(client) == "hi"
    run = worked.state.run("agent")
    export(run)
    assert trace.get_tracer_provider().force_flush()
    rows = Bifrost(client=client).sql(
        "SELECT card_uid FROM vala.traces.spans WHERE run_id = $1",
        [run.run_id],
        model=Attributed,
    )
    assert rows == [Attributed(card_uid=worked.agent_uid)]
