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
from collections.abc import Iterator, Mapping
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

import httpx
import openai
import pytest
from openai.types.chat import ChatCompletionMessageParam
from wyrd.cards import CardKind
from wyrd.gateway_admin import Gateway
from wyrd.prompt import Prompt
from wyrd.prompt.decisions import ChoiceQuestion, NoulCriteria, NoulQuestion, ScoreQuestion
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli
from wyrd.verify import (
    Agent,
    AgentRunConfig,
    Check,
    ChoiceTransform,
    Component,
    ExecutionRoute,
    ProbabilityTransform,
    Requirement,
    ScoreTransform,
    Service,
    VerdictTransform,
    Verifier,
)

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

REFUND_CHOICES = {"CASE-PASS": "passed", "CASE-FAIL": "failed", "CASE-UNSURE": "inconclusive"}
"""State marker → the refund question's selected label."""

NATIVE_DESK_QUESTIONS: dict[str, Any] = {
    "refund_policy": {
        "type": "choice",
        "instructions": "Does the reply promise a refund the policy does not allow?",
        "criteria": {
            "passed": "The reply makes no refund promise.",
            "failed": "The reply promises a refund.",
            "inconclusive": "The reply cannot be judged.",
        },
    },
    "helpful": {
        "type": "noul",
        "instructions": "Does the reply help the customer?",
        "criteria": {"true": "It answers the question.", "false": "It does not."},
    },
    "polite": {"type": "noul", "instructions": "Is the reply polite?"},
    "urgency": {
        "type": "score",
        "instructions": "How urgently does the reply need follow-up?",
        "criteria": ["Routine", "Soon", "Immediate"],
    },
}
"""The refund desk's questions as the native Jev questions the typed models lower to."""

TYPED_DESK_QUESTIONS = {
    "refund_policy": ChoiceQuestion(
        instructions="Does the reply promise a refund the policy does not allow?",
        criteria={
            "passed": "The reply makes no refund promise.",
            "failed": "The reply promises a refund.",
            "inconclusive": "The reply cannot be judged.",
        },
    ),
    "helpful": NoulQuestion(
        instructions="Does the reply help the customer?",
        criteria=NoulCriteria(
            true_description="It answers the question.", false_description="It does not."
        ),
    ),
    "polite": NoulQuestion(instructions="Is the reply polite?"),
    "urgency": ScoreQuestion(
        instructions="How urgently does the reply need follow-up?",
        criteria=["Routine", "Soon", "Immediate"],
    ),
}
"""The same questions authored with the public Pydantic question models."""


def desk_answer(choice: str) -> dict[str, Any]:
    """Jev's answer to every refund desk question, selecting ``choice`` for the refund question."""
    return {
        "model": "jev-1.13.0",
        "answers": {
            "refund_policy": {
                "type": "choice",
                "choice": choice,
                "confidence": 0.9,
                "probabilities": {
                    label: 0.9 if label == choice else 0.05
                    for label in ("passed", "failed", "inconclusive")
                },
            },
            "helpful": {"type": "noul", "noul": 0.8},
            "polite": {"type": "noul", "noul": 0.7},
            "urgency": {
                "type": "score",
                "score": 1.0,
                "legend": {"0": "Routine", "1": "Soon", "2": "Immediate"},
            },
        },
        "usage": {"input_tokens": 50, "output_tokens": 5},
    }


POLICY_QUESTIONS: dict[str, Any] = {
    "refund": {
        "type": "choice",
        "instructions": "Does the reply follow the refund policy?",
        "criteria": {
            "compliant": "It follows the policy.",
            "violation": "It breaks the policy.",
            "unknown": "It cannot be judged.",
        },
    },
    "helpful": {"type": "noul", "instructions": "Does the reply help the customer?"},
    "unresolved": {"type": "noul", "instructions": "Does the reply leave the issue unresolved?"},
    "compliance": {
        "type": "score",
        "instructions": "How compliant is the reply?",
        "criteria": ["Low", "Medium", "High"],
    },
}
"""The policy desk's questions: a custom Choice, two Nouls, and a three-level Score."""


def policy_answers(
    label: str, confidence: float, helpful: float, unresolved: float, score: float
) -> dict[str, Any]:
    """Jev's answers to the policy desk questions."""
    return {
        "refund": {
            "type": "choice",
            "choice": label,
            "confidence": confidence,
            "probabilities": {label: confidence},
        },
        "helpful": {"type": "noul", "noul": helpful},
        "unresolved": {"type": "noul", "noul": unresolved},
        "compliance": {
            "type": "score",
            "score": score,
            "legend": {"0": "Low", "1": "Medium", "2": "High"},
        },
    }


