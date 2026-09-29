"""Unmodified OpenAI client journeys through the Wyrd gateway's compatible surface.

Every journey drives the official ``openai`` client with a normal Wyrd access
token against a real ``wyrd-server`` whose in-process gateway reaches the
shared recording mock upstream. They prove the OpenAI-compatible route over
OpenAI, Anthropic, Gemini, and Vertex deployments; stable refusals without
dispatch or secret leakage; distinct model access for two callers with
revocation; and embedding and image evidence in Bifrost and tenant storage.
"""

from __future__ import annotations

import hashlib
from typing import Any

import httpx
import openai
import pytest
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

from .support import (
    IMAGE_BYTES,
    PROVIDER_KEY,
    Received,
    assert_upstream_credentials,
    call_rows,
    deploy,
    exchange,
    principal,
    usage,
)

MESSAGES: list[Any] = [{"role": "user", "content": "hi"}]

# Upstream credential header and expected token usage per deployment backend.
BACKENDS = {
    "openai/gpt-4o": ("authorization", {("input_tokens", "11"), ("output_tokens", "4")}),
    "anthropic/claude-sonnet-5": ("x-api-key", {("input_tokens", "5"), ("output_tokens", "3")}),
    "gemini/gemini-2.5-flash": (
        "x-goog-api-key",
        {("input_tokens", "7"), ("output_tokens", "2")},
    ),
    "vertex/gemini-2.5-pro": ("authorization", {("input_tokens", "7"), ("output_tokens", "2")}),
}


def _client(server: WyrdTestServer, token: str) -> openai.OpenAI:
    """Unmodified OpenAI client pointed at the server's compatible base URL."""
    return openai.OpenAI(base_url=f"{server.base_url}/v1", api_key=token, max_retries=0)


def _deploy_backends(server: WyrdTestServer) -> None:
    """Deploy one chat model on each built-in provider and enable metadata capture."""
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
    Gateway(server_url=server.base_url, credential=server.api_key).put_capture_policy(
        {"mode": "metadata", "payload_fields": []}
    )


def _model(row: dict[str, Any]) -> str:
    """``provider/model`` of a call row's requested model."""
    requested = row["requested_model"]
    return f"{requested['provider']}/{requested['model']}"


