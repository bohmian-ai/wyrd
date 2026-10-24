"""``WyrdClient.http_auth`` swaps provider keys for a fresh Wyrd access token on every request."""

from __future__ import annotations

import asyncio
import itertools
from typing import TYPE_CHECKING, cast

import httpx
import httpx2
import pytest
from opentelemetry.sdk.trace import TracerProvider
from wyrd import WyrdClient, WyrdError
from wyrd.client._http_auth import HttpAuth
from wyrd.state import WyrdState

from tests.unit.state.support import TinyDataInterface, TinyModelInterface

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from wyrd.observe import Run

PLACEHOLDERS = {"Authorization": "Bearer wyrd", "x-api-key": "wyrd", "x-goog-api-key": "wyrd"}


class Minting:
    """A stand-in rootless client that mints a new token on each ask."""

    workload_uid = None

    def __init__(self) -> None:
        self._tokens = (f"token-{n}" for n in itertools.count(1))

    def access_token(self) -> str:
        return next(self._tokens)


def credentials(headers: Mapping[str, str]) -> str:
    """The credential headers a request reached the transport with."""
    names = ("authorization", "x-api-key", "x-goog-api-key", "x-wyrd-access-token")
    return repr({name: headers[name] for name in names if name in headers})


def test_http_auth_on_a_client_replaces_provider_keys_with_its_access_token() -> None:
    client = WyrdClient(server_url="http://127.0.0.1:9", credential="access-token-1")
    echo = httpx2.MockTransport(
        lambda request: httpx2.Response(200, text=credentials(request.headers))
    )
    with httpx2.Client(auth=client.http_auth, transport=echo, headers=PLACEHOLDERS) as http:
        sent = http.get("http://wyrd/v1/models").text
    assert sent == repr({"x-wyrd-access-token": "Bearer access-token-1"})


def test_each_httpx_request_carries_a_fresh_token() -> None:
    echo = httpx.MockTransport(
        lambda request: httpx.Response(200, text=credentials(request.headers))
    )
    auth = HttpAuth(cast("WyrdClient", Minting()))
    with httpx.Client(auth=auth, transport=echo, headers=PLACEHOLDERS) as http:
        sent = [http.get("http://wyrd/mcp").text for _ in range(2)]
    assert sent == [repr({"x-wyrd-access-token": f"Bearer token-{n}"}) for n in (1, 2)]


def test_async_requests_carry_a_fresh_token() -> None:
    echo = httpx2.MockTransport(
        lambda request: httpx2.Response(200, text=credentials(request.headers))
    )

    async def send_twice() -> list[str]:
        auth = HttpAuth(cast("WyrdClient", Minting()))
        async with httpx2.AsyncClient(auth=auth, transport=echo, headers=PLACEHOLDERS) as http:
            return [(await http.get("http://wyrd/v1/models")).text for _ in range(2)]

    assert asyncio.run(send_twice()) == [
        repr({"x-wyrd-access-token": f"Bearer token-{n}"}) for n in (1, 2)
    ]


CONTEXT = (
    "x-wyrd-access-token",
    "x-wyrd-workload-uid",
    "x-wyrd-deployment-environment",
    "wyrd-card-uid",
    "wyrd-run-id",
    "traceparent",
)


def context(headers: Mapping[str, str]) -> dict[str, str]:
    """The Wyrd context headers a request reached the transport with."""
    return {name: headers[name] for name in CONTEXT if name in headers}


