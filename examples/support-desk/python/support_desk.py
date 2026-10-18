"""The support desk: an Agent-backed Service that answers customer questions,
records each ticket, and checks every answer against its refund policy with
the ``support/policy-check`` Verifier.

``deploy`` registers the checked-in Cards under ``service/``, which ensures the
declared ``vala.datasets.tickets`` table, turns on metadata capture, and checks
the Agent's model is deployed. ``serve`` answers every question in its own
Agent Run with ``answer``, the application's own gateway call, and submits each
answer for a background policy check.
``wait_for_verdicts`` resumes each check from its execution ID alone and waits
for its result, ``check`` judges one answer immediately, and ``explain`` joins
one Run's evidence over MCP.

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
from typing import TYPE_CHECKING, Any, cast

import openai
from opentelemetry import trace
from wyrd import verify
from wyrd.cards import Cards, DeclarationReceipt
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.observe import Run
from wyrd.state import WyrdState

if TYPE_CHECKING:
    from openai.types.chat import ChatCompletionMessageParam

REQUESTS = 100
"""How many requests ``serve`` answers; every tenth asks for a refund."""

SERVICE = Path(__file__).resolve().parents[1] / "service"
"""The checked-in Service Cards."""

MCP_VERSION = "2026-07-28"
"""The MCP protocol revision Wyrd serves."""


@dataclass
class Desk:
    """The deployed support desk: its registration, loaded Card graph, and gateway client."""

    receipt: DeclarationReceipt
    state: WyrdState
    gateway: openai.OpenAI

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
    execution_id: str
    """The background policy check of the request's answer."""


def policy_check(answer: str) -> dict[str, Any]:
    """The ``policy-check`` inputs for ``answer``: the answer and the checked-in policy PDF."""
    return {"answer": answer, "policy": verify.File(SERVICE / "refund-policy.pdf")}


def gateway_client(client: WyrdClient) -> openai.OpenAI:
    """An OpenAI client for the Wyrd gateway that authenticates as ``client``.

    ``http_auth`` replaces the placeholder key with a fresh Wyrd access token on
    every request, so the application never holds a provider key.
    """
    return openai.OpenAI(
        base_url=f"{client.server_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=client.http_auth),
        max_retries=0,
    )


def answer(gateway: openai.OpenAI, state: WyrdState, run: Run, asked: str) -> str:
    """Answer ``asked`` with the support Agent's Prompt through its gateway deployment.

    The application makes the model call itself. The ``wyrd-run-id`` and
    ``wyrd-card-uid`` headers tie the gateway's record of the call to ``run``
    and the Agent Card; the gateway never forwards them upstream.

    Raises:
        openai.APIStatusError: the gateway's refusal, with Wyrd's stable code
            in ``code``, such as ``WYRD_PERMISSION_403_DENIED_RBAC``.
    """
    deployment = state.card("agent").spec["execution_route"]["deployment"]
    prompt = state.agent("agent").prompt().prompt
    headers = {"wyrd-run-id": run.run_id, "wyrd-card-uid": str(state.card_ref("agent").uid)}
    completion = gateway.chat.completions.create(
        model=f"{deployment}/{prompt.model}",
        messages=cast("list[ChatCompletionMessageParam]", prompt.render(question=asked).messages),
        extra_headers=headers,
    )
    return completion.choices[0].message.content or ""


def deploy(client: WyrdClient, bundle: Path) -> Desk:
    """Register and hydrate the Service into ``bundle``, capture gateway call
    metadata, and check the Agent's model is deployed.

    Raises:
        WyrdError: the registration refusal, including
            ``WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`` for an incompatible
            existing ``tickets`` table.
        LookupError: naming the exact gateway deployment to configure when
            the Agent's gateway deployment does not exist.
    """
    cards = Cards(client)
    receipt = cards.declare_from_path(SERVICE / "support-desk.yaml")
    state = WyrdState.from_path(cards.hydrate(receipt.root, bundle).destination, client)
    gateway = Gateway(client)
    gateway.put_capture_policy({"mode": "metadata", "payload_fields": []})
    name = state.card("agent").spec["execution_route"].get("deployment")
    if name and not any(d["name"] == name for d in gateway.deployments()):
        route = f"{name}/{state.card('prompt').spec['model']}"
        raise LookupError(
            f"the support Agent's model {route} is not deployed; configure a gateway "
            f"provider deployment for {route} (PUT /v1/admin/gateway/provider-deployments/{{name}})"
        )
    return Desk(receipt, state, gateway_client(client))