@pytest.mark.integration
def test_openai_client_reaches_every_backend_through_the_server(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    server, received = gateway_server
    _deploy_backends(server)
    token = server.access_token()
    client = _client(server, token)

    for model in BACKENDS:
        completion = client.chat.completions.create(
            model=model, messages=MESSAGES, max_completion_tokens=16
        )
        assert completion.choices[0].message.content == "hi", model
        assert completion.choices[0].finish_reason == "stop", model
        chunks = list(
            client.chat.completions.create(
                model=model,
                messages=MESSAGES,
                max_completion_tokens=16,
                stream=True,
                stream_options={"include_usage": True},
            )
        )
        text = "".join(c.choices[0].delta.content or "" for c in chunks if c.choices)
        assert text == "hi", model
        assert [c.choices[0].finish_reason for c in chunks if c.choices][-1] == "stop", model
        assert chunks[-1].usage is not None, model

    assert len(received) == 2 * len(BACKENDS)
    paths = [path for path, _ in received]
    assert paths[0:2] == ["/v1/chat/completions"] * 2
    assert paths[2:4] == ["/v1/messages"] * 2
    assert paths[4] == "/v1beta/models/gemini-2.5-flash:generateContent"
    assert paths[5] == "/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
    vertex = "/v1/projects/acme/locations/us-central1/publishers/google/models/gemini-2.5-pro"
    assert paths[6] == f"{vertex}:generateContent"
    assert paths[7] == f"{vertex}:streamGenerateContent?alt=sse"
    for index, (header, _) in enumerate(BACKENDS.values()):
        assert_upstream_credentials(received[2 * index : 2 * index + 2], header, token)

    rows = call_rows(
        server,
        server.access_token(),
        "ingress_dialect = 'openai_chat_completions'",
        2 * len(BACKENDS),
        "caller_principal_id, requested_model, usage, streaming, outcome",
    )
    assert {row["caller_principal_id"] for row in rows} == {principal(token)}
    assert all(row["outcome"] == "succeeded" for row in rows)
    for model, (_, expected) in BACKENDS.items():
        mine = [row for row in rows if _model(row) == model]
        assert sorted(row["streaming"] for row in mine) == [False, True], model
        assert usage(mine) == expected, model


@pytest.mark.integration
def test_openai_client_receives_stable_refusals_without_dispatch_or_leakage(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    server, received = gateway_server
    _deploy_backends(server)
    token = server.access_token()
    client = _client(server, token)
    refusals: list[openai.APIStatusError] = []

    reader = exchange(server, server.bootstrap_service(["reader"], name="openai-reader"))
    with pytest.raises(openai.PermissionDeniedError) as denied:
        _client(server, reader).chat.completions.create(
            model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
        )
    refusals.append(denied.value)
    missing = httpx.post(
        f"{server.base_url}/v1/chat/completions",
        json={"model": "openai/gpt-4o", "messages": MESSAGES},
    )
    with pytest.raises(openai.BadRequestError) as foreign:
        _client(server, "sk-not-a-wyrd-token").chat.completions.create(
            model="openai/gpt-4o", messages=MESSAGES
        )
    refusals.append(foreign.value)
    with pytest.raises(openai.APIStatusError) as unknown:
        client.chat.completions.create(
            model="openai/gpt-missing", messages=MESSAGES, max_completion_tokens=16
        )
    refusals.append(unknown.value)
    with pytest.raises(openai.APIStatusError) as incapable:
        client.embeddings.create(model="anthropic/claude-sonnet-5", input="hi")
    refusals.append(incapable.value)

    assert denied.value.body["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert missing.status_code == 401
    assert missing.json()["error"]["code"] == "WYRD_AUTH_401_UNAUTHENTICATED"
    assert foreign.value.body["code"] == "WYRD_AUTH_400_BAD_TOKEN_FORMAT"
    assert unknown.value.body["code"].startswith("WYRD_GATEWAY_"), unknown.value.body
    assert incapable.value.status_code == 404, incapable.value.body
    assert incapable.value.body["code"] == "WYRD_GATEWAY_404_MODEL_UNAVAILABLE"
    for response in [error.response for error in refusals] + [missing]:
        assert response.headers.get("wyrd-request-id"), response.text
        assert PROVIDER_KEY not in response.text
        assert token not in response.text
    assert received == [], "no refusal reaches a provider"

    with client.chat.completions.create(
        model="openai/gpt-4o", messages=MESSAGES, stream=True
    ) as stream:
        first = next(iter(stream))
        assert first.choices[0].delta.content == "h"
    healthy = client.chat.completions.create(
        model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
    )
    assert healthy.choices[0].message.content == "hi", "the server serves after a cancelled stream"
    assert PROVIDER_KEY not in healthy.model_dump_json()


@pytest.mark.integration
def test_two_users_have_distinct_model_access_and_traceable_usage(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    server, received = gateway_server
    _deploy_backends(server)

    def model_access(provider: str, model: str) -> dict[str, object]:
        return {
            "resource": "gateway",
            "action": "invoke",
            "scope": {"gateway": {"model": {"provider": provider, "model": model}}},
        }

    ada_key = server.scoped_api_key("gateway_ada", [model_access("openai", "gpt-4o")])
    ada = exchange(server, ada_key)
    bea = exchange(
        server,
        server.scoped_api_key("gateway_bea", [model_access("anthropic", "claude-sonnet-5")]),
    )

    for caller, allowed, denied in (
        (ada, "openai/gpt-4o", "anthropic/claude-sonnet-5"),
        (bea, "anthropic/claude-sonnet-5", "openai/gpt-4o"),
    ):
        answer = _client(server, caller).chat.completions.create(
            model=allowed, messages=MESSAGES, max_completion_tokens=16
        )
        assert answer.choices[0].message.content == "hi"
        with pytest.raises(openai.PermissionDeniedError) as refused:
            _client(server, caller).chat.completions.create(
                model=denied, messages=MESSAGES, max_completion_tokens=16
            )
        assert refused.value.body["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert [path for path, _ in received] == ["/v1/chat/completions", "/v1/messages"]

    openai_usage = _client(server, ada).chat.completions.create(
        model="openai/gpt-4o", messages=MESSAGES
    )
    assert openai_usage.usage is not None
    assert (openai_usage.usage.prompt_tokens, openai_usage.usage.completion_tokens) == (11, 4)

    server.revoke_scoped_role("gateway_ada")
    with pytest.raises(openai.PermissionDeniedError) as revoked:
        _client(server, exchange(server, ada_key)).chat.completions.create(
            model="openai/gpt-4o", messages=MESSAGES, max_completion_tokens=16
        )
    assert revoked.value.body["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert len(received) == 3, "a revoked caller never dispatches"
    bea_again = _client(server, bea).chat.completions.create(
        model="anthropic/claude-sonnet-5", messages=MESSAGES, max_completion_tokens=16
    )
    assert bea_again.usage is not None
    assert (bea_again.usage.prompt_tokens, bea_again.usage.completion_tokens) == (5, 3)

    rows = call_rows(
        server,
        server.access_token(),
        "outcome = 'succeeded'",
        4,
        "caller_principal_id, requested_model, usage",
    )
    by_caller = {
        principal(ada): ("openai/gpt-4o", BACKENDS["openai/gpt-4o"][1]),
        principal(bea): ("anthropic/claude-sonnet-5", BACKENDS["anthropic/claude-sonnet-5"][1]),
    }
    assert {row["caller_principal_id"] for row in rows} == set(by_caller)
    for caller, (model, expected) in by_caller.items():
        mine = [row for row in rows if row["caller_principal_id"] == caller]
        assert len(mine) == 2
        assert {_model(row) for row in mine} == {model}
        assert usage(mine) == expected


@pytest.mark.integration
def test_embedding_and_image_evidence_reaches_bifrost_and_storage(
    gateway_server: tuple[WyrdTestServer, Received],
) -> None:
    server, received = gateway_server
    deploy(server, "openai", "text-embedding-3-small", ["embeddings"])
    deploy(server, "openai", "gpt-image-1", ["images"])
    Gateway(server_url=server.base_url, credential=server.api_key).put_capture_policy(
        {"mode": "payload", "payload_fields": ["response"]}
    )
    token = server.access_token()
    client = _client(server, token)

    embedding = client.embeddings.create(model="openai/text-embedding-3-small", input="hi")
    assert embedding.data[0].embedding == [0.25, -0.5, 0.75]
    assert embedding.usage.prompt_tokens == 6
    image = client.images.generate(model="openai/gpt-image-1", prompt="a rune")
    assert image.data is not None
    assert image.data[0].b64_json is not None
    assert [path for path, _ in received] == ["/v1/embeddings", "/v1/images/generations"]
    assert_upstream_credentials(received, "authorization", token)

    rows = call_rows(
        server,
        token,
        "outcome = 'succeeded'",
        2,
        "caller_principal_id, operation, usage, payload_object_refs",
    )
    by_operation = {row["operation"]: row for row in rows}
    assert set(by_operation) == {"embeddings", "images"}
    assert {row["caller_principal_id"] for row in rows} == {principal(token)}
    assert usage([by_operation["embeddings"]]) == {("input_tokens", "6")}

    digest = f"sha256:{hashlib.sha256(IMAGE_BYTES).hexdigest()}"
    refs = by_operation["images"]["payload_object_refs"]
    assert [ref["digest"] for ref in refs] == [digest]
    assert refs[0]["size_bytes"] == len(IMAGE_BYTES)
    stored = httpx.get(
        f"{server.base_url}/v1/gateway/payload-objects/{digest}",
        headers={"x-wyrd-access-token": f"Bearer {token}"},
    )
    assert stored.status_code == 200, stored.text
    assert stored.content == IMAGE_BYTES, "the storage reference retrieves identical bytes"
