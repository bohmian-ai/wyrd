"""The unmodified OpenAI client calls every backend through the gateway's compatible surface.

The client holds a normal Wyrd access token, the server's in-process gateway
reaches the shared recording upstream, and each call is recorded in
``vala.gateway.calls``.
"""

from __future__ import annotations

from typing import Any

import openai
import pytest
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

from ..support import Receiver, client_of
from .support import (
    IMAGE_BYTES,
    IMAGE_DIGEST,
    PROVIDER_KEY,
    access_token,
    assert_upstream_credentials,
    calls,
    deploy,
    usage,
)

pytestmark = pytest.mark.integration

MESSAGES: list[Any] = [{"role": "user", "content": "hi"}]
VERTEX = "/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro"


def openai_client(server: WyrdTestServer, token: str) -> openai.OpenAI:
    """The official OpenAI client pointed at the server's compatible base URL."""
    return openai.OpenAI(base_url=f"{server.base_url}/v1", api_key=token, max_retries=0)


@pytest.fixture
def backends(
    gateway_server: tuple[WyrdTestServer, Receiver],
) -> tuple[WyrdTestServer, Receiver]:
    """A server with one chat model on each built-in provider and metadata capture."""
    server, upstream = gateway_server
    deploy(server, "openai", "gpt-4o", ["chat_completions"])
    deploy(server, "anthropic", "claude-sonnet-5", ["chat_completions"])
    deploy(server, "gemini", "gemini-2.5-flash", ["chat_completions"])
    deploy(
        server,
        "vertex",
        "gemini-2.5-pro",
        ["chat_completions"],
        adapter={"vertex": {"project": "acme", "location": "us-central1"}},
    )
    Gateway(client_of(server)).put_capture_policy({"mode": "metadata", "payload_fields": []})
    return server, upstream


@pytest.mark.parametrize(
    ("model", "header", "paths", "expected_usage"),
    [
        (
            "openai/gpt-4o",
            ("authorization", f"Bearer {PROVIDER_KEY}"),
            ["/v1/chat/completions", "/v1/chat/completions"],
            {("input_tokens", "11"), ("output_tokens", "4")},
        ),
        (
            "anthropic/claude-sonnet-5",
            ("x-api-key", PROVIDER_KEY),
            ["/v1/messages", "/v1/messages"],
            {("input_tokens", "5"), ("output_tokens", "3")},
        ),
        (
            "gemini/gemini-2.5-flash",
            ("x-goog-api-key", PROVIDER_KEY),
            [
                "/v1beta/models/gemini-2.5-flash:generateContent",
                "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse",
            ],
            {("input_tokens", "7"), ("output_tokens", "2")},
        ),
        (
            "vertex/gemini-2.5-pro",
            ("authorization", f"Bearer {PROVIDER_KEY}"),
            [f"{VERTEX}:generateContent", f"{VERTEX}:streamGenerateContent?alt=sse"],
            {("input_tokens", "7"), ("output_tokens", "2")},
        ),
    ],
)
def test_openai_client_reaches_the_backend_through_the_server(
    backends: tuple[WyrdTestServer, Receiver],
    model: str,
    header: tuple[str, str],
    paths: list[str],
    expected_usage: set[tuple[str, str]],
) -> None:
    server, upstream = backends
    token = server.access_token()
    client = openai_client(server, token)

    completion = client.chat.completions.create(
        model=model, messages=MESSAGES, max_completion_tokens=16
    )
    chunks = list(
        client.chat.completions.create(
            model=model,
            messages=MESSAGES,
            max_completion_tokens=16,
            stream=True,
            stream_options={"include_usage": True},
        )
    )
    rows = calls(
        server,
        "ingress_dialect = 'openai_chat_completions'",
        "caller_principal_id, usage, streaming, outcome",
    )

    assert (completion.choices[0].message.content, completion.choices[0].finish_reason) == (
        "hi",
        "stop",
    )
    assert "".join(c.choices[0].delta.content or "" for c in chunks if c.choices) == "hi"
    assert chunks[-1].usage is not None
    assert [delivery.path for delivery in upstream.deliveries] == paths
    assert_upstream_credentials(upstream, *header, token)
    assert len({row["caller_principal_id"] for row in rows} - {None}) == 1
    assert sorted(row["streaming"] for row in rows) == [False, True]
    assert {row["outcome"] for row in rows} == {"succeeded"}
    assert usage(rows) == expected_usage


