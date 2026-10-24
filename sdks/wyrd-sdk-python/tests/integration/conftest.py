"""Session server, local receiver, and Card fixtures shared by the integration journeys.

The session ``WyrdTestServer`` exports ``WYRD_SERVER_URL``, ``WYRD_GRPC_URL``,
and ``WYRD_API_KEY`` the way a deployment's environment does, so SDK and CLI
calls in a test resolve them without arguments.
"""

from __future__ import annotations

import json
import os
import re
import threading
import time
from collections.abc import Generator, Iterator
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.operators import OperatorConnections
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .gateway.support import PROVIDER_KEY, Upstream
from .support import (
    BASELINE_TIMEOUT_SECONDS,
    FRAME_CLAIM,
    LIVE_REVIEW,
    ON_CALL_TOKEN,
    UNBADGED_BASE64,
    Delivery,
    QueryRow,
    Receiver,
    Watch,
    binding_ids,
    client_of,
    declare,
    hydrated,
    pull,
    service_key,
)
from .test_local_development import configure_gateway

AGREES = (
    '{"outcome":"passed","reason":"it agrees",'
    '"citations":[{"slot":"answer","locator":{"json_pointer":""}}]}'
)
"""The judge's Verdict for ``answer-judged``: passed, citing the whole answer."""

FOLLOWS = (
    '{"outcome":"passed","reason":"within the policy",'
    '"citations":[{"slot":"answer","locator":{"json_pointer":"/text"}}]}'
)
"""The policy judge's Verdict for an answer naming no scripted case: passed, citing its text."""

BREAKS = (
    '{"outcome":"failed","reason":"contradicts the policy",'
    '"citations":[{"slot":"answer","locator":{"json_pointer":"/text"}}]}'
)
"""The policy judge's Verdict for an answer containing ``REFUSE-POLICY``: failed, citing its text."""

UNCITED = '{"outcome":"passed","reason":"trust me","citations":[]}'
"""The policy judge's Verdict for an answer containing ``UNCITED``: a pass citing nothing."""

FOREIGN_CITE = (
    '{"outcome":"passed","reason":"see the note",'
    '"citations":[{"slot":"answer","locator":{"json_pointer":"/note"}}]}'
)
"""The policy judge's Verdict for ``JUDGE-FOREIGN-CITE``: a pass citing a pointer the answer lacks."""


def scored(text: str) -> str:
    """The scored judge's Verdict for a prompt carrying ``text``: a pass citing the answer text.

    Its custom data is a score and a category, which the judge's schema
    refuses on ``BAD-DATA`` and which exceeds the 65,536-byte bound on
    ``HUGE-DATA``.
    """
    data: dict[str, object] = {"score": 0.9, "category": "refund"}
    if "BAD-DATA" in text:
        data = {"score": "high", "category": "refund"}
    elif "HUGE-DATA" in text:
        data = {"score": 0.9, "category": "x" * 70_000}
    return json.dumps(
        {
            "outcome": "passed",
            "reason": "scored",
            "citations": [{"slot": "answer", "locator": {"json_pointer": "/text"}}],
            "data": data,
        }
    )


READ_POLICY = {
    "role": "assistant",
    "content": None,
    "tool_calls": [
        {
            "id": "call-read-policy",
            "type": "function",
            "function": {"name": "fs_read", "arguments": '{"path":"/inputs/policy.pdf"}'},
        }
    ],
}
"""The refund reader's first turn: read its workspace copy of the policy as ``fs_read``."""


INVESTIGATE = {
    "role": "assistant",
    "content": None,
    "tool_calls": [
        {
            "id": id,
            "type": "function",
            "function": {"name": tool.replace(".", "_"), "arguments": json.dumps(arguments)},
        }
        for id, tool, arguments in (
            (
                "call-card",
                "cards.get",
                {
                    "card_ref": {
                        "kind": "Service",
                        "name": "policy-desk",
                        "version": "1.0.0",
                    }
                },
            ),
            (
                "call-query",
                "bifrost.query",
                {
                    "sql": "SELECT execution_id FROM vala.verification.results LIMIT 1",
                    "max_rows": 1,
                },
            ),
            ("call-code", "code.run", {"source": "6 * 7"}),
        )
    ],
}
"""The investigator's first turn: call the Card, Bifrost, and code tools at once.

The model names each tool by its provider-facing name, the dotted wire name
spelled with underscores.
"""

