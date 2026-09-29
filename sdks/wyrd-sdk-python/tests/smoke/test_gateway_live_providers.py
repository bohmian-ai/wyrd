"""Opt-in live provider smoke journeys through the Wyrd gateway.

This lane is the only place a gateway call reaches a real upstream. Operator
credentials come from the environment — normally exported by an untracked
``mise.local.toml`` — and a provider whose credential is absent is skipped
explicitly instead of failing the lane. Nothing here asserts token counts or
deterministic failures: the proof is connectivity and a parseable response
shape. Credentials are never printed, logged, or written to a fixture; only the
provider name and model reach the skip and failure messages.

Run with ``mise run test:gateway:smoke:live``; the credential-free
``test:gateway:journey`` lane never selects it.
"""

from __future__ import annotations

import contextlib
import json
import os
import pathlib
import subprocess
from collections.abc import Iterator, Sequence
from typing import Any

import anthropic
import httpx
import openai
import pytest
from google import genai
from wyrd.gateway import Gateway
from wyrd.testing import WyrdTestServer

# Provider → environment variables carrying that provider's own credential,
# most specific first. These are the conventional names a provider's own SDK
# reads, so an operator environment already holding them needs no second copy
# and this lane owns no private alias.
CREDENTIAL_VARIABLES = {
    "openai": ("OPENAI_API_KEY",),
    "anthropic": ("ANTHROPIC_API_KEY",),
    "gemini": ("GOOGLE_API_KEY", "GEMINI_API_KEY"),
}

# Provider → model this lane calls.
MODELS = {
    "openai": "gpt-4o-mini",
    "anthropic": "claude-sonnet-4-5",
    "gemini": "gemini-3.7-flash",
}

# Provider → header its deployment authenticates with; bearer when absent.
API_KEY_HEADERS = {"anthropic": "x-api-key", "gemini": "x-goog-api-key"}

# Operation family → the OpenAI model this lane calls for it. Only OpenAI
# serves these families: `ProviderAdapter` restricts the Anthropic, Gemini, and
# Vertex adapters to Chat Completions.
OPERATION_MODELS = {
    "embeddings": "text-embedding-3-small",
    "speech": "tts-1",
    "transcription": "whisper-1",
    "images": "gpt-image-1-mini",
}

# Repository root, so the tenant administrator's submission runs the real CLI.
REPO_ROOT = pathlib.Path(__file__).resolve().parents[4]

MESSAGES: list[Any] = [{"role": "user", "content": "Reply with the single word: ready"}]

# Prompt every non-chat family sends. It is deliberately trivial: this lane
# proves that a real provider payload decodes through the typed schemas, not
# that a model is good at anything.
PROMPT = "Reply with the single word: ready"

# Output budget every call asks for. It is generous for a one-word answer
# because a thinking model spends this budget on reasoning first: too small a
# cap returns a truncated answer carrying no text, which proves nothing about
# connectivity.
MAX_OUTPUT_TOKENS = 512


def credential(provider: str) -> str:
    """Operator credential for ``provider``, skipping the test when unset.

    The first variable of ``CREDENTIAL_VARIABLES[provider]`` that is set wins.
    The value is returned but never rendered: the skip message names only the
    variables an operator may export.
    """
    variables = CREDENTIAL_VARIABLES[provider]
    for variable in variables:
        secret = os.environ.get(variable)
        if secret:
            return secret
    pytest.skip(f"{provider} smoke skipped: none of {', '.join(variables)} is set")


def model(provider: str) -> str:
    """Model this lane calls on ``provider``."""
    return MODELS[provider]


def operation_model(family: str) -> str:
    """OpenAI model this lane calls for operation ``family``."""
    return OPERATION_MODELS[family]


