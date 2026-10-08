"""Session server, local receiver, and Card fixtures shared by the integration journeys.

The session ``WyrdTestServer`` exports ``WYRD_SERVER_URL``, ``WYRD_GRPC_URL``,
and ``WYRD_API_KEY`` the way a deployment's environment does, so SDK and CLI
calls in a test resolve them without arguments.
"""

from __future__ import annotations

import json
import os
import threading
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.operators import OperatorConnections
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .gateway.support import PROVIDER_KEY, Upstream
from .support import (
    BASELINE_TIMEOUT_SECONDS,
    ON_CALL_TOKEN,
    Delivery,
    QueryRow,
    Receiver,
    download,
    hydrated,
    register,
    service_key,
)

if TYPE_CHECKING:
    from wyrd.operators import OperatorConnectionView

JUDGE_VERDICT = {
    "id": "chatcmpl-judge",
    "object": "chat.completion",
    "created": 1,
    "model": "gpt-test",
    "choices": [
        {
            "index": 0,
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": '{"passed":true}'},
        }
    ],
    "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8},
}
"""The local LLM judge's answer: the graded answer passes."""


@pytest.fixture(scope="session")
def receiver() -> Iterator[Receiver]:
    """Serve the session's judge provider and Operator hook endpoint."""
    server = ThreadingHTTPServer(("127.0.0.1", 0), BaseHTTPRequestHandler)
    received = Receiver(url=f"http://127.0.0.1:{server.server_address[1]}")

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            body = json.loads(self.rfile.read(int(self.headers["content-length"] or 0)) or b"{}")
            headers = {name.lower(): value for name, value in self.headers.items()}
            reply = JUDGE_VERDICT if self.path == "/v1/chat/completions" else {}
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(reply).encode())
            received.record(Delivery(self.path, headers, body))

        def log_message(self, format: str, *args: object) -> None:
            """Keep per-request access lines out of the test output."""

    server.RequestHandlerClass = Handler
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        yield received
    finally:
        server.shutdown()


@pytest.fixture(scope="session")
def wyrd_server(receiver: Receiver) -> Iterator[WyrdTestServer]:
    """The session's server, with Verifiers running and the judge on the local receiver."""
    with WyrdTestServer(verification_runtime=True, provider_base_url=receiver.url) as server:
        yield server


@pytest.fixture(scope="session")
def cards(wyrd_server: WyrdTestServer) -> Cards:
    """The session administrator's registry client, resolved from the environment."""
    return Cards()


@pytest.fixture(scope="session")
def bifrost(wyrd_server: WyrdTestServer) -> Bifrost:
    """The session administrator's query-only Bifrost client."""
    return Bifrost()


@pytest.fixture(scope="session")
def other_tenant(wyrd_server: WyrdTestServer) -> str:
    """A second tenant on the session server, for isolation stories."""
    return wyrd_server.seed_tenant("other-tenant")


@pytest.fixture(scope="session")
def reader_key(wyrd_server: WyrdTestServer) -> str:
    """An API key whose principal may read but holds no write or ``evals:run`` grant."""
    return wyrd_server.bootstrap_service(["reader"], name="reader")


