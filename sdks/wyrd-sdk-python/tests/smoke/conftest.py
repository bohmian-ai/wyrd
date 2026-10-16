"""Credentials, models, and the one live server shared by the smoke modules.

Operator credentials come from the environment — normally exported by an
untracked ``mise.local.toml`` — and a provider whose credential is absent is
skipped explicitly instead of failing the lane. Credentials are never printed,
logged, or written to a fixture; only the provider name and model reach the
skip and failure messages.

The whole session shares one live server, set up the production way: one
deployment per keyed provider on one tenant, and one caller token for every
deployed model.
"""

from __future__ import annotations

import os
from collections.abc import Iterator, Sequence
from dataclasses import dataclass
from typing import Literal

import pytest
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway, GatewayOperation, ProviderAuth, ProviderDeployment
from wyrd.testing import WyrdTestServer, cli

# Every provider this lane sweeps.
Provider = Literal["openai", "anthropic", "gemini", "jev"]
PROVIDERS: tuple[Provider, ...] = ("openai", "anthropic", "gemini", "jev")

# Provider → environment variables carrying that provider's own credential,
# most specific first. These are the conventional names a provider's own SDK
# reads, so an operator environment already holding them needs no second copy
# and this lane owns no private alias.
CREDENTIAL_VARIABLES = {
    "openai": ("OPENAI_API_KEY",),
    "anthropic": ("ANTHROPIC_API_KEY",),
    "gemini": ("GOOGLE_API_KEY", "GEMINI_API_KEY"),
    "jev": ("TYPESAFE_API_KEY",),
}

# Provider → current model this lane calls, overridable per provider with
# ``WYRD_SMOKE_<PROVIDER>_MODEL``.
MODELS = {
    "openai": "gpt-6-luna",
    "anthropic": "claude-haiku-5-5",
    "gemini": "gemini-3.8-flash",
    "jev": "jev-latest",
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


def secret(provider: str) -> str | None:
    """Operator credential for ``provider``, or ``None`` when unset.

    The first variable of ``CREDENTIAL_VARIABLES[provider]`` that is set wins.
    """
    for variable in CREDENTIAL_VARIABLES[provider]:
        value = os.environ.get(variable)
        if value:
            return value
    return None


def credential(provider: str) -> str:
    """Operator credential for ``provider``, skipping the test when unset.

    The value is returned but never rendered: the skip message names only the
    variables an operator may export.
    """
    value = secret(provider)
    if value:
        return value
    variables = CREDENTIAL_VARIABLES[provider]
    # ty does not yet read pytest.skip's signature through its exception decorator.
    pytest.skip(f"{provider} smoke skipped: none of {', '.join(variables)} is set")  # ty: ignore[too-many-positional-arguments]


def model(provider: str) -> str:
    """Model this lane calls on ``provider``: ``WYRD_SMOKE_<PROVIDER>_MODEL`` or the default."""
    return os.environ.get(f"WYRD_SMOKE_{provider.upper()}_MODEL") or MODELS[provider]


def operation_model(family: str) -> str:
    """OpenAI model this lane calls for operation ``family``."""
    return OPERATION_MODELS[family]


def deployed_models(provider: str) -> list[tuple[str, Sequence[GatewayOperation]]]:
    """Each ``(model, capabilities)`` pair ``provider``'s deployment declares.

    An operation family is only reachable through a deployment that declares
    it, so OpenAI carries every family its cases call and Jev its typed route.
    """
    if provider == "openai":
        return [
            (model("openai"), ["chat_completions", "responses", "batches"]),
            (operation_model("embeddings"), ["embeddings"]),
            (operation_model("speech"), ["audio"]),
            (operation_model("transcription"), ["audio"]),
            (operation_model("images"), ["images"]),
        ]
    if provider == "jev":
        return [(model("jev"), ["system_one", "chat_completions"])]
    return [(model(provider), ["chat_completions"])]


@dataclass(frozen=True)
class LiveGateway:
    """The session's live server and the caller token every gateway call carries."""

    server: WyrdTestServer
    token: str

    @property
    def base_url(self) -> str:
        """Base URL of the live server."""
        return self.server.base_url


def deploy(admin: WyrdClient, provider: Provider, key: str) -> None:
    """Submit ``key`` as ``provider``'s managed credential and deploy it.

    The tenant administrator writes the key through the in-process CLI
    command, which reports a rejected body without quoting it and returns a
    redacted view; the view is checked so a regression that echoes the key
    fails without printing it.
    """
    name = f"{provider}-live"
    view = cli.put_provider_credential(
        {"name": name, "provider": provider, "source": {"managed_secret": {"secret": key}}},
        client=admin,
    )
    redacted = key not in str(view)
    assert redacted, f"the {provider} credential view echoed the submitted secret"
    header = API_KEY_HEADERS.get(provider)
    auth: ProviderAuth = (
        {"api_key_header": {"header": header, "credential": name}}
        if header
        else {"bearer": {"credential": name}}
    )
    declared = deployed_models(provider)
    deployment: ProviderDeployment = {
        "name": provider,
        "adapter": provider,
        "auth": auth,
        "models": [native for native, _ in declared],
        "capabilities": list(dict.fromkeys(op for _, ops in declared for op in ops)),
    }
    Gateway(admin).put_deployment(deployment)


@pytest.fixture(scope="session")
def live_gateway() -> Iterator[LiveGateway]:
    """One live server with a deployment per keyed provider, plus one caller token.

    The sequence is the production one: the tenant administrator submits each
    operator key as a managed secret and declares its deployment, and a
    separate non-admin principal that may invoke exactly the deployed models
    receives the token, so a submitted key is never readable by the identity
    that calls the provider. A provider without a key gets no deployment, and
    its tests skip on their own ``credential`` check.
    """
    keyed = {provider: key for provider in PROVIDERS if (key := secret(provider))}
    if not keyed:
        # ty does not yet read pytest.skip's signature through its exception decorator.
        pytest.skip("gateway smoke skipped: no provider credential is set")  # ty: ignore[too-many-positional-arguments]
    with WyrdTestServer(mutate_env=False, live_providers=True) as server:
        admin = WyrdClient(server_url=server.base_url, credential=server.api_key)
        for provider, key in keyed.items():
            deploy(admin, provider, key)
        caller = server.scoped_api_key(
            "live_invoker",
            [
                {
                    "resource": "gateway",
                    "action": "invoke",
                    "scope": {"gateway": {"model": {"deployment": provider, "model": native}}},
                }
                for provider in keyed
                for native, _ in deployed_models(provider)
            ],
        )
        token = WyrdClient(server_url=server.base_url, credential=caller).access_token()
        yield LiveGateway(server, token)


# One Jev typed question of each form. The state carries quotes and a newline
# as text, which proves it travels as a string rather than spliced JSON.
JEV_QUESTIONS = {
    "polite": {"type": "noul", "instructions": "Is the reply polite?"},
    "tone": {
        "type": "choice",
        "instructions": "What is the tone of the reply?",
        "criteria": {"polite": "courteous", "rude": "curt"},
    },
    "quality": {
        "type": "score",
        "instructions": "How good is the reply?",
        "criteria": ["poor", "ok", "great"],
    },
}
JEV_STATE = 'Thank you for waiting; "your refund" has been issued.\nAnything else?'


def assert_jev_answers(answers: dict) -> None:
    """Assert ``answers`` holds one typed Jev answer for each of ``JEV_QUESTIONS``."""
    assert 0.0 <= answers["polite"]["noul"] <= 1.0
    assert answers["tone"]["choice"] in {"polite", "rude"}
    assert answers["quality"]["type"] == "score"
