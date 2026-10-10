"""A developer runs Wyrd locally with only the administrator key ``wyrd setup`` printed.

They register and hydrate an assistant, invoke a model through the Gateway with
the stock OpenAI client, observe and verify a Run, export its spans through
``state.start_telemetry``, and query the evidence back. No key is issued and
nothing is flushed on the server's behalf. ``work`` is that whole workflow for
one client, so the signed-in story proves the same steps for a saved login.
"""

from __future__ import annotations

import threading
from collections.abc import Iterator
from dataclasses import dataclass
from http.server import ThreadingHTTPServer
from pathlib import Path

import openai
import pytest
from opentelemetry import trace
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.errors import WyrdError
from wyrd.gateway import Gateway, GatewayAuth
from wyrd.observe import Run
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

from .gateway.support import Upstream
from .support import Receiver, download, hydrated, register

pytestmark = pytest.mark.integration


class Attributed(BaseModel):
    """One evidence row: the Card it is attributed to."""

    card_uid: str | None


@dataclass
class Worked:
    """What ``work`` leaves running for a caller to keep using."""

    state: WyrdState
    agent_uid: str


def local_server(
    human_sso: bool = False, access_ttl_seconds: int | None = None
) -> Iterator[WyrdTestServer]:
    """A server that verifies in real time and routes the Gateway to a local upstream answering ``hi``."""
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("LocalUpstream", (Upstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        with WyrdTestServer(
            mutate_env=False,
            verification_runtime=True,
            provider_base_url=recorded.url,
            human_sso=human_sso,
            access_ttl_seconds=access_ttl_seconds,
        ) as server:
            yield server
    finally:
        upstream.shutdown()


def work(client: WyrdClient, bundle_home: Path) -> Worked:
    """Run the whole local workflow as ``client`` and assert each step."""
    configure_gateway(client)
    cards = Cards(client)
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(download(cards, "assistant", bundle_home), client)
    state.start_bifrost()

    assert invoke(client) == "hi"

    run = state.run()
    run.for_card("model").observe.drift({"latency": 12.5})
    judgment = run.for_card("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert (judgment.passed, judgment.kind) == (True, "eval_assertion")

    state.start_telemetry()
    state.start_telemetry()
    export(run.for_card("agent"))
    # Shutdown flushes the exported span.
    state.shutdown()

    bifrost = Bifrost(client=client)
    for table, alias in (("vala.drift.observations", "model"), ("vala.traces.spans", "agent")):
        rows = bifrost.sql(
            f"SELECT card_uid FROM {table} WHERE run_id = $1", [run.run_id], model=Attributed
        )
        assert rows == [Attributed(card_uid=str(state.card_ref(alias).uid))], table
    return Worked(state, str(state.card_ref("agent").uid))


def export(run: Run) -> None:
    """Start one ``answer`` span inside ``run``'s scope, which attributes it to that Run and Card."""
    with run, trace.get_tracer("wyrd.tests.local").start_as_current_span("answer"):
        pass


def configure_gateway(client: WyrdClient) -> None:
    """Write the ``openai-key`` credential and deploy ``gpt-4o`` on it as ``client``."""
    cli.put_provider_credential(
        {
            "name": "openai-key",
            "provider": "openai",
            "source": {"managed_secret": {"secret": "sk-local-upstream"}},
        },
        client=client,
    )
    Gateway(client).put_deployment(
        {
            "name": "gpt-4o",
            "model": {"provider": "openai", "model": "gpt-4o"},
            "adapter": "openai",
            "auth": {"bearer": {"credential": "openai-key"}},
            "capabilities": ["chat_completions"],
            "routing_weight": 1,
        }
    )


def invoke(client: WyrdClient) -> str:
    """Send one Chat Completions turn through the Gateway with the stock OpenAI client."""
    gateway = openai.OpenAI(
        base_url=f"{client.server_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=GatewayAuth(client)),
        max_retries=0,
    )
    completion = gateway.chat.completions.create(
        model="openai/gpt-4o", messages=[{"role": "user", "content": "hi"}]
    )
    return completion.choices[0].message.content or ""


@pytest.fixture(scope="module")
def server() -> Iterator[WyrdTestServer]:
    """The local deployment."""
    yield from local_server()


@pytest.mark.usefixtures("fresh_tracer_provider")
def test_admin_key_completes_the_local_workflow(server: WyrdTestServer, tmp_path: Path) -> None:
    client = WyrdClient(
        server_url=server.base_url, credential=server.tenant_admin_key(), grpc_url=server.grpc_url
    )

    work(client, tmp_path)


@pytest.mark.usefixtures("fresh_tracer_provider")
def test_start_telemetry_refuses_the_applications_own_provider(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    from opentelemetry.sdk.trace import TracerProvider

    client = WyrdClient(
        server_url=server.base_url, credential=server.tenant_admin_key(), grpc_url=server.grpc_url
    )
    cards = Cards(client)
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(download(cards, "assistant", tmp_path), client)
    own = TracerProvider()
    trace.set_tracer_provider(own)

    with pytest.raises(WyrdError) as refused:
        state.start_telemetry()

    assert refused.value.code == "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS"
    assert trace.get_tracer_provider() is own