def test_state_auth_snapshots_context_before_token_refresh(fixtures_dir: Path) -> None:
    def configured(environment: str) -> WyrdState:
        state = WyrdState.from_path(
            fixtures_dir / "bundles" / "complete",
            WyrdClient(server_url="http://127.0.0.1:9", credential="access-token-1"),
            interfaces={
                "model": TinyModelInterface(),
                "backup": TinyModelInterface(),
                "training_data": TinyDataInterface(),
            },
        )
        state.configure(deployment_environment=environment)
        return state

    prod, staging = configured("prod"), configured("staging")
    service = prod.root_ref.uid
    agent = prod.card_ref("agent_triage").uid
    assert service is not None and agent is not None
    prod_auth, staging_auth = prod.http_auth, staging.http_auth
    sent: list[dict[str, str]] = []

    def record(request: httpx.Request | httpx2.Request) -> None:
        sent.append(context(request.headers))

    httpx_echo = httpx.MockTransport(lambda r: (record(r), httpx.Response(200))[1])
    httpx2_echo = httpx2.MockTransport(lambda r: (record(r), httpx2.Response(200))[1])
    plain = {"x-wyrd-access-token": "Bearer access-token-1", "x-wyrd-workload-uid": service}

    with httpx.Client(auth=prod_auth, transport=httpx_echo, headers=PLACEHOLDERS) as http:
        http.get("http://wyrd/v1/models")
    assert sent.pop() == {
        **plain,
        "x-wyrd-deployment-environment": "prod",
        "wyrd-card-uid": service,
    }, "outside a Run the Service is the subject and no Run is sent"

    # A Run of another state nested inside, and a Run entered while the token
    # is fetched, never relabel the request that already started.
    later = prod.run("agent_triage")
    refreshing = HttpAuth(cast("WyrdClient", Refreshing(later)), prod_auth._state)
    with (
        prod.run("agent_triage") as run,
        staging.run(),
        httpx2.Client(auth=refreshing, transport=httpx2_echo, headers=PLACEHOLDERS) as http,
    ):
        http.get("http://wyrd/v1/models")
        later.__exit__(None, None, None)
    assert sent.pop() == {
        "x-wyrd-access-token": "Bearer token-1",
        "x-wyrd-workload-uid": service,
        "x-wyrd-deployment-environment": "prod",
        "wyrd-card-uid": agent,
        "wyrd-run-id": run.run_id,
    }

    async def concurrent() -> None:
        async def call(auth: HttpAuth, state: WyrdState) -> None:
            with state.run("agent_triage"):
                async with httpx2.AsyncClient(auth=auth, transport=httpx2_echo) as http:
                    await http.get("http://wyrd/v1/models")

        await asyncio.gather(call(prod_auth, prod), call(staging_auth, staging))

    asyncio.run(concurrent())
    assert sorted(request["x-wyrd-deployment-environment"] for request in sent) == [
        "prod",
        "staging",
    ]
    assert len({request["wyrd-run-id"] for request in sent}) == 2
    sent.clear()

    explicit = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
    with (
        TracerProvider().get_tracer("test").start_as_current_span("call"),
        httpx.Client(auth=prod_auth, transport=httpx_echo) as http,
    ):
        http.get("http://wyrd/v1/models")
        http.get("http://wyrd/v1/models", headers={"traceparent": explicit})
    assert sent[0]["traceparent"].startswith("00-"), "the active trace is propagated"
    assert sent[1]["traceparent"] == explicit, "an explicit parent is never overridden"
    sent.clear()

    with httpx.Client(auth=prod_auth, transport=httpx_echo) as http:
        http.get("http://wyrd/v1/models", headers={"wyrd-card-uid": service})
        for conflict in (
            {"wyrd-card-uid": agent},
            {"wyrd-run-id": run.run_id},
            {"x-wyrd-workload-uid": agent},
        ):
            with pytest.raises(WyrdError) as refused:
                http.get("http://wyrd/v1/models", headers=conflict)
            assert refused.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
        http.get("http://wyrd/v1/models", headers={"x-wyrd-deployment-environment": "staging"})
    assert len(sent) == 2, "a matching identity is accepted; an identity conflict sends nothing"
    assert sent[1]["x-wyrd-deployment-environment"] == "prod", (
        "a differing tag is never a conflict; the configured tag is sent"
    )


class Refreshing:
    """A stand-in rootless client that enters ``run`` while it mints its token."""

    workload_uid = None

    def __init__(self, run: Run) -> None:
        self._run = run
        self._tokens = Minting()

    def access_token(self) -> str:
        self._run.__enter__()
        return self._tokens.access_token()
