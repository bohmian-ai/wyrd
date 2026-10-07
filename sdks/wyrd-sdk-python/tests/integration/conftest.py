"""Session server, local receiver, and Card fixtures shared by the integration journeys.

The session ``WyrdTestServer`` exports ``WYRD_SERVER_URL``, ``WYRD_GRPC_URL``,
and ``WYRD_API_KEY`` the way a deployment's environment does, so SDK and CLI
calls in a test resolve them without arguments.
"""

from __future__ import annotations

import json
import threading
from collections.abc import Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import BaseModel
from wyrd import cli
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.cards import CardRef, Cards
from wyrd.model import ModelInterface
from wyrd.operators import OperatorConnections
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .gateway.support import PROVIDER_KEY, Received, Upstream

if TYPE_CHECKING:
    from wyrd.operators import OperatorConnectionView

FIXTURES = Path(__file__).resolve().parents[4] / "fixtures"
"""The repository-root fixture corpus shared by the Rust, Python, and TypeScript journeys."""

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


@dataclass
class Delivery:
    """One request the local receiver accepted."""

    path: str
    headers: dict[str, str]
    body: dict


@dataclass
class Receiver:
    """A loopback endpoint standing in for the judge provider and Operator hooks.

    ``POST /v1/chat/completions`` answers as a passing LLM judge; any other
    ``POST`` is accepted as an Operator delivery. Every request is recorded in
    arrival order.
    """

    url: str
    deliveries: list[Delivery] = field(default_factory=list)
    delivered: threading.Condition = field(default_factory=threading.Condition)

    def to(self, path: str) -> list[Delivery]:
        """Every delivery received on ``path`` so far."""
        return [delivery for delivery in self.deliveries if delivery.path == path]

    def wait_for(self, path: str, timeout: float) -> Delivery:
        """Block until a delivery arrives on ``path`` and return the first one.

        Raises:
            TimeoutError: when nothing reaches ``path`` within ``timeout`` seconds.
        """
        with self.delivered:
            if not self.delivered.wait_for(lambda: self.to(path), timeout):
                raise TimeoutError(f"nothing was delivered to {path} in {timeout} s")
        return self.to(path)[0]


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
            with received.delivered:
                received.deliveries.append(Delivery(self.path, headers, body))
                received.delivered.notify_all()

        def log_message(self, *args: object) -> None:
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


def register(cards: Cards, path: str) -> dict[str, CardRef]:
    """Register one checked-in fixture file and return every Card it registered, by name."""
    receipt = cards.register_from_path(str(FIXTURES / path))
    return {outcome.card_ref.name: outcome.card_ref for outcome in receipt.outcomes}


def download(service: str, output_dir: Path) -> Path:
    """Hydrate the registered ``service`` graph into ``output_dir`` with ``wyrd get``."""
    cli.get(output_dir=output_dir, kind="Service", space="default", name=service, version="1.0.0")
    return output_dir


def service_key(server: WyrdTestServer, service: str) -> str:
    """Issue the registered ``service``'s own key, granted the built-in ``agent`` Role.

    A Card-bound principal starts with only the workload Role; ``agent`` adds
    ``evals:run``. No public surface grants a Role yet, so the harness does.
    """
    return server.credential_registered_service(f"default/Service/{service}@1.0.0", ["agent"])


BASELINE_TIMEOUT_SECONDS = 90.0
"""How long a fixture waits for a Drift Verifier's baseline to be fitted."""


class StandInModel(ModelInterface):
    """Stand-in for the fixture Models' ``Custom`` loader.

    The journeys observe and verify the Model Card without running it, and the
    Card names a loader module that does not exist offline. Supplying this
    instance for the alias keeps hydration from importing it.
    """

    def __init__(self) -> None:
        """Start with the empty holder slot the Model holder expects after load."""
        super().__init__()
        self.model: object = None

    def save(self, path: Path, save_kwargs: dict[str, object] | None = None) -> None:
        """Never called: the journeys register checked-in Cards, they do not save one."""
        raise NotImplementedError

    def load(self, path: Path, load_kwargs: dict[str, object] | None = None) -> None:
        """Read nothing and publish an identity function as the loaded model."""
        self.model = lambda value: value


@pytest.fixture(scope="session")
def bifrost(wyrd_server: WyrdTestServer) -> Bifrost:
    """The session administrator's query-only Bifrost client."""
    return Bifrost()


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
    return download("assistant", tmp_path_factory.mktemp("assistant"))


@pytest.fixture(scope="session")
def assistant_key(wyrd_server: WyrdTestServer, assistant_bundle: Path) -> str:
    """The ``assistant`` Service's own API key."""
    return service_key(wyrd_server, "assistant")


@pytest.fixture
def assistant(
    assistant_bundle: Path, assistant_key: str, monkeypatch: pytest.MonkeyPatch
) -> WyrdState:
    """The hydrated ``assistant`` Service, running under its own key."""
    monkeypatch.setenv("WYRD_API_KEY", assistant_key)
    return WyrdState.from_path(assistant_bundle, interfaces={"model": StandInModel()})


@pytest.fixture
def unfitted_assistant(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> WyrdState:
    """A Service whose bound ``tier-drift`` baseline can never be fitted."""
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/unfitted-assistant.yaml")
    bundle = download("unfitted-assistant", tmp_path)
    monkeypatch.setenv("WYRD_API_KEY", service_key(wyrd_server, "unfitted-assistant"))
    return WyrdState.from_path(bundle, interfaces={"model": StandInModel()})


ON_CALL_TOKEN = "on-call-bearer-token"
"""The bearer secret the ``on-call-hooks`` connection presents to the receiver."""


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
    monkeypatch: pytest.MonkeyPatch,
) -> Iterator[WyrdState]:
    """The ``latency-watch`` Service, fitted and hydrated, with Bifrost started under its key."""
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    registered = register(cards, "cards/scheduled_drift_alerts_operator/latency-watch.yaml")
    wyrd_server.wait_for_baseline(str(registered["latency-shift"].uid), BASELINE_TIMEOUT_SECONDS)
    bundle = download("latency-watch", tmp_path)
    monkeypatch.setenv("WYRD_API_KEY", service_key(wyrd_server, "latency-watch"))
    state = WyrdState.from_path(bundle)
    state.start_bifrost()
    yield state
    state.shutdown()


class QueryRow(BaseModel):
    """The user columns of the query journeys' seeded table."""

    id: int
    value: str


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
    wyrd_server.flush_bifrost()
    return fqn


@pytest.fixture
def gateway_server(wyrd_server: WyrdTestServer) -> Iterator[tuple[WyrdTestServer, Received]]:
    """Server whose built-in adapters reach a recording mock upstream.

    Starts after the session server and owns its own environment patch, so a
    test's ``monkeypatch`` unwinds before this server restores the session
    endpoints.
    """
    received: Received = []
    handler = type("RecordingUpstream", (Upstream,), {"received": received})
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        base = f"http://127.0.0.1:{upstream.server_address[1]}"
        with pytest.MonkeyPatch.context() as env:
            env.setenv("WYRD_TEST_GATEWAY_PROVIDER_KEY", PROVIDER_KEY)
            with WyrdTestServer(provider_base_url=base) as server:
                yield server, received
    finally:
        upstream.shutdown()