REVIEWED = (
    '{"outcome":"passed","reason":"the spans were read",'
    '"citations":[{"slot":"traces","locator":{"json_pointer":"/0"}}]}'
)
"""The coding judge's Verdict once it read the spans: passed, citing the first trace ID."""


def live_query(trace_id: str) -> dict:
    """The coding judge's first turn: read ``trace_id``'s span names live as ``bifrost_query``."""
    sql = (
        "SELECT name FROM vala.traces.spans "
        f"WHERE trace_id = decode('{trace_id}', 'hex') ORDER BY name"
    )
    arguments = json.dumps({"sql": sql, "max_rows": 10})
    return {
        "role": "assistant",
        "content": None,
        "tool_calls": [
            {
                "id": "call-spans",
                "type": "function",
                "function": {"name": "bifrost_query", "arguments": arguments},
            }
        ],
    }


BADGED = (
    '{"outcome":"passed","reason":"a badge is visible",'
    '"citations":[{"slot":"frame","locator":{"page":1}}]}'
)
"""The frame judge's Verdict for a frame showing a badge: passed, citing the frame as its one page."""

NO_BADGE = (
    '{"outcome":"failed","reason":"no badge is visible",'
    '"citations":[{"slot":"frame","locator":{"page":1}}]}'
)
"""The frame judge's Verdict for the frame showing no badge."""


SLOW_SECONDS = 15.0
"""How long the ``SLOW-READER`` reader waits to answer, past its step's five-second deadline."""


def completion(body: dict) -> tuple[int, dict]:
    """The local model's Chat Completions status and reply to ``body``.

    The refund reader fails its provider on ``PROVIDER-DOWN``, breaks its
    schema on ``OFF-SCHEMA``, and answers after ``SLOW_SECONDS`` on
    ``SLOW-READER``; otherwise it first calls ``fs.read`` and, once the tool
    result is back, answers a 100-dollar limit. The investigator calls each
    of its tools once and finishes once the results are back. The coding
    judge reads the trace's spans live through ``bifrost_query`` and passes,
    citing the first trace ID, once the read is back. The frame judge fails
    the frame carrying ``UNBADGED_BASE64`` and passes every other. The policy
    judge (its prompt carries ``Rules:``) fails ``REFUSE-POLICY``, passes
    ``UNCITED`` without a citation, answers ``JUDGE-MALFORMED`` with no
    Verdict, cites a foreign pointer on ``JUDGE-FOREIGN-CITE``, and otherwise
    passes citing the answer text. The scored judge (its prompt carries
    ``SCORE:``) answers ``scored``; every other judge agrees.
    """
    text = json.dumps(body)
    returned = any(message.get("role") == "tool" for message in body.get("messages", []))
    if "REFUND RULES" in text:
        if "PROVIDER-DOWN" in text:
            return 500, {}
        message = (
            {"role": "assistant", "content": '{"max_refund":100}'} if returned else READ_POLICY
        )
        if "OFF-SCHEMA" in text:
            message = {"role": "assistant", "content": '{"wrong":1}'}
        elif "SLOW-READER" in text:
            time.sleep(SLOW_SECONDS)
            message = {"role": "assistant", "content": '{"max_refund":100}'}
    elif "INVESTIGATE" in text:
        message = {"role": "assistant", "content": '{"done":true}'} if returned else INVESTIGATE
    elif FRAME_CLAIM in text:
        content = NO_BADGE if UNBADGED_BASE64 in text else BADGED
        message = {"role": "assistant", "content": content}
    elif LIVE_REVIEW in text:
        [trace_id, *_] = re.findall("[0-9a-f]{32}", text)
        message = {"role": "assistant", "content": REVIEWED} if returned else live_query(trace_id)
    elif "SCORE:" in text:
        message = {"role": "assistant", "content": scored(text)}
    else:
        content = AGREES
        if "Rules:" in text:
            content = FOLLOWS
            for marker, verdict in (
                ("REFUSE-POLICY", BREAKS),
                ("UNCITED", UNCITED),
                ("JUDGE-MALFORMED", "not a verdict"),
                ("JUDGE-FOREIGN-CITE", FOREIGN_CITE),
            ):
                if marker in text:
                    content = verdict
        message = {"role": "assistant", "content": content}
    return 200, {
        "id": "chatcmpl-judge",
        "object": "chat.completion",
        "created": 1,
        "model": "gpt-test",
        "choices": [{"index": 0, "finish_reason": "stop", "message": message}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8},
    }


