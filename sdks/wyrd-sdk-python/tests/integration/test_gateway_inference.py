"""Callers reach the Wyrd gateway with an access token or a Card-scoped key (AC-051, AC-053)."""

from __future__ import annotations

import secrets
from pathlib import Path

import openai
import pytest
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

from .gateway.support import PROVIDER_KEY, calls, deploy
from .support import (
    FIXTURES,
    Receiver,
    ancestry,
    client_of,
    download,
    register,
    timeline,
)

pytestmark = pytest.mark.integration

ASK = FIXTURES / "cards/gateway_inference/ask-agent.yaml"


def model_calls(upstream: Receiver) -> int:
    """Chat Completions requests the gateway dispatched to the provider upstream."""
    return len(upstream.to("/v1/chat/completions"))


def openai_client(caller: WyrdClient) -> openai.OpenAI:
    """An OpenAI client pointed at the gateway with ``caller``'s Wyrd access token."""
    return openai.OpenAI(
        base_url=f"{caller.server_url}/v1", api_key=caller.access_token(), max_retries=0
    )


def answer(caller: WyrdClient, state: WyrdState, alias: str, deployment: str) -> str:
    """Send Agent ``alias``'s rendered Prompt to ``deployment`` as the application's own call."""
    prompt = state.agent(alias).prompt().prompt
    completion = openai_client(caller).chat.completions.create(
        model=f"{deployment}/{prompt.model}", messages=prompt.render(question="hi").messages
    )
    return completion.choices[0].message.content or ""


@pytest.fixture
def server(gateway_server: tuple[WyrdTestServer, Receiver]) -> WyrdTestServer:
    """A gateway server with the ``openai`` deployment capturing call metadata."""
    server, _ = gateway_server
    deploy(server, "openai")
    Gateway(client_of(server)).put_capture_policy({"mode": "metadata", "payload_fields": []})
    return server


@pytest.fixture
def upstream(server: WyrdTestServer, gateway_server: tuple[WyrdTestServer, Receiver]) -> Receiver:
    """What the deployed gateway's provider upstream received."""
    return gateway_server[1]


@pytest.fixture
def admin(server: WyrdTestServer) -> WyrdClient:
    """The gateway server's administrator."""
    return client_of(server)


@pytest.fixture
def ask(admin: WyrdClient) -> CardRef:
    """The registered ``ask-agent`` Agent."""
    return Cards(admin).register_from_path(ASK).root


def test_openai_client_calls_the_gateway_with_an_access_token(
    admin: WyrdClient, upstream: Receiver
) -> None:
    completion = openai_client(admin).chat.completions.create(
        model="openai/gpt-4o", messages=[{"role": "user", "content": "hi"}]
    )

    assert completion.choices[0].message.content == "hi"
    assert model_calls(upstream) == 1
    assert upstream.deliveries[0].headers["authorization"] == f"Bearer {PROVIDER_KEY}"


def test_gateway_span_joins_the_callers_trace(server: WyrdTestServer, admin: WyrdClient) -> None:
    trace_id, span_id = secrets.token_hex(16), secrets.token_hex(8)
    completion = openai_client(admin).chat.completions.create(
        model="openai/gpt-4o",
        messages=[{"role": "user", "content": "hi"}],
        extra_headers={"traceparent": f"00-{trace_id}-{span_id}-01", "tracestate": "wyrd=journey"},
    )

    assert completion.choices[0].message.content == "hi"
    spans = timeline(server, trace_id)
    attempts = [span for span in spans if span["scope"]["name"] == "wyrd.gateway"]
    assert len(attempts) == 1, spans
    assert ancestry(spans, attempts[0])[-1].get("parent_span_id") == span_id
    assert attempts[0].get("trace_state") == "wyrd=journey"


def test_malformed_traceparent_is_ignored_by_the_gateway(
    server: WyrdTestServer, admin: WyrdClient
) -> None:
    trace_id = secrets.token_hex(16)
    completion = openai_client(admin).chat.completions.create(
        model="openai/gpt-4o",
        messages=[{"role": "user", "content": "hi"}],
        extra_headers={"traceparent": f"00-{trace_id.upper()}-00f067aa0ba902b7-01"},
    )

    assert completion.choices[0].message.content == "hi"
    assert timeline(server, trace_id) == []


