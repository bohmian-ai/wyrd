"""A tenant reaches TypeSafe Jev through the Wyrd Gateway.

Native ``POST /v1/systemone`` callers reach one Jev deployment with typed
questions and get typed answers back, a stock OpenAI client reaches the same
deployment on ``/v1/chat/completions`` by sending the typed Jev request as its
last user message. A local recording upstream stands in for
``api.typesafe.ai`` and sees only the operator's Jev key.
"""

from __future__ import annotations

import json
import threading
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

import httpx
import openai
import pytest
from openai.types.chat import ChatCompletionMessageParam
from wyrd.gateway_admin import Gateway
from wyrd.testing import WyrdTestServer, cli

from .support import Delivery, Receiver, client_of

pytestmark = pytest.mark.integration

JEV_KEY = "jev-operator-key"

QUESTIONS = {
    "helpful": {"type": "noul", "instructions": "Does the reply help the customer?"},
    "tone": {
        "type": "choice",
        "instructions": "What is the tone of the reply?",
        "criteria": {"polite": "courteous", "rude": "curt"},
    },
}

REFUSALS = {
    "refused-form": 400,
    "rate-limited": 429,
    "bad-key": 401,
}
"""State text → the status the upstream refuses it with."""


def answer(noul: float, tone: str) -> dict[str, Any]:
    """Jev's answer: ``helpful`` with probability ``noul``, tone ``tone``."""
    return {
        "model": "jev-1.13.0",
        "answers": {
            "helpful": {"type": "noul", "noul": noul},
            "tone": {
                "type": "choice",
                "choice": tone,
                "confidence": 0.8,
                "probabilities": {"polite": 0.8, "rude": 0.2},
            },
        },
        "usage": {"input_tokens": 378, "output_tokens": 65},
    }


class JevUpstream(BaseHTTPRequestHandler):
    """Mock ``/v1/systemone`` recording each request into ``upstream``.

    Answers helpful and polite unless the state names a scripted case:
    ``Go away`` is unhelpful and rude, and each ``REFUSALS`` key is refused
    with a body that echoes the operator key after ``key cred``.
    """

    upstream: Receiver

    def do_POST(self) -> None:
        body = json.loads(self.rfile.read(int(self.headers["content-length"])))
        headers = {k.lower(): v for k, v in self.headers.items()}
        self.upstream.record(Delivery(self.path, headers, body))
        state = json.dumps(body.get("state"))
        status, reply = 200, answer(0.93, "polite")
        if "Go away" in state:
            reply = answer(0.12, "rude")
        for needle, refused in REFUSALS.items():
            if needle in state:
                status, reply = refused, {"detail": f"key cred {JEV_KEY}"}
        payload = json.dumps(reply).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: Any) -> None:
        del format, args