POLICY_ANSWERS = {
    "DESK-PASS": policy_answers("compliant", 0.3, 0.75, 0.02, 2.0),
    "DESK-FAIL": policy_answers("violation", 0.99, 0.25, 0.98, 1.8),
    "DESK-UNSURE": policy_answers("unknown", 0.5, 0.5, 0.5, 2.0),
}
"""State marker → Jev's policy desk answers: every bound, both polarities, and the band between."""


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
        for marker, choice in REFUND_CHOICES.items():
            if marker in state:
                reply = desk_answer(choice)
        for marker, answers in POLICY_ANSWERS.items():
            if marker in state:
                reply = {"model": "jev-1.13.0", "answers": answers}
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
            with WyrdTestServer(
                provider_base_url=recorded.url, verification_runtime=True, mutate_env=False
            ) as server:
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


def refund_desk(prompt: Prompt, transform: VerdictTransform | Mapping[str, Any]) -> Service:
    """``refund-desk``: the ``refund-judge`` Jev Agent asking ``prompt``, verified by its Choice answer.

    ``refund-promise`` binds the ``reply`` slot to the Prompt's ``${context}`` and reads
    the judge's answer through ``transform``.
    """
    judge = Agent(
        "refund-judge",
        "1.0.0",
        prompt=prompt,
        execution_route=ExecutionRoute.wyrd_gateway(deployment="jev"),
        run_config=AgentRunConfig(max_iterations=1),
    )
    check = Check.agent(judge, inputs={"context": "inputs.reply"}, verdict_transform=transform)
    verifier = Verifier(
        "refund-promise",
        "1.0.0",
        inputs={"reply": str},
        requirements=[
            Requirement(
                "no-refund-promise",
                "The reply makes no refund promise the policy does not allow.",
                check,
            )
        ],
    )
    return Service("refund-desk", "1.0.0").add_component(
        Component("refund-judge", judge).verified_by([verifier])
    )


def test_typed_jev_choice_check_becomes_the_verdict(
    jev: tuple[WyrdTestServer, Receiver],
) -> None:
    """Typed, native, and mixed Jev questions author one Prompt whose Choice answer is the verdict.

    The Pydantic question and transform models need no ``model_dump``: the
    declared graph stores the native Jev request and canonical transform, so
    redeclaring the native-dictionary form changes nothing. Each reply reaches
    Jev once as the bound state, and the selected label becomes the verdict.
    """
    server, upstream = jev
    client = client_of(server)
    typed, native, mixed = (
        Prompt.jev("jev-latest", state="${context}", questions=questions)
        for questions in (
            TYPED_DESK_QUESTIONS,
            NATIVE_DESK_QUESTIONS,
            {
                **NATIVE_DESK_QUESTIONS,
                **{k: TYPED_DESK_QUESTIONS[k] for k in ("helpful", "urgency")},
            },
        )
    )
    assert typed.model_dump() == native.model_dump() == mixed.model_dump()
    assert typed.model_dump()["body"] == {
        "state": "${context}",
        "questions": NATIVE_DESK_QUESTIONS,
    }

    state = WyrdState.declare(refund_desk(typed, ChoiceTransform(question="refund_policy")), client)
    again = WyrdState.declare(
        refund_desk(native, {"kind": "choice", "question": "refund_policy"}), client
    )
    assert [again.card_ref(alias) for alias in again.aliases] == [
        state.card_ref(alias) for alias in state.aliases
    ]
    envelopes = [state.card(alias) for alias in state.aliases]
    [verifier] = [each for each in envelopes if each.card_ref.kind == CardKind.Verifier]
    [requirement] = verifier.spec["requirements"]
    assert requirement["check"]["verdict_transform"] == {
        "kind": "choice",
        "question": "refund_policy",
    }
    prompts = [each.spec for each in envelopes if each.card_ref.kind == CardKind.Prompt]
    assert prompts, [each.card_ref for each in envelopes]
    assert all(spec == typed.model_dump() for spec in prompts), prompts

    for reply, outcome in [
        ('CASE-PASS: "Thanks" for asking.\nRefunds follow the policy.', "passed"),
        ("CASE-FAIL: you will get a full refund today.", "failed"),
        ("CASE-UNSURE: maybe.", "inconclusive"),
    ]:
        asked = len(upstream.deliveries)
        result = state.run("refund-judge").observe.verify("refund-promise", reply=reply)

        [judged] = result.requirements
        assert (judged["outcome"], judged["reason"]) == (
            outcome,
            f"Question 'refund_policy' selected '{outcome}' (confidence 0.9).",
        ), judged
        assert judged.get("citations", []) == []
        assert result.verdict == outcome
        (delivery,) = upstream.deliveries[asked:]
        assert (delivery.path, delivery.body) == (
            "/v1/systemone",
            {"model": "jev-latest", "state": reply, "questions": NATIVE_DESK_QUESTIONS},
        )