def test_openai_client_receives_stable_refusals_without_dispatch_or_leakage(
    backends: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = backends
    token = server.access_token()
    client = openai_client(server, token)
    reader = access_token(server, server.bootstrap_service(["reader"], name="openai-reader"))

    with pytest.raises(openai.PermissionDeniedError) as denied:
        openai_client(server, reader).chat.completions.create(
            model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
        )
    with pytest.raises(openai.BadRequestError) as foreign:
        openai_client(server, "sk-not-a-wyrd-token").chat.completions.create(
            model="openai/gpt-4o", messages=MESSAGES
        )
    with pytest.raises(openai.APIStatusError) as unknown:
        client.chat.completions.create(
            model="openai/gpt-missing", messages=MESSAGES, max_completion_tokens=16
        )
    with pytest.raises(openai.APIStatusError) as incapable:
        client.embeddings.create(model="anthropic/claude-sonnet-5", input="hi")

    refusals = [denied.value, foreign.value, unknown.value, incapable.value]
    assert [refusal.code for refusal in refusals] == [
        "WYRD_PERMISSION_403_DENIED_RBAC",
        "WYRD_AUTH_400_BAD_TOKEN_FORMAT",
        "WYRD_GATEWAY_404_MODEL_UNAVAILABLE",
        "WYRD_GATEWAY_404_MODEL_UNAVAILABLE",
    ]
    for refusal in refusals:
        assert refusal.response.headers.get("wyrd-request-id")
        assert PROVIDER_KEY not in refusal.response.text
        assert token not in refusal.response.text
    assert upstream.deliveries == []


def test_server_keeps_serving_after_a_cancelled_stream(
    backends: tuple[WyrdTestServer, Receiver],
) -> None:
    server, _ = backends
    client = openai_client(server, server.access_token())

    with client.chat.completions.create(
        model="openai/gpt-4o", messages=MESSAGES, stream=True
    ) as stream:
        assert next(iter(stream)).choices[0].delta.content == "h"
    healthy = client.chat.completions.create(
        model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
    )

    assert healthy.choices[0].message.content == "hi"
    assert PROVIDER_KEY not in healthy.model_dump_json()


def model_access(provider: str, model: str) -> dict[str, object]:
    """A permission to invoke exactly one gateway model."""
    return {
        "resource": "gateway",
        "action": "invoke",
        "scope": {"gateway": {"model": {"provider": provider, "model": model}}},
    }


def test_two_users_have_distinct_model_access(backends: tuple[WyrdTestServer, Receiver]) -> None:
    server, upstream = backends
    ada = access_token(
        server, server.scoped_api_key("gateway_ada", [model_access("openai", "gpt-4o")])
    )
    bea = access_token(
        server, server.scoped_api_key("gateway_bea", [model_access("anthropic", "claude-sonnet-5")])
    )

    for caller, allowed, denied in (
        (ada, "openai/gpt-4o", "anthropic/claude-sonnet-5"),
        (bea, "anthropic/claude-sonnet-5", "openai/gpt-4o"),
    ):
        answer = openai_client(server, caller).chat.completions.create(
            model=allowed, messages=MESSAGES, max_completion_tokens=16
        )
        assert answer.choices[0].message.content == "hi"
        with pytest.raises(openai.PermissionDeniedError) as refused:
            openai_client(server, caller).chat.completions.create(
                model=denied, messages=MESSAGES, max_completion_tokens=16
            )
        assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert [delivery.path for delivery in upstream.deliveries] == [
        "/v1/chat/completions",
        "/v1/messages",
    ]


def test_each_users_usage_is_recorded_under_their_principal(
    backends: tuple[WyrdTestServer, Receiver],
) -> None:
    server, _ = backends
    ada = access_token(
        server, server.scoped_api_key("gateway_ada", [model_access("openai", "gpt-4o")])
    )
    bea = access_token(
        server, server.scoped_api_key("gateway_bea", [model_access("anthropic", "claude-sonnet-5")])
    )
    openai_client(server, ada).chat.completions.create(
        model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
    )
    openai_client(server, bea).chat.completions.create(
        model="anthropic/claude-sonnet-5", messages=MESSAGES, max_completion_tokens=16
    )

    rows = calls(server, "outcome = 'succeeded'", "caller_principal_id, requested_model, usage")

    by_caller: dict[str, list[dict[str, Any]]] = {}
    for row in rows:
        by_caller.setdefault(row["caller_principal_id"], []).append(row)
    assert sorted(
        (rows[0]["requested_model"]["provider"], usage(rows)) for rows in by_caller.values()
    ) == [
        ("anthropic", {("input_tokens", "5"), ("output_tokens", "3")}),
        ("openai", {("input_tokens", "11"), ("output_tokens", "4")}),
    ]


def test_revoked_user_never_dispatches(backends: tuple[WyrdTestServer, Receiver]) -> None:
    server, upstream = backends
    key = server.scoped_api_key("gateway_ada", [model_access("openai", "gpt-4o")])
    server.revoke_scoped_role("gateway_ada")

    with pytest.raises(openai.PermissionDeniedError) as revoked:
        openai_client(server, access_token(server, key)).chat.completions.create(
            model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
        )
    assert revoked.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert upstream.deliveries == []


def test_embedding_and_image_evidence_reaches_bifrost(
    gateway_server: tuple[WyrdTestServer, Receiver],
) -> None:
    """Payload capture records the image by digest; fetching the bytes is proven in Rust."""
    server, upstream = gateway_server
    deploy(server, "openai", "text-embedding-3-small", ["embeddings"])
    deploy(server, "openai", "gpt-image-1", ["images"])
    Gateway(client_of(server)).put_capture_policy(
        {"mode": "payload", "payload_fields": ["response"]}
    )
    token = server.access_token()
    client = openai_client(server, token)

    embedding = client.embeddings.create(model="openai/text-embedding-3-small", input="hi")
    image = client.images.generate(model="openai/gpt-image-1", prompt="a rune")
    rows = calls(server, "outcome = 'succeeded'", "operation, usage, payload_object_refs")

    assert embedding.data[0].embedding == [0.25, -0.5, 0.75]
    assert embedding.usage.prompt_tokens == 6
    assert image.data is not None and image.data[0].b64_json is not None
    assert [delivery.path for delivery in upstream.deliveries] == [
        "/v1/embeddings",
        "/v1/images/generations",
    ]
    assert_upstream_credentials(upstream, "authorization", f"Bearer {PROVIDER_KEY}", token)
    by_operation = {row["operation"]: row for row in rows}
    assert usage([by_operation["embeddings"]]) == {("input_tokens", "6")}
    [ref] = by_operation["images"]["payload_object_refs"]
    assert (ref["digest"], ref["size_bytes"]) == (IMAGE_DIGEST, len(IMAGE_BYTES))
