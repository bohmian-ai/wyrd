"""Shared fixtures for wyrd integration tests."""

import threading
import uuid
from collections.abc import Iterator
from http.server import ThreadingHTTPServer

import pytest
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.testing import WyrdTestServer

from .gateway.support import PROVIDER_KEY, Received, Upstream


@pytest.fixture(scope="session")
def wyrd_server():
    with WyrdTestServer(mutate_env=True) as srv:
        yield srv


class QueryRow(BaseModel):
    """The user columns of the query journeys' seeded table."""

    id: int
    value: str


@pytest.fixture
def query_table(wyrd_server: WyrdTestServer) -> str:
    """A fresh `vala.datasets` table holding rows 1-3, written and published through the SDK."""
    fqn = f"vala.datasets.query_{uuid.uuid4().hex}"
    writer = Bifrost(
        TableConfig(QueryRow, fqn), server_url=wyrd_server.base_url, credential=wyrd_server.api_key
    )
    assert writer.register() == "created"
    for row_id, value in ((1, "one"), (2, "two"), (3, "three")):
        writer.insert({"id": row_id, "value": value})
    writer.flush()
    writer.shutdown()
    wyrd_server.flush_bifrost()
    return fqn


@pytest.fixture
def gateway_server(
    monkeypatch: pytest.MonkeyPatch,
) -> Iterator[tuple[WyrdTestServer, Received]]:
    """Server whose built-in adapters reach a recording mock upstream."""
    monkeypatch.setenv("WYRD_TEST_GATEWAY_PROVIDER_KEY", PROVIDER_KEY)
    received: Received = []
    handler = type("RecordingUpstream", (Upstream,), {"received": received})
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        base = f"http://127.0.0.1:{upstream.server_address[1]}"
        with WyrdTestServer(mutate_env=True, provider_base_url=base) as server:
            yield server, received
    finally:
        upstream.shutdown()