def question(index: int) -> str:
    """The customer question of request ``index``."""
    if index % 10 == 0:
        return f"Can I get a refund for order {index}?"
    return f"Where is order {index}?"


def serve(desk: Desk) -> list[Served]:
    """Answer ``REQUESTS`` questions, each in its own Agent Run inside a
    ``support-desk.request`` span: ``answer`` through the gateway, record
    the ticket, and submit the answer for a background ``policy-check`` without
    waiting for it. Telemetry and tickets are flushed on return.
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
            reply = answer(desk.gateway, state, run, asked)
            run.observe.record(
                "vala.datasets.tickets",
                {
                    "ticket_id": f"T-{index}",
                    "question": asked,
                    "answer": reply,
                    "refund": "refund" in asked,
                },
            )
            execution = run.observe.submit("policy-check", **policy_check(reply))
        if not isinstance(execution, verify.Execution):
            raise TypeError("policy-check judges Run evidence, not a window")
        served.append(Served(run.run_id, execution.id))
    state.shutdown()
    return served


def check(desk: Desk, answer: str) -> verify.VerificationResult:
    """Judge ``answer`` with ``policy-check`` now, in a fresh Agent Run.

    Raises:
        WyrdError: the local input refusal or the server's refusal of the execution.
    """
    return desk.state.run("agent").observe.verify("policy-check", **policy_check(answer))


def wait_for_verdicts(
    client: WyrdClient, served: list[Served], timeout: float
) -> list[verify.VerificationResult]:
    """Resume every served request's background check from its execution ID
    alone, as another process would, and wait for its result, in ``served`` order.

    Raises:
        WyrdError: the execution's own error, or
            ``WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`` when the results are not
            all in within ``timeout`` seconds.
    """
    deadline = time.monotonic() + timeout
    return [
        verify.Execution.connect(request.execution_id, client).wait(
            max(deadline - time.monotonic(), 0.0)
        )
        for request in served
    ]


def explain(client: WyrdClient, run_id: str) -> dict[str, Any]:
    """Join every piece of evidence of Run ``run_id`` — its ticket, request
    span, gateway call, and policy-check verdict — through MCP
    ``bifrost.query`` with a fresh access token, as column name to value.

    Raises:
        ValueError: for a malformed Run ID.
        RuntimeError: when MCP or the tool refuses the call, or the Run has no
            complete joined row.
    """
    if not re.fullmatch(r"[0-9a-fA-F-]+", run_id):
        raise ValueError(f"malformed Run ID {run_id}")
    sql = (
        "SELECT t.run_id, s.trace_id, t.ticket_id, t.answer, s.name AS span, "
        "g.call_id, g.card_uid AS call_card_uid, r.verdict "
        "FROM vala.datasets.tickets t "
        "JOIN vala.traces.spans s ON s.run_id = t.run_id "
        "JOIN vala.gateway.calls g ON g.run_id = t.run_id "
        "JOIN vala.verification.results r ON r.run_id = t.run_id "
        f"WHERE t.run_id = '{run_id}' AND s.name = 'support-desk.request' "
        "ORDER BY g.call_id LIMIT 1"
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
    """Deploy, judge one refund promise now, serve, wait for every background
    check, resume the first failure by its ID alone, and explain one passing
    and one failing request.
    """
    client = WyrdClient()
    with tempfile.TemporaryDirectory() as bundle:
        desk = deploy(client, Path(bundle) / "bundle")
        print(f"deployed support-agent {desk.uid('support-agent')}")
        print(f"immediate check: {check(desk, 'You have a guaranteed refund.').verdict}")
        served = serve(desk)
        results = wait_for_verdicts(client, served, timeout=300)
        paired = list(zip(served, results, strict=True))
        failed = next(
            (request for request, result in paired if result.verdict == "failed"),
            None,
        )
        if failed is not None:
            # Any process can resume a check from its ID with the ambient client.
            resumed = verify.Execution.connect(failed.execution_id).wait(30)
            for requirement in resumed.requirements:
                print(f"{requirement['id']}: {requirement['outcome']} ({requirement['reason']})")
        for verdict in ("passed", "failed"):
            request = next(
                (request for request, result in paired if result.verdict == verdict),
                None,
            )
            if request is not None:
                print(explain(client, request.run_id))


if __name__ == "__main__":
    main()
