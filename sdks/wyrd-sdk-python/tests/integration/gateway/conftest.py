"""Gateway journey fixture: a real server whose adapters reach one recording mock upstream."""

from __future__ import annotations

import threading
from collections.abc import Iterator
from http.server import ThreadingHTTPServer

import pytest
from wyrd.testing import WyrdTestServer

from .support import PROVIDER_KEY, Received, Upstream


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