def policy_desk() -> Service:
    """``policy-desk``: one Jev Agent's answer read by four acceptance policies.

    ``desk-policy`` maps the custom ``refund`` labels, reads ``helpful`` expected
    true and ``unresolved`` expected false, and requires ``compliance >= 2``.
    """
    judge = Agent(
        "desk-judge",
        "1.0.0",
        prompt=Prompt.jev("jev-latest", state="${context}", questions=POLICY_QUESTIONS),
        execution_route=ExecutionRoute.wyrd_gateway(deployment="jev"),
        run_config=AgentRunConfig(max_iterations=1),
    )
    policies: list[tuple[str, str, VerdictTransform | Mapping[str, Any]]] = [
        (
            "refund-policy",
            "The reply follows the refund policy.",
            {
                "kind": "choice",
                "question": "refund",
                "outcomes": {
                    "compliant": "passed",
                    "violation": "failed",
                    "unknown": "inconclusive",
                },
            },
        ),
        (
            "helpful",
            "The reply helps the customer.",
            ProbabilityTransform(question="helpful", expected=True, pass_at=0.75, fail_at=0.25),
        ),
        (
            "resolved",
            "The reply resolves the issue.",
            ProbabilityTransform(question="unresolved", expected=False, pass_at=0.9, fail_at=0.1),
        ),
        (
            "compliant",
            "The reply is at least highly compliant.",
            ScoreTransform(question="compliance", operator="greater_than_or_equals", expected=2),
        ),
    ]
    verifier = Verifier(
        "desk-policy",
        "1.0.0",
        inputs={"reply": str},
        requirements=[
            Requirement(
                id,
                statement,
                Check.agent(judge, inputs={"context": "inputs.reply"}, verdict_transform=policy),
            )
            for id, statement, policy in policies
        ],
    )
    return Service("policy-desk", "1.0.0").add_component(
        Component("desk-judge", judge).verified_by([verifier])
    )


@pytest.mark.parametrize(
    ("reply", "outcomes", "verdict", "reasons"),
    [
        (
            "DESK-PASS: the refund follows policy.",
            ["passed"] * 4,
            "passed",
            [
                "Question 'refund' selected 'compliant' (confidence 0.3),"
                " which the policy maps to passed.",
                "Question 'helpful' answered true with probability 0.75;"
                " the expected answer true has probability 0.75, at least pass_at 0.75.",
                "Question 'unresolved' answered true with probability 0.02;"
                f" the expected answer false has probability {1 - 0.02}, at least pass_at 0.9.",
                "Question 'compliance' scored 2 on levels 0 to 2; greater_than_or_equals 2 holds.",
            ],
        ),
        (
            "DESK-FAIL: you get a refund whatever the policy says.",
            ["failed"] * 4,
            "failed",
            [
                "Question 'refund' selected 'violation' (confidence 0.99),"
                " which the policy maps to failed.",
                "Question 'helpful' answered true with probability 0.25;"
                " the expected answer true has probability 0.25, at most fail_at 0.25.",
                "Question 'unresolved' answered true with probability 0.98;"
                f" the expected answer false has probability {1 - 0.98}, at most fail_at 0.1.",
                "Question 'compliance' scored 1.8 on levels 0 to 2;"
                " greater_than_or_equals 2 does not hold.",
            ],
        ),
        (
            "DESK-UNSURE: we will see.",
            ["inconclusive", "inconclusive", "inconclusive", "passed"],
            "inconclusive",
            [
                "Question 'refund' selected 'unknown' (confidence 0.5),"
                " which the policy maps to inconclusive.",
                "Question 'helpful' answered true with probability 0.5;"
                " the expected answer true has probability 0.5, between fail_at 0.25 and pass_at 0.75.",
                "Question 'unresolved' answered true with probability 0.5;"
                " the expected answer false has probability 0.5, between fail_at 0.1 and pass_at 0.9.",
                "Question 'compliance' scored 2 on levels 0 to 2; greater_than_or_equals 2 holds.",
            ],
        ),
    ],
)
def test_jev_policies_judge_choice_probability_and_score(
    jev: tuple[WyrdTestServer, Receiver],
    reply: str,
    outcomes: list[str],
    verdict: str,
    reasons: list[str],
) -> None:
    """Each acceptance policy reads its own question of one Jev answer.

    A custom Choice map assigns every label its outcome whatever the
    confidence, Noul bounds are inclusive for both polarities with an
    inconclusive band between, and a Score comparison passes at its bound and
    fails below it. The selected native answer is each requirement's actual.
    """
    server, upstream = jev
    state = WyrdState.declare(policy_desk(), client_of(server))
    asked = len(upstream.deliveries)

    result = state.run("desk-judge").observe.verify("desk-policy", reply=reply)

    answers = POLICY_ANSWERS[reply.split(":")[0]]
    judged = {each["id"]: each for each in result.requirements}
    for (id, question), outcome, reason in zip(
        [
            ("refund-policy", "refund"),
            ("helpful", "helpful"),
            ("resolved", "unresolved"),
            ("compliant", "compliance"),
        ],
        outcomes,
        reasons,
        strict=True,
    ):
        assert (judged[id]["outcome"], judged[id]["reason"]) == (outcome, reason), judged[id]
        assert judged[id]["actual"] == answers[question], id
        assert judged[id].get("citations", []) == []
    assert result.verdict == verdict
    deliveries = upstream.deliveries[asked:]
    assert deliveries
    assert all(
        (each.path, each.body)
        == ("/v1/systemone", {"model": "jev-latest", "state": reply, "questions": POLICY_QUESTIONS})
        for each in deliveries
    ), deliveries