@pytest.fixture
def fresh_tracer_provider(monkeypatch: pytest.MonkeyPatch) -> None:
    """Unset OpenTelemetry's once-only global tracer provider for one test.

    The API lets a process install its global provider once; restoring the two
    module globals afterwards keeps each telemetry test's install its own.
    """
    from opentelemetry import trace
    from opentelemetry.util._once import Once

    monkeypatch.setattr(trace, "_TRACER_PROVIDER", None)
    monkeypatch.setattr(trace, "_TRACER_PROVIDER_SET_ONCE", Once())


@contextmanager
def serving(delay: float = 0.0) -> Generator[Receiver]:
    """Serve the local model provider and Operator hook endpoint until exit.

    With a ``delay``, each judge call is recorded on arrival and answered
    ``delay`` seconds later, so a test can hold judgments in flight.
    """
    server = ThreadingHTTPServer(("127.0.0.1", 0), BaseHTTPRequestHandler)
    received = Receiver(url=f"http://127.0.0.1:{server.server_address[1]}")

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            body = json.loads(self.rfile.read(int(self.headers["content-length"] or 0)) or b"{}")
            headers = {name.lower(): value for name, value in self.headers.items()}
            if delay:
                received.record(Delivery(self.path, headers, body))
                time.sleep(delay)
            status, reply = completion(body) if self.path == "/v1/chat/completions" else (200, {})
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(reply).encode())
            if not delay:
                received.record(Delivery(self.path, headers, body))

        def log_message(self, format: str, *args: object) -> None:
            """Keep per-request access lines out of the test output."""

    server.RequestHandlerClass = Handler
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        yield received
    finally:
        server.shutdown()