@pytest.fixture(scope="session")
def assistant_bundle(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """The ``assistant`` Service, registered with a fitted latency baseline and downloaded."""
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    registered = register(cards, "cards/verify_in_real_time/assistant.yaml")
    wyrd_server.wait_for_baseline(str(registered["latency-drift"].uid), BASELINE_TIMEOUT_SECONDS)
    return download(cards, "assistant", tmp_path_factory.mktemp("assistant"))


@pytest.fixture(scope="session")
def assistant_key(wyrd_server: WyrdTestServer, assistant_bundle: Path) -> str:
    """The ``assistant`` Service's own key, issued with ``wyrd auth issue-key`` and no added Role."""
    return service_key("assistant")


@pytest.fixture
def assistant(assistant_bundle: Path, assistant_key: str) -> WyrdState:
    """The hydrated ``assistant`` Service, acting as its own key."""
    return hydrated(assistant_bundle, WyrdClient(credential=assistant_key))


@pytest.fixture
def unfitted_assistant(wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path) -> WyrdState:
    """A Service whose bound ``tier-drift`` baseline can never be fitted, acting as its own key."""
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/unfitted-assistant.yaml")
    bundle = download(cards, "unfitted-assistant", tmp_path)
    return hydrated(bundle, WyrdClient(credential=service_key("unfitted-assistant")))


@pytest.fixture(scope="session")
def on_call_hooks(wyrd_server: WyrdTestServer, receiver: Receiver) -> OperatorConnectionView:
    """The ``on-call-hooks`` HTTP connection: the receiver's origin and a bearer token."""
    return OperatorConnections().create(
        {
            "provider": "http",
            "name": "on-call-hooks",
            "origin": receiver.url,
            "auth": {"scheme": "bearer", "token": ON_CALL_TOKEN},
        }
    )


@pytest.fixture
def latency_watch(
    wyrd_server: WyrdTestServer,
    cards: Cards,
    on_call_hooks: OperatorConnectionView,
    tmp_path: Path,
) -> Iterator[WyrdState]:
    """The ``latency-watch`` Service, fitted and hydrated, with Bifrost started under its key."""
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    registered = register(cards, "cards/scheduled_drift_alerts_operator/latency-watch.yaml")
    wyrd_server.wait_for_baseline(str(registered["latency-shift"].uid), BASELINE_TIMEOUT_SECONDS)
    bundle = download(cards, "latency-watch", tmp_path)
    state = WyrdState.from_path(bundle, WyrdClient(credential=service_key("latency-watch")))
    state.start_bifrost()
    yield state
    state.shutdown()


@pytest.fixture(scope="session")
def observed_bundle(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """The ``observed-service`` graph, registered and downloaded."""
    register(cards, "cards/observe_a_run/observed-model.yaml")
    register(cards, "cards/observe_a_run/observed-service.yaml")
    return download(cards, "observed-service", tmp_path_factory.mktemp("observed"))


@pytest.fixture(scope="session")
def observed_key(observed_bundle: Path) -> str:
    """A key bound to the ``observed-service`` Card, issued with ``wyrd auth issue-key``."""
    return service_key("observed-service")


@pytest.fixture
def observed(observed_bundle: Path, observed_key: str) -> WyrdState:
    """A fresh state of the ``observed-service`` acting as its own key; Bifrost is not started."""
    return hydrated(observed_bundle, WyrdClient(credential=observed_key))


@pytest.fixture
def observed_with_bifrost(observed: WyrdState) -> Iterator[WyrdState]:
    """The ``observed-service`` state with Bifrost started under the Service's own key."""
    observed.start_bifrost()
    yield observed
    observed.shutdown()


@pytest.fixture
def otlp_environment(wyrd_server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch) -> None:
    """Point every stock OTLP exporter at the deployment's Wyrd gRPC address and key."""
    monkeypatch.setenv("OTEL_EXPORTER_OTLP_ENDPOINT", os.environ["WYRD_GRPC_URL"])
    monkeypatch.setenv("OTEL_EXPORTER_OTLP_INSECURE", "true")
    monkeypatch.setenv("OTEL_EXPORTER_OTLP_HEADERS", f"x-wyrd-api-key={wyrd_server.api_key}")


@pytest.fixture(scope="session")
def query_table(wyrd_server: WyrdTestServer) -> str:
    """A ``vala.datasets`` table holding rows 1-3, written and published through the SDK."""
    fqn = "vala.datasets.query_rows"
    writer = Bifrost(TableConfig(QueryRow, fqn))
    assert writer.register() == "created"
    for row_id, value in ((1, "one"), (2, "two"), (3, "three")):
        writer.insert({"id": row_id, "value": value})
    writer.flush()
    writer.shutdown()
    # Publish the flushed rows so every query journey reads them.
    wyrd_server.flush_bifrost()
    return fqn


@pytest.fixture
def gateway_server(wyrd_server: WyrdTestServer) -> Iterator[tuple[WyrdTestServer, Receiver]]:
    """Server whose built-in adapters reach a recording mock upstream.

    The server leaves the session endpoints in the environment untouched, so
    every call to it passes an explicit client.
    """
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("RecordingUpstream", (Upstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        with pytest.MonkeyPatch.context() as env:
            env.setenv("WYRD_TEST_GATEWAY_PROVIDER_KEY", PROVIDER_KEY)
            with WyrdTestServer(provider_base_url=recorded.url, mutate_env=False) as server:
                yield server, recorded
    finally:
        upstream.shutdown()