@pytest.fixture
def jev() -> Iterator[tuple[WyrdTestServer, Receiver]]:
    """A server with one Jev deployment on a local upstream.

    Each test gets its own server: an upstream refusal cools the deployment
    down, which must not leak into the next story.
    """
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), JevUpstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("RecordingJev", (JevUpstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        with pytest.MonkeyPatch.context() as env:
            env.setenv("WYRD_TEST_GATEWAY_PROVIDER_KEY", JEV_KEY)
            with WyrdTestServer(provider_base_url=recorded.url, mutate_env=False) as server:
                admin = client_of(server)
                cli.put_provider_credential(
                    {
                        "name": "jev-key",
                        "provider": "jev",
                        "source": {"environment": {"binding": "test-jev-key"}},
                    },
                    client=admin,
                )
                Gateway(admin).put_deployment(
                    {
                        "name": "jev",
                        "adapter": "jev",
                        "auth": {"bearer": {"credential": "jev-key"}},
                        "capabilities": ["system_one", "chat_completions"],
                    }
                )
                yield server, recorded
    finally:
        upstream.shutdown()


def post(server: WyrdTestServer, route: str, body: dict[str, Any]) -> httpx.Response:
    """POST ``body`` to the Gateway ``route`` with an administrator access token."""
    return httpx.post(
        f"{server.base_url}{route}",
        json=body,
        headers={"authorization": f"Bearer {server.access_token()}"},
        timeout=30,
    )


def test_native_systemone_answers_relay_intact(jev: tuple[WyrdTestServer, Receiver]) -> None:
    server, upstream = jev
    asked = len(upstream.deliveries)
    body = {"model": "jev-latest", "state": "Thanks!", "questions": QUESTIONS}

    reply = post(server, "/v1/systemone", {**body, "model": "jev/jev-latest"})

    assert (reply.status_code, reply.json()) == (200, answer(0.93, "polite"))
    (delivery,) = upstream.deliveries[asked:]
    assert (delivery.path, delivery.body) == ("/v1/systemone", body)
    assert delivery.headers["authorization"] == f"Bearer {JEV_KEY}"


def test_invalid_jev_questions_are_refused_before_jev(
    jev: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = jev
    asked = len(upstream.deliveries)

    for body in [
        {
            "model": "jev/jev-latest",
            "state": "s",
            "questions": {"q": {"type": "essay", "instructions": "x"}},
        },
        {"model": "jev/jev-latest", "state": {"answer": "s"}, "questions": QUESTIONS},
        {"model": "jev/jev-latest", "state": "s", "questions": QUESTIONS, "temperature": 0},
    ]:
        reply = post(server, "/v1/systemone", body)
        assert (reply.status_code, reply.json()["error"]["code"]) == (
            400,
            "WYRD_GATEWAY_400_INVALID_REQUEST",
        ), reply.text
    assert upstream.deliveries[asked:] == []


def openai_client(server: WyrdTestServer) -> openai.OpenAI:
    """A stock OpenAI client pointed at the Gateway with an administrator token."""
    return openai.OpenAI(
        base_url=f"{server.base_url}/v1", api_key=server.access_token(), max_retries=0
    )


def chat(state: str) -> list[ChatCompletionMessageParam]:
    """Messages whose last user turn is the typed Jev request about ``state``."""
    return [
        {"role": "system", "content": "Judge the reply."},
        {"role": "user", "content": json.dumps({"state": state, "questions": QUESTIONS})},
    ]


def test_chat_completions_pass_typed_jev_requests_through(
    jev: tuple[WyrdTestServer, Receiver],
) -> None:
    server, upstream = jev
    asked = len(upstream.deliveries)
    client = openai_client(server)
    state = 'He said "thanks".\nAnything else?'

    completion = client.chat.completions.create(model="jev/jev-latest", messages=chat(state))

    content = completion.choices[0].message.content
    assert content is not None
    assert json.loads(content) == answer(0.93, "polite")
    assert completion.usage is not None
    assert (
        completion.usage.prompt_tokens,
        completion.usage.completion_tokens,
        completion.usage.total_tokens,
    ) == (378, 65, 443)
    (delivery,) = upstream.deliveries[asked:]
    assert (delivery.path, delivery.body) == (
        "/v1/systemone",
        {"model": "jev-latest", "state": state, "questions": QUESTIONS},
    )
    assert delivery.headers["authorization"] == f"Bearer {JEV_KEY}"

    with pytest.raises(openai.BadRequestError) as raised:
        client.chat.completions.create(
            model="jev/jev-latest",
            messages=[{"role": "user", "content": "Is this reply helpful?"}],
        )
    assert raised.value.code == "WYRD_GATEWAY_400_INVALID_REQUEST"
    assert len(upstream.deliveries) == asked + 1, "ordinary chat never reaches Jev"


def test_jev_refusals_relay_like_other_providers(jev: tuple[WyrdTestServer, Receiver]) -> None:
    """Jev refusals relay on both routes exactly like every other provider's.

    The upstream status returns with Jev's body natively and an OpenAI error
    envelope on Chat Completions, the operator key removed and no Wyrd code
    invented. A validation refusal and a refused key are never retried and do
    not cool the deployment down. Throttling cools the one deployment down, so
    it is proven last; the 5xx statuses share its retry class and are pinned by
    the gateway adapter's unit tests.
    """
    server, upstream = jev
    client = openai_client(server)

    def native(state: str) -> httpx.Response:
        return post(
            server,
            "/v1/systemone",
            {"model": "jev/jev-latest", "state": state, "questions": QUESTIONS},
        )

    for state, status, error in [
        ("refused-form", 400, openai.BadRequestError),
        ("bad-key", 401, openai.AuthenticationError),
    ]:
        asked = len(upstream.deliveries)
        reply = native(state)
        assert (reply.status_code, reply.json()) == (status, {"detail": "key cred "})
        with pytest.raises(error) as refused:
            client.chat.completions.create(model="jev/jev-latest", messages=chat(state))
        assert refused.value.status_code == status
        assert refused.value.code is None
        assert JEV_KEY not in refused.value.response.text
        assert len(upstream.deliveries) == asked + 2, f"{status} is never retried"

    with pytest.raises(openai.RateLimitError) as throttled:
        client.chat.completions.create(model="jev/jev-latest", messages=chat("rate-limited"))
    assert throttled.value.code is None
    assert JEV_KEY not in throttled.value.response.text