@pytest.fixture
def private_tmp(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """An empty ``TMPDIR``, where a declared ``WyrdState``'s private files land."""
    (tmp_path / "tmp").mkdir()
    monkeypatch.setenv("TMPDIR", str(tmp_path / "tmp"))
    return tmp_path / "tmp"


@pytest.fixture(scope="session")
def receiver() -> Iterator[Receiver]:
    """Serve the session's model provider and Operator hook endpoint."""
    with serving() as received:
        yield received


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
def viewer_key(wyrd_server: WyrdTestServer) -> str:
    """An API key whose principal may read but holds no write or ``verifier:run`` grant."""
    return wyrd_server.bootstrap_service(["viewer"], name="viewer")


@pytest.fixture(scope="session")
def assistant_bundle(wyrd_server: WyrdTestServer, tmp_path_factory: pytest.TempPathFactory) -> Path:
    """The ``assistant`` Service, registered with a fitted latency baseline and downloaded."""
    return registered_assistant(WyrdClient(), tmp_path_factory.mktemp("assistant"), wyrd_server)


def registered_assistant(
    client: WyrdClient, destination: Path, fitted_by: WyrdTestServer | None = None
) -> Path:
    """Register the ``assistant`` Service as ``client`` and download it.

    Its agent checks call the gateway, so their model is deployed there first.
    With ``fitted_by``, the download waits for the latency baseline, which that
    server can only observe in its fixture tenant.
    """
    cards = Cards(client)
    configure_gateway(client)
    declare(cards, "cards/latency_baseline/latency-baseline.yaml")
    declare(cards, "cards/verify_in_real_time/latency-model.yaml")
    registered = declare(cards, "cards/verify_in_real_time/assistant.yaml")
    if fitted_by is not None:
        fitted_by.wait_for_baseline(str(registered["latency-drift"].uid), BASELINE_TIMEOUT_SECONDS)
    return pull(cards, "assistant", destination)


@pytest.fixture(scope="session")
def assistant_key(wyrd_server: WyrdTestServer, assistant_bundle: Path) -> str:
    """A key for the ``assistant`` Service's runtime account, holding only ``workload``."""
    return service_key("assistant")


@pytest.fixture
def assistant(assistant_bundle: Path, assistant_key: str) -> WyrdState:
    """The hydrated ``assistant`` Service, acting as its own key."""
    return hydrated(assistant_bundle, WyrdClient(credential=assistant_key))


@pytest.fixture
def unfitted_assistant(wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path) -> WyrdState:
    """A Service whose bound ``tier-drift`` baseline can never be fitted, acting as its own key."""
    declare(cards, "cards/latency_baseline/latency-baseline.yaml")
    declare(cards, "cards/verify_in_real_time/latency-model.yaml")
    declare(cards, "cards/verify_in_real_time/unfitted-assistant.yaml")
    bundle = pull(cards, "unfitted-assistant", tmp_path)
    return hydrated(bundle, WyrdClient(credential=service_key("unfitted-assistant")))


@contextmanager
def watching(
    tmp_path: Path, service: str, verifier: str, *, fitted: bool = True
) -> Generator[Watch]:
    """Deploy the scheduled-drift ``service`` on a server of its own until exit.

    The server's ``on-call-hooks`` connection points at a hook of its own,
    ``verifier``'s baseline is fitted unless ``fitted`` is false (a Custom
    Verifier has none), and the Service is hydrated with Bifrost started
    under its own key.
    """
    with (
        serving() as hooks,
        WyrdTestServer(verification_runtime=True, mutate_env=False) as server,
    ):
        admin = client_of(server)
        OperatorConnections(admin).create(
            {
                "provider": "http",
                "name": "on-call-hooks",
                "origin": hooks.url,
                "auth": {"scheme": "bearer", "token": ON_CALL_TOKEN},
            }
        )
        cards = Cards(admin)
        declare(cards, "cards/latency_baseline/latency-baseline.yaml")
        registered = declare(cards, f"cards/scheduled_drift_alerts_operator/{service}.yaml")
        if fitted:
            server.wait_for_baseline(str(registered[verifier].uid), BASELINE_TIMEOUT_SECONDS)
        root = registered[service]
        key = service_key(service, admin)
        state = WyrdState.from_path(pull(cards, service, tmp_path), client_of(server, key))
        state.start_bifrost()
        [binding_id] = binding_ids(cards, root)
        try:
            yield Watch(server, state, root, binding_id, hooks, verifier)
        finally:
            state.shutdown()


@pytest.fixture
def latency_watch(tmp_path: Path) -> Iterator[Watch]:
    """The ``latency-watch`` Service with its PSI ``latency-shift`` Verifier fitted."""
    with watching(tmp_path, "latency-watch", "latency-shift") as watch:
        yield watch


@pytest.fixture
def latency_steady(tmp_path: Path) -> Iterator[Watch]:
    """The ``latency-steady`` Service with its SPC ``latency-control`` Verifier fitted."""
    with watching(tmp_path, "latency-steady", "latency-control") as watch:
        yield watch


@pytest.fixture
def latency_budget(tmp_path: Path) -> Iterator[Watch]:
    """The ``latency-budget`` Service with its two-slot Custom ``latency-error-budget`` Verifier."""
    with watching(tmp_path, "latency-budget", "latency-error-budget", fitted=False) as watch:
        yield watch


@pytest.fixture(scope="session")
def observed_bundle(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """The ``observed-service`` graph, registered and downloaded."""
    declare(cards, "cards/observe_a_run/observed-model.yaml")
    declare(cards, "cards/observe_a_run/observed-service.yaml")
    return pull(cards, "observed-service", tmp_path_factory.mktemp("observed"))


@pytest.fixture(scope="session")
def observed_key(observed_bundle: Path) -> str:
    """A key for the ``observed-service`` runtime account, holding only ``workload``."""
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
def metadata_capture(wyrd_server: WyrdTestServer) -> Iterator[None]:
    """Gateway call metadata captured for one test, the session's policy restored after it."""
    gateway = Gateway(client_of(wyrd_server))
    previous = gateway.capture_policy()
    gateway.put_capture_policy({"mode": "metadata", "payload_fields": []})
    yield
    gateway.put_capture_policy(
        {"mode": previous["mode"], "payload_fields": previous["payload_fields"]}
    )


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
