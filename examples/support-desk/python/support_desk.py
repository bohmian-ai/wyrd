"""The support desk: an Agent-backed Service that answers customer questions,
records each ticket, and is verified continuously and in real time.

``deploy`` registers the checked-in Cards under ``service/``, which ensures the
declared ``vala.datasets.tickets`` table, turns on metadata capture, and checks
the Agent's model is deployed. ``serve`` answers every question in its own
Agent Run. ``wait_for_verdicts`` waits for both Verifiers to judge every
answer, and ``explain`` joins one Run's evidence over MCP.

Run it against a server: ``WYRD_SERVER_URL=... WYRD_API_KEY=... python support_desk.py``.
"""

from __future__ import annotations

import json
import re
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from opentelemetry import trace
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards, RegistrationReceipt
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.state import WyrdState

REQUESTS = 100
"""How many requests ``serve`` answers; every tenth asks for a refund."""

SERVICE = Path(__file__).resolve().parents[1] / "service"
"""The checked-in Service Cards."""

MCP_VERSION = "2026-07-28"
"""The MCP protocol revision Wyrd serves."""


@dataclass
class Desk:
    """The deployed support desk: its registration and loaded Card graph."""

    receipt: RegistrationReceipt
    state: WyrdState

    def uid(self, name: str) -> str:
        """The registered UID of the Card named ``name``."""
        return next(
            str(outcome.card_ref.uid)
            for outcome in self.receipt.outcomes
            if outcome.card_ref.name == name
        )


@dataclass
class Served:
    """One answered request."""

    run_id: str
    passed: bool
    """Whether ``no-refund-promise`` passed the answer in real time."""


class VerdictCount(BaseModel):
    """One verdict count row."""

    card_uid: str
    verdict: str
    n: int


def deploy(client: WyrdClient, bundle: Path) -> Desk:
    """Register and hydrate the Service into ``bundle``, capture gateway call
    metadata, and check the Agent's model is deployed.

    Raises:
        WyrdError: the registration refusal, including
            ``WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`` for an incompatible
            existing ``tickets`` table.
        LookupError: naming the model when no deployment serves it.
    """
    cards = Cards(client)
    receipt = cards.register_from_path(SERVICE / "support-desk.yaml")
    state = WyrdState.from_path(cards.hydrate(receipt.root, bundle).destination, client)
    gateway = Gateway(client)
    gateway.put_capture_policy({"mode": "metadata", "payload_fields": []})
    model = state.card("prompt").spec["model"]
    if not any(d["model"]["model"] == model for d in gateway.deployments()):
        raise LookupError(f"the support Agent's model {model!r} is not deployed")
    return Desk(receipt, state)


def question(index: int) -> str:
    """The customer question of request ``index``."""
    if index % 10 == 0:
        return f"Can I get a refund for order {index}?"
    return f"Where is order {index}?"


def serve(desk: Desk) -> list[Served]:
    """Answer ``REQUESTS`` questions, each in its own Agent Run inside a
    ``support-desk.request`` span: invoke the Agent through the gateway, record
    the ticket, observe the answer for ``answer-quality``, and judge it with
    ``no-refund-promise``. Telemetry and observations are flushed on return.
    """
    state = desk.state
    state.start_bifrost()
    state.start_telemetry()
    tracer = trace.get_tracer("support-desk")
    served = []
    for index in range(REQUESTS):
        run = state.run("agent")
        asked = question(index)
        with run, tracer.start_as_current_span("support-desk.request"):
            answer = run.invoke({"question": asked})
            run.observe.record(
                "vala.datasets.tickets",
                {
                    "ticket_id": f"T-{index}",
                    "question": asked,
                    "answer": answer,
                    "refund": "refund" in asked,
                },
            )
            run.observe.eval({"answer": answer})
            judgment = run.observe.verify("no-refund-promise", {"answer": answer})
        served.append(Served(run.run_id, judgment.passed))
    state.shutdown()
    return served


