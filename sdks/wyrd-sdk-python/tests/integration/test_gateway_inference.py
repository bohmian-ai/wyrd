"""Callers reach the Wyrd gateway with an access token, a Card-scoped key, or a loaded Workflow (AC-051, AC-053)."""

from __future__ import annotations

import openai
import pytest
from wyrd.agent import Workflow
from wyrd.cards import CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer, cli

from .gateway.support import PROVIDER_KEY, deploy
from .support import FIXTURES, Receiver, client_of

pytestmark = pytest.mark.integration

ASK = FIXTURES / "cards/gateway_inference/ask.yaml"

EXAMPLE = FIXTURES.parent / "examples/workflows/code-review"
"""The code-review example Workflow directory; its Prompts call ``gpt-5-5`` through the Wyrd gateway."""

EXAMPLE_INPUT = {"code": "diff"}
"""A code-review input; every Prompt renders it into the request the upstream answers."""


def model_calls(upstream: Receiver) -> int:
    """Chat Completions requests the gateway dispatched to the provider upstream."""
    return len(upstream.to("/v1/chat/completions"))


def openai_client(caller: WyrdClient) -> openai.OpenAI:
    """An OpenAI client pointed at the gateway with ``caller``'s Wyrd access token."""
    return openai.OpenAI(
        base_url=f"{caller.server_url}/v1", api_key=caller.access_token(), max_retries=0
    )


@pytest.fixture
def server(gateway_server: tuple[WyrdTestServer, Receiver]) -> WyrdTestServer:
    """A gateway server with OpenAI chat deployments of ``gpt-4o`` and ``gpt-5-5``."""
    server, _ = gateway_server
    deploy(server, "openai", "gpt-4o", ["chat_completions"])
    deploy(server, "openai", "gpt-5-5", ["chat_completions"])
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
    """The registered ``ask`` Workflow."""
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


def test_loaded_workflow_calls_the_gateway_through_its_loading_client(
    admin: WyrdClient, upstream: Receiver, ask: CardRef
) -> None:
    workflow = Cards(admin).workflow.load(ask)

    run = workflow.run({"question": "hi"})

    assert (run.status, run.outputs) == ("succeeded", {"answer": "hi"})
    assert model_calls(upstream) == 1
    assert upstream.deliveries[0].headers["authorization"] == f"Bearer {PROVIDER_KEY}"


def test_example_workflow_runs_through_the_wyrd_gateway(
    admin: WyrdClient, upstream: Receiver
) -> None:
    example = Workflow.from_path(EXAMPLE / "workflow.yaml", admin)

    run = example.run(EXAMPLE_INPUT)

    assert (run.status, run.outputs) == ("succeeded", {"review": "hi"})
    assert model_calls(upstream) == 3


def test_applying_a_workflow_calls_no_model(admin: WyrdClient, upstream: Receiver) -> None:
    applied = cli.apply(EXAMPLE, client=admin)

    assert str(applied.root).startswith("engineering/Workflow/code-review@1.0.0#")
    assert upstream.deliveries == []


def test_registered_example_runs_through_the_gateway(admin: WyrdClient, upstream: Receiver) -> None:
    cli.apply(EXAMPLE, client=admin)
    registered = Cards(admin).workflow.load(
        space="engineering", name="code-review", version="1.0.0"
    )

    run = registered.run(EXAMPLE_INPUT)

    assert (run.status, run.outputs) == ("succeeded", {"review": "hi"})
    assert model_calls(upstream) == 3