def test_gateway_call_records_a_lone_run_or_card(server: WyrdTestServer, admin: WyrdClient) -> None:
    card_uid = str(
        register(Cards(admin), "cards/gateway_inference/ask-agent.yaml")["ask-agent"].uid
    )
    issued = cli.issue_key(
        kind="Agent", name="ask-agent", version="1.0.0", space="default", client=admin
    )
    agent = openai_client(client_of(server, issued.key))

    for headers in ({"wyrd-run-id": "run-alone"}, {"wyrd-card-uid": card_uid}):
        completion = agent.chat.completions.create(
            model="openai/gpt-4o",
            messages=[{"role": "user", "content": "hi"}],
            extra_headers=headers,
        )
        assert completion.choices[0].message.content == "hi"

    server.flush_bifrost()
    calls = Bifrost(client=admin).sql(
        "SELECT run_id, card_uid FROM vala.gateway.calls WHERE sub_principal_id = $1 ORDER BY run_id",
        [str(issued.principal_id)],
    )
    assert calls.to_arrow().to_pylist() == [
        {"run_id": "run-alone", "card_uid": None},
        {"run_id": None, "card_uid": card_uid},
    ]


def test_caller_without_gateway_invoke_is_refused(
    server: WyrdTestServer, upstream: Receiver
) -> None:
    reader = client_of(server, server.scoped_api_key("gateway_reader", ["gateway:read"]))

    with pytest.raises(openai.PermissionDeniedError) as refused:
        openai_client(reader).chat.completions.create(
            model="openai/gpt-4o", messages=[{"role": "user", "content": "hi"}]
        )
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert model_calls(upstream) == 0


def test_cli_issues_a_card_scoped_key_and_writes_a_provider_credential(
    admin: WyrdClient, ask: CardRef
) -> None:
    issued = cli.issue_key(
        kind="Agent", name="ask-agent", version="1.0.0", space="default", client=admin
    )
    view = cli.put_provider_credential(
        {
            "name": "managed-openai-key",
            "provider": "openai",
            "source": {"managed_secret": {"secret": "sk-managed"}},
        },
        client=admin,
    )

    assert str(issued.card_ref) == "default/Agent/ask-agent@1.0.0"
    assert issued.key.startswith(issued.prefix)
    assert (view["name"], view["provider"], view["state"]) == (
        "managed-openai-key",
        "openai",
        "active",
    )
    assert "sk-managed" not in str(view)


def test_agent_prompt_routes(
    server: WyrdTestServer, admin: WyrdClient, upstream: Receiver, tmp_path: Path
) -> None:
    deploy(server, "openai", name="openai-backup")
    deploy(server, "openai", name="openai-granted", access="granted")
    register(Cards(admin), "cards/gateway_inference/ask-service.yaml")
    bundle = download(Cards(admin), "ask-service", tmp_path)
    key = cli.issue_key(
        kind="Service", name="ask-service", version="1.0.0", space="default", client=admin
    )
    caller = client_of(server, key.key)
    state = WyrdState.from_path(bundle, caller)

    for alias in ("agent", "body-agent"):
        declared = state.card(alias).spec["execution_route"]["deployment"]
        assert declared == "openai"
        assert answer(caller, state, alias, declared) == "hi"
        assert answer(caller, state, alias, "openai-backup") == "hi"

    assert [delivery.body["model"] for delivery in upstream.to("/v1/chat/completions")] == [
        "gpt-4o"
    ] * 4
    assert calls(
        server,
        f"sub_principal_id = '{key.principal_id}'",
        "requested_model.deployment AS deployment",
    ) == [{"deployment": deployment} for deployment in ("openai", "openai-backup") * 2]

    with pytest.raises(openai.PermissionDeniedError) as refused:
        answer(caller, state, "agent", "openai-granted")
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert model_calls(upstream) == 4, "the refusal precedes upstream IO"