def submit(server: WyrdTestServer, provider: str, secret: str) -> str:
    """Submit ``secret`` as a managed credential and return the credential name.

    The tenant administrator writes the key through the real CLI on stdin,
    which is the public submission path: no SDK exposes a managed-secret
    mutation helper, the secret never reaches a process argument or a file on
    disk, and neither the command nor the server's answer may echo it.
    """
    name = f"{provider}-live"
    body = json.dumps(
        {
            "name": name,
            "provider": provider,
            "source": {"managed_secret": {"secret": secret}},
        }
    )
    environment = {
        key: value
        for key, value in os.environ.items()
        if key not in {"WYRD_API_KEY", "WYRD_TENANT", "WYRD_WORKLOAD_TOKEN"}
    }
    environment["WYRD_ACCESS_TOKEN"] = server.access_token()
    finished = subprocess.run(  # noqa: S603
        [  # noqa: S607
            "cargo",
            "run",
            "--locked",
            "--quiet",
            "-p",
            "wyrd-cli",
            "--bin",
            "wyrd",
            "--",
            "gateway",
            "credential",
            "put",
            "--file",
            "-",
            "--server",
            server.base_url,
        ],
        input=body,
        cwd=REPO_ROOT,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert finished.returncode == 0, f"{provider} submission failed: {finished.stderr}"
    assert secret not in finished.stdout, "the CLI echoed the submitted secret"
    assert secret not in finished.stderr, "the CLI echoed the submitted secret"
    assert json.loads(finished.stdout)["source"] == "managed_secret"
    return name


def caller(server: WyrdTestServer, provider: str, models: Sequence[str]) -> str:
    """Access token of a non-admin principal that may only invoke ``models``.

    Administration and invocation are separate principals here for the same
    reason they are in production: the key submitted above is never readable
    by the identity that calls the provider.
    """
    key = server.scoped_api_key(
        f"{provider}_live_invoker",
        [
            {
                "resource": "gateway",
                "action": "invoke",
                "scope": {"gateway": {"model": {"provider": provider, "model": native}}},
            }
            for native in models
        ],
    )
    response = httpx.post(
        f"{server.base_url}/auth/token",
        json={"grant_type": "wyrd_api_key", "api_key": key},
    )
    response.raise_for_status()
    return response.json()["access_token"]


@contextlib.contextmanager
def live_server(
    provider: str,
    secret: str,
    *,
    deployments: Sequence[tuple[str, Sequence[str]]] | None = None,
) -> Iterator[tuple[WyrdTestServer, str]]:
    """Yield a server whose built-in adapters reach real providers, plus a caller token.

    The sequence is the production one: the tenant administrator submits the
    operator's own provider key as a managed secret through the CLI, declares
    the deployments that may use it, and a separate principal scoped to exactly
    those models receives the token every call below carries. ``deployments``
    administers one deployment per ``(model, capabilities)`` pair, defaulting to
    the provider's chat model, because an operation family is only reachable
    through a deployment that declares it and each family needs its own model.
    """
    with WyrdTestServer(mutate_env=True, live_providers=True) as server:
        name = submit(server, provider, secret)
        header = API_KEY_HEADERS.get(provider)
        auth = (
            {"api_key_header": {"header": header, "credential": name}}
            if header
            else {"bearer": {"credential": name}}
        )
        declared = list(deployments or [(model(provider), ["chat_completions"])])
        gateway = Gateway(server_url=server.base_url, credential=server.api_key)
        for index, (native, capabilities) in enumerate(declared):
            deployment: dict[str, Any] = {
                "name": f"{provider}-live-{index}",
                "model": {"provider": provider, "model": native},
                "adapter": provider,
                "auth": auth,
                "capabilities": list(capabilities),
                "routing_weight": 1,
            }
            gateway.put_deployment(deployment)
        yield server, caller(server, provider, [native for native, _ in declared])


@pytest.mark.smoke
@pytest.mark.parametrize("provider", ["openai", "anthropic", "gemini"])
def test_openai_compatible_client_reaches_a_live_provider(provider: str) -> None:
    """The unmodified OpenAI client reaches each real provider through the server."""
    secret = credential(provider)
    with live_server(provider, secret) as (server, token):
        client = openai.OpenAI(base_url=f"{server.base_url}/v1", api_key=token, max_retries=0)
        completion = client.chat.completions.create(
            model=f"{provider}/{model(provider)}",
            messages=MESSAGES,
            max_completion_tokens=MAX_OUTPUT_TOKENS,
        )
        assert completion.choices, f"{provider} returned no choice"
        assert isinstance(completion.choices[0].message.content, str)
        assert completion.usage is not None, f"{provider} reported no usage"


@pytest.mark.smoke
def test_anthropic_client_reaches_live_anthropic() -> None:
    """The official Anthropic client reaches real Anthropic through native ingress.

    Native ingress names an exact provider model, never a ``provider/model``
    projection: that is the id the provider's own client would send.
    """
    secret = credential("anthropic")
    with live_server("anthropic", secret) as (server, token):
        client = anthropic.Anthropic(base_url=server.base_url, api_key=token, max_retries=0)
        message = client.messages.create(
            model=model("anthropic"),
            max_tokens=MAX_OUTPUT_TOKENS,
            messages=MESSAGES,
        )
        assert message.content, "anthropic returned no content block"
        assert message.usage.output_tokens >= 0


@pytest.mark.smoke
def test_google_client_reaches_live_gemini() -> None:
    """The official Google GenAI client reaches real Gemini through native ingress.

    The model is the exact Gemini id: ``/v1beta/models/{target}`` is one path
    segment, so a ``provider/model`` projection would not route at all.
    """
    secret = credential("gemini")
    with live_server("gemini", secret) as (server, token):
        client = genai.Client(
            api_key=token,
            http_options=genai.types.HttpOptions(base_url=server.base_url, retry_options=None),
        )
        answer = client.models.generate_content(
            model=model("gemini"),
            contents="Reply with the single word: ready",
        )
        assert answer.candidates, "gemini returned no candidate"


@contextlib.contextmanager
def openai_live(*deployments: tuple[str, Sequence[str]]) -> Iterator[openai.OpenAI]:
    """Yield an OpenAI client aimed at a live server administering ``deployments``."""
    secret = credential("openai")
    with live_server("openai", secret, deployments=list(deployments)) as (server, token):
        yield openai.OpenAI(base_url=f"{server.base_url}/v1", api_key=token, max_retries=0)


@pytest.mark.smoke
def test_openai_client_reaches_live_responses() -> None:
    """Real Responses answers decode buffered and as a relayed event stream.

    Both shapes are covered because the streamed answer is a different wire
    contract: its usage arrives on the terminal ``response.completed`` event
    rather than a top-level object.
    """
    projection = f"openai/{model('openai')}"
    with openai_live((model("openai"), ["chat_completions", "responses"])) as client:
        answer = client.responses.create(
            model=projection,
            input=PROMPT,
            max_output_tokens=MAX_OUTPUT_TOKENS,
        )
        assert answer.output_text.strip(), "responses returned no output text"
        assert answer.usage is not None, "responses reported no usage"

        completed = [
            event
            for event in client.responses.create(
                model=projection,
                input=PROMPT,
                max_output_tokens=MAX_OUTPUT_TOKENS,
                stream=True,
            )
            if event.type == "response.completed"
        ]
        assert completed, "the relayed stream carried no terminal event"
        assert completed[-1].response.usage is not None, "the terminal event reported no usage"


@pytest.mark.smoke
def test_openai_client_reaches_live_embeddings() -> None:
    """A real embedding answer decodes with its vector and usage intact."""
    embedding = operation_model("embeddings")
    with openai_live((embedding, ["embeddings"])) as client:
        answer = client.embeddings.create(model=f"openai/{embedding}", input="ready")
        assert answer.data, "embeddings returned no vector"
        assert len(answer.data[0].embedding) > 1, "the vector did not survive the gateway"
        assert answer.usage.prompt_tokens > 0, "embeddings reported no usage"


@pytest.mark.smoke
def test_openai_client_round_trips_live_audio() -> None:
    """Speech bytes come back verbatim and transcribe through the multipart route.

    The generated audio is fed straight back as the transcription upload, so
    the two Audio directions prove each other without a fixture file.
    """
    speech_model = operation_model("speech")
    transcription_model = operation_model("transcription")
    with openai_live(
        (speech_model, ["audio"]),
        (transcription_model, ["audio"]),
    ) as client:
        spoken = client.audio.speech.create(
            model=f"openai/{speech_model}",
            voice="alloy",
            input="ready",
            response_format="mp3",
        )
        audio = spoken.read()
        assert len(audio) > 1024, "speech returned no audio payload"

        transcript = client.audio.transcriptions.create(
            model=f"openai/{transcription_model}",
            file=("speech.mp3", audio, "audio/mpeg"),
        )
        assert transcript.text.strip(), "transcription returned no text"


@pytest.mark.smoke
def test_openai_client_reaches_live_images() -> None:
    """A real image answer decodes and its bytes survive the gateway."""
    image_model = operation_model("images")
    with openai_live((image_model, ["images"])) as client:
        answer = client.images.generate(
            model=f"openai/{image_model}",
            prompt="a plain blue square on a white background",
            n=1,
            size="1024x1024",
        )
        assert answer.data, "images returned no item"
        assert answer.data[0].b64_json, "the image bytes did not survive the gateway"


@pytest.mark.smoke
def test_openai_client_runs_a_live_batch_lifecycle() -> None:
    """Upload, create, retrieve, and cancel a real batch under Wyrd ids.

    The batch is cancelled rather than awaited: completion takes up to a day,
    and every typed schema on the submission path is already exercised by the
    four exchanges. The input line names the caller's ``provider/model``
    projection, which the gateway rewrites to the provider's own model before
    the file is uploaded.
    """
    chat = model("openai")
    line = json.dumps(
        {
            "custom_id": "smoke-1",
            "method": "POST",
            "url": "/v1/chat/completions",
            "body": {"model": f"openai/{chat}", "messages": MESSAGES, "max_tokens": 16},
        }
    )
    with openai_live((chat, ["chat_completions", "batches"])) as client:
        upload = client.files.create(
            file=("batch.jsonl", f"{line}\n".encode(), "application/jsonl"),
            purpose="batch",
        )
        assert upload.id.startswith("file-"), f"unexpected file id: {upload.id}"

        batch = client.batches.create(
            input_file_id=upload.id,
            endpoint="/v1/chat/completions",
            completion_window="24h",
        )
        assert batch.id.startswith("batch_"), f"unexpected batch id: {batch.id}"
        assert client.batches.retrieve(batch.id).id == batch.id
        assert client.batches.cancel(batch.id).status in {"cancelling", "cancelled"}
