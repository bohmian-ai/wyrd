"""Loaded Workflows call the Wyrd gateway through the client that loaded them.

A Workflow loaded from the registry, or from a file whose refs were read from
the registry, keeps that server and credential for its ``wyrd_gateway`` steps.
Ambient client configuration pointing elsewhere, or missing entirely, does
not redirect the run.
"""

from __future__ import annotations

import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

import pytest
from wyrd import WyrdError
from wyrd.agent import Workflow
from wyrd.cards import Cards
from wyrd.testing import WyrdTestServer

from .support import Received, deploy

PROMPT = """\
apiVersion: wyrd/v1
kind: Prompt
metadata:
  space: workflow-gateway
  name: ask-prompt
  version: "1.0.0"
spec:
  model: gpt-4o
  request:
    provider: open_ai_chat_completion
    body:
      model: gpt-4o
      messages:
        - role: user
          content: "{{question}}"
  variables: [question]
  response_type: text
"""

AGENT = """\
apiVersion: wyrd/v1
kind: Agent
metadata:
  space: workflow-gateway
  name: ask-agent
  version: "1.0.0"
spec:
  prompt: ./prompt.yaml
  tool_names: []
  run_config:
    max_iterations: 1
"""

WORKFLOW = """\
apiVersion: wyrd/v1
kind: Workflow
metadata:
  space: workflow-gateway
  name: {name}
  version: "1.0.0"
spec:
  llm_route:
    kind: wyrd_gateway
  inputs:
    question:
      type: str
      value: ""
  steps:
    - id: ask
      action:
        type: agent
        target: {target}
      inputs:
        question: input.question
  outputs:
    answer: steps.ask.output.text
"""


class Elsewhere(BaseHTTPRequestHandler):
    """Server the ambient configuration names; it must never be called."""

    received: list[str]

    def do_GET(self) -> None:
        self._refuse()

    def do_POST(self) -> None:
        self._refuse()

    def _refuse(self) -> None:
        self.received.append(self.path)
        self.send_error(401)

    def log_message(self, format: str, *args: Any) -> None:
        del format, args


def _bundle(directory: Path) -> tuple[Path, Path]:
    """Write the Prompt, Agent, a local Workflow, and one referencing the registered Agent."""
    directory.mkdir()
    (directory / "prompt.yaml").write_text(PROMPT)
    (directory / "agent.yaml").write_text(AGENT)
    local = directory / "workflow.yaml"
    local.write_text(WORKFLOW.format(name="ask", target="./agent.yaml"))
    external = directory / "external.yaml"
    external.write_text(
        WORKFLOW.format(
            name="ask-external",
            target='{kind: Agent, name: ask-agent, version: "1.0.0"}',
        )
    )
    return local, external


def _model_calls(received: Received) -> int:
    """Chat Completions requests the gateway dispatched to the provider upstream."""
    return sum(path == "/v1/chat/completions" for path, _ in received)


@pytest.mark.integration
def test_loaded_workflow_calls_the_gateway_through_its_loading_client(
    gateway_server: tuple[WyrdTestServer, Received],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    server, upstream = gateway_server
    deploy(server, "openai", "gpt-4o", ["chat_completions"])
    local, external = _bundle(tmp_path / "bundle")
    cards = Cards(server_url=server.base_url, credential=server.api_key)
    cards.register_from_path(local)

    # The authored file reads its external Agent through the ambient client,
    # which still names this server.
    authored = Workflow.from_path(external)

    elsewhere_received: list[str] = []
    handler = type("RecordingElsewhere", (Elsewhere,), {"received": elsewhere_received})
    elsewhere = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=elsewhere.serve_forever, daemon=True).start()
    try:
        # Ambient configuration now names another server and credential.
        monkeypatch.setenv("WYRD_SERVER_URL", f"http://127.0.0.1:{elsewhere.server_address[1]}")
        monkeypatch.setenv("WYRD_API_KEY", "wyrd_elsewhere_key")
        monkeypatch.delenv("WYRD_ACCESS_TOKEN", raising=False)
        monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))

        registered = cards.workflow.load(space="workflow-gateway", name="ask", version="1.0.0")
        run = registered.run({"question": "q"})
        assert run.status == "succeeded", run.error
        assert run.outputs == {"answer": "hi"}
        assert _model_calls(upstream) == 1

        run = authored.run({"question": "q"})
        assert run.status == "succeeded", run.error
        assert run.outputs == {"answer": "hi"}
        assert _model_calls(upstream) == 2

        # An authoring edit keeps the loading client; a refused edit changes nothing.
        registered.with_outputs({"reply": "steps.ask.output.text"})
        with pytest.raises(WyrdError):
            registered.with_outputs({"reply": "not a source"})
        run = registered.run({"question": "q"})
        assert run.status == "succeeded", run.error
        assert run.outputs == {"reply": "hi"}
        assert _model_calls(upstream) == 3
        assert elsewhere_received == []

        # With no ambient configuration at all, loaded Workflows still run,
        # while a wholly local file was loaded through no client and has none.
        monkeypatch.delenv("WYRD_SERVER_URL")
        monkeypatch.delenv("WYRD_API_KEY")
        run = authored.run({"question": "q"})
        assert run.status == "succeeded", run.error
        assert _model_calls(upstream) == 4
        with pytest.raises(WyrdError) as unavailable:
            Workflow.from_path(local).run({"question": "q"})
        assert unavailable.value.code == "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE"
        assert _model_calls(upstream) == 4
    finally:
        elsewhere.shutdown()
