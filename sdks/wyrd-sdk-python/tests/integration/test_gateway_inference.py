"""Callers reach the Wyrd gateway with an access token, a Card-scoped key, or a loaded Workflow (AC-051, AC-053)."""

from __future__ import annotations

import threading
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import openai
import pytest
from wyrd import WyrdError, cli
from wyrd.agent import Workflow
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

from .conftest import FIXTURES
from .gateway.support import PROVIDER_KEY, Received, deploy

ASK = FIXTURES / "cards/gateway_inference/ask.yaml"
ASK_EXTERNAL = FIXTURES / "cards/gateway_inference/ask-external.yaml"


def model_calls(received: Received) -> int:
    """Chat Completions requests the gateway dispatched to the provider upstream."""
    return sum(path == "/v1/chat/completions" for path, _ in received)


@pytest.fixture
def gateway(gateway_server: tuple[WyrdTestServer, Received]) -> Iterator[Received]:
    """A gateway server with one OpenAI chat deployment; yields what its upstream received."""
    server, received = gateway_server
    deploy(server, "openai", "gpt-4o", ["chat_completions"])
    yield received


@pytest.mark.integration
def test_openai_client_calls_the_gateway_with_an_access_token(gateway: Received) -> None:
    client = openai.OpenAI(
        base_url=f"{WyrdClient().server_url}/v1",
        api_key=WyrdClient().access_token(),
        max_retries=0,
    )

    completion = client.chat.completions.create(
        model="openai/gpt-4o", messages=[{"role": "user", "content": "hi"}]
    )

    assert completion.choices[0].message.content == "hi"
    assert model_calls(gateway) == 1
    assert gateway[0][1]["authorization"] == f"Bearer {PROVIDER_KEY}"


@pytest.mark.integration
def test_cli_issues_a_card_scoped_key_and_writes_a_provider_credential(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    Cards().register_from_path(str(ASK))

    issued = cli.issue_key(kind="Agent", name="ask-agent", version="1.0.0", space="default")
    written = cli.put_provider_credential(
        {
            "name": "ask-openai",
            "provider": "openai",
            "source": {"environment": {"binding": "test-provider-key"}},
        }
    )

    assert WyrdClient(credential=issued["key"]).access_token()
    assert (written["name"], written["state"]) == ("ask-openai", "active")
    assert Gateway().credential("ask-openai")["source"] == {
        "environment": {"binding": "test-provider-key"}
    }


class Elsewhere(BaseHTTPRequestHandler):
    """The server the ambient configuration names; any request to it is recorded and refused."""

    received: list[str]

    def do_GET(self) -> None:
        self._refuse()

    def do_POST(self) -> None:
        self._refuse()

    def _refuse(self) -> None:
        self.received.append(self.path)
        self.send_error(401)

    def log_message(self, format: str, *args: object) -> None:
        """Keep per-request access lines out of the test output."""


@pytest.fixture
def elsewhere() -> Iterator[tuple[str, list[str]]]:
    """A second server that must never be called; yields its URL and the paths it received."""
    received: list[str] = []
    server = ThreadingHTTPServer(
        ("127.0.0.1", 0), type("RecordingElsewhere", (Elsewhere,), {"received": received})
    )
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        yield f"http://127.0.0.1:{server.server_address[1]}", received
    finally:
        server.shutdown()


@pytest.mark.integration
def test_loaded_workflow_calls_the_gateway_through_its_loading_client(
    gateway: Received,
    elsewhere: tuple[str, list[str]],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Loaded Workflows keep the server and credential that loaded them, whatever the ambient config."""
    cards = Cards()
    cards.register_from_path(str(ASK))
    authored = Workflow.from_path(ASK_EXTERNAL)
    elsewhere_url, elsewhere_received = elsewhere
    monkeypatch.setenv("WYRD_SERVER_URL", elsewhere_url)
    monkeypatch.setenv("WYRD_API_KEY", "wyrd_elsewhere_key")
    monkeypatch.delenv("WYRD_ACCESS_TOKEN", raising=False)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))

    registered = cards.workflow.load(space="default", name="ask", version="1.0.0")
    assert registered.run({"question": "q"}).outputs == {"answer": "hi"}
    assert authored.run({"question": "q"}).outputs == {"answer": "hi"}
    edited = registered.with_outputs({"reply": "steps.ask.output.text"})
    with pytest.raises(WyrdError) as refused:
        registered.with_outputs({"reply": "not a source"})
    assert refused.value.code == "WYRD_WORKFLOW_422_VALIDATION"
    assert edited.run({"question": "q"}).outputs == {"reply": "hi"}
    assert model_calls(gateway) == 3
    assert elsewhere_received == []

    monkeypatch.delenv("WYRD_SERVER_URL")
    monkeypatch.delenv("WYRD_API_KEY")
    assert authored.run({"question": "q"}).outputs == {"answer": "hi"}
    with pytest.raises(WyrdError) as unavailable:
        Workflow.from_path(ASK).run({"question": "q"})
    assert unavailable.value.code == "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE"
    assert model_calls(gateway) == 4