def wait_for_verdicts(client: WyrdClient, desk: Desk, timeout: float) -> dict[str, tuple[int, int]]:
    """Wait until both Verifiers have judged every answer, then return their
    ``(passed, failed)`` counts by Verifier name.

    Raises:
        TimeoutError: when the verdicts are not all in within ``timeout`` seconds.
    """
    bifrost = Bifrost(client=client)
    names = {desk.uid(name): name for name in ("answer-quality", "no-refund-promise")}
    deadline = time.monotonic() + timeout
    while True:
        rows = bifrost.sql(
            "SELECT card_uid, verdict, COUNT(*) AS n FROM vala.verification.results "
            "WHERE subject_card_uid = $1 GROUP BY card_uid, verdict",
            [desk.uid("support-agent")],
            model=VerdictCount,
        )
        verdicts: dict[str, tuple[int, int]] = {}
        for row in rows:
            if name := names.get(row.card_uid):
                passed, failed = verdicts.get(name, (0, 0))
                verdicts[name] = (
                    (passed + row.n, failed)
                    if row.verdict == "passed"
                    else (passed, failed + row.n)
                )
        if all(sum(verdicts.get(name, (0, 0))) >= REQUESTS for name in names.values()):
            return verdicts
        if time.monotonic() >= deadline:
            raise TimeoutError(f"verdicts still incomplete after {timeout} s: {verdicts}")
        time.sleep(1)


def explain(client: WyrdClient, run_id: str) -> dict[str, Any]:
    """Join every piece of evidence of Run ``run_id`` — its observation,
    ticket, request span, gateway call, and both verdicts — through MCP
    ``bifrost.query`` with a fresh access token, as column name to value.

    Raises:
        ValueError: for a malformed Run ID.
        RuntimeError: when MCP or the tool refuses the call, or the Run has no
            complete joined row.
    """
    if not re.fullmatch(r"[0-9a-fA-F-]+", run_id):
        raise ValueError(f"malformed Run ID {run_id}")
    sql = (
        "SELECT o.run_id, o.trace_id, t.ticket_id, t.answer, s.name AS span, "
        "g.call_id, g.card_uid AS call_card_uid, c.verdict AS continuous, r.verdict AS realtime "
        "FROM vala.eval.observations o "
        "JOIN vala.datasets.tickets t ON t.run_id = o.run_id "
        "JOIN vala.traces.spans s ON s.trace_id = o.trace_id AND s.run_id = o.run_id "
        "JOIN vala.gateway.calls g ON g.run_id = o.run_id AND g.card_uid = o.card_uid "
        "JOIN vala.verification.results c ON c.run_id = o.run_id AND c.binding_id IS NOT NULL "
        "JOIN vala.verification.results r ON r.run_id = o.run_id AND r.binding_id IS NULL "
        f"WHERE o.run_id = '{run_id}' AND s.name = 'support-desk.request'"
    )
    body = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "bifrost.query",
            "arguments": {"sql": sql},
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": MCP_VERSION,
                "io.modelcontextprotocol/clientInfo": {"name": "support-desk", "version": "1.0.0"},
                "io.modelcontextprotocol/clientCapabilities": {},
            },
        },
    }
    request = urllib.request.Request(
        f"{client.server_url}/mcp",
        data=json.dumps(body).encode(),
        headers={
            "content-type": "application/json",
            "accept": "application/json, text/event-stream",
            "x-wyrd-access-token": f"Bearer {client.access_token()}",
            "mcp-protocol-version": MCP_VERSION,
            "mcp-method": "tools/call",
            "mcp-name": "bifrost.query",
        },
    )
    try:
        with urllib.request.urlopen(request) as response:
            reply = response.read().decode()
    except urllib.error.HTTPError as refused:
        raise RuntimeError(
            f"MCP refused the call with {refused.code}: {refused.read().decode()}"
        ) from refused
    message = next(
        json.loads(line.removeprefix("data:"))
        for line in reply.splitlines()
        if line.startswith("data:")
    )
    result = message["result"]
    content = result["structuredContent"]
    if result.get("isError"):
        raise RuntimeError(f"bifrost.query failed: {content}")
    if not content["rows"]:
        raise RuntimeError(f"Run {run_id} has no joined evidence")
    return {
        column["name"]: value
        for column, value in zip(content["columns"], content["rows"][0], strict=True)
    }


def main() -> None:
    """Deploy, serve, and print the verdicts and one passing and one failing explanation."""
    client = WyrdClient()
    with tempfile.TemporaryDirectory() as bundle:
        desk = deploy(client, Path(bundle) / "bundle")
        served = serve(desk)
        print(wait_for_verdicts(client, desk, timeout=300))
        for passed in (True, False):
            run = next(request for request in served if request.passed is passed)
            print(explain(client, run.run_id))


if __name__ == "__main__":
    main()
