"""``WyrdMiddleware`` in real ASGI applications against a real server.

The ``observed-service`` state starts and drains with the application's
lifespan, each request observes the ``model`` Card under its own Run, and the
drift rows those Runs submit, or the spans the application's own OpenTelemetry
instrumentation records, are read back from Bifrost.
"""

from __future__ import annotations

import asyncio
import logging
import os
import runpy
import signal
import socket
import subprocess
import sys
import time
from collections.abc import AsyncGenerator, AsyncIterator, Iterator, MutableMapping
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

import httpx
import pytest
from fastapi import FastAPI, Request, WebSocket
from fastapi.responses import StreamingResponse
from fastapi.testclient import TestClient
from opentelemetry import trace
from opentelemetry.instrumentation.asgi import OpenTelemetryMiddleware
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.integrations.asgi import RUN_SCOPE_KEY, WyrdMiddleware
from wyrd.otel import _EXPORTER_EXTRA, install_run_correlation, span_exporter
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import hydrated, reset_telemetry_providers, stale_bundle, timeline

pytestmark = pytest.mark.integration

SDK_ROOT = Path(__file__).resolve().parents[2]
"""The Python SDK root, from which the worker application imports."""

QUICKSTART = SDK_ROOT.parents[1] / "examples/quickstart"
FASTAPI_EXAMPLE = SDK_ROOT.parents[1] / "examples/integrate-fastapi"


def model_alias(scope: MutableMapping[str, Any]) -> str:
    """Every request observes the ``model`` Card, which ``observed-drift`` is bound to."""
    return "model"


def submit(run: Any, latency: float) -> str:
    """Submit one drift sample on ``run`` and return its ``run_id``."""
    run.observe.submit("observed-drift", {"latency": latency})
    return run.run_id


def drift(server: WyrdTestServer, bifrost: Bifrost, run_ids: list[str]) -> dict[str, set]:
    """Publish captured rows, then map each of ``run_ids`` to its ``(latency, subject_card_uid)`` rows."""
    server.flush_bifrost()
    placeholders = ", ".join(f"${index}" for index in range(1, len(run_ids) + 1))
    rows = bifrost.sql(
        "SELECT run_id, num_value, subject_card_uid FROM bifrost.drift.observations "
        f"WHERE series = 'latency' AND run_id IN ({placeholders})",
        run_ids,
    )
    stored: dict[str, set] = {}
    for row in rows.to_arrow().to_pylist():
        stored.setdefault(row["run_id"], set()).add((row["num_value"], row["subject_card_uid"]))
    return stored


@asynccontextmanager
async def lifespan(app: Any) -> AsyncGenerator[list[dict[str, Any]]]:
    """Drive ``app``'s lifespan as a server does, collecting its replies.

    Startup runs on entry; shutdown runs on exit only when startup completed.
    The list holds the startup reply and, after exit, the shutdown reply, then
    ``{"type": "raised", "error": ...}`` when the application raised.
    """
    inbox: asyncio.Queue[dict[str, Any]] = asyncio.Queue()
    outbox: asyncio.Queue[dict[str, Any]] = asyncio.Queue()
    await inbox.put({"type": "lifespan.startup"})
    scope = {"type": "lifespan", "asgi": {"version": "3.0"}, "state": {}}
    task = asyncio.create_task(app(scope, inbox.get, outbox.put))
    # A task that dies without replying fails the test instead of hanging it.
    replied = asyncio.create_task(outbox.get())
    await asyncio.wait([task, replied], return_when=asyncio.FIRST_COMPLETED)
    assert replied.done(), f"the application ended without replying: {task.exception()!r}"
    replies = [replied.result()]
    try:
        yield replies
    finally:
        if replies[0]["type"] == "lifespan.startup.complete":
            await inbox.put({"type": "lifespan.shutdown"})
            # An application may raise during shutdown instead of replying.
            reply = asyncio.create_task(outbox.get())
            await asyncio.wait([task, reply], return_when=asyncio.FIRST_COMPLETED)
            reply.cancel()
            await asyncio.wait([reply])
            if not reply.cancelled():
                replies.append(reply.result())
            elif not outbox.empty():
                replies.append(outbox.get_nowait())
        await asyncio.wait([task])
        if task.cancelled():
            replies.append({"type": "raised", "error": asyncio.CancelledError()})
        elif task.exception() is not None:
            replies.append({"type": "raised", "error": task.exception()})


def cycle(app: Any) -> list[dict[str, Any]]:
    """Run ``app``'s whole lifespan with nothing served in between."""

    async def run() -> list[dict[str, Any]]:
        async with lifespan(app) as replies:
            pass
        return replies

    return asyncio.run(run())


SECRET = "postgres://wyrd:hunter2@db/wyrd token=sk-live-1\nforged log line"
"""An exception body Wyrd must never copy into a message or log it creates."""


class Faulty:
    """``state``, except that each of ``names`` runs and then raises ``error``."""

    def __init__(self, state: WyrdState, error: BaseException, *names: str) -> None:
        self.state, self.error, self.names = state, error, names

    def __getattr__(self, name: str) -> Any:
        method = getattr(self.state, name)
        if name not in self.names:
            return method

        def faulty(*args: Any) -> Any:
            method(*args)
            raise self.error

        return faulty


def raw(state: Any, events: list[str], error: BaseException | None = None) -> WyrdMiddleware:
    """A plain ASGI lifespan that starts, records evidence during shutdown, then raises ``error``.

    Without ``error`` it reads startup and raises before replying.
    """

    async def application(scope: Any, receive: Any, send: Any) -> None:
        await receive()
        if error is None:
            raise RuntimeError(SECRET)
        await send({"type": "lifespan.startup.complete"})
        await receive()
        events.append(submit(state.run("model"), 8.0))
        raise error

    return WyrdMiddleware(application, state=state, telemetry=False)


def closed(state: WyrdState) -> bool:
    """Whether ``state`` has been shut down: its writer can no longer start."""
    try:
        state.start_bifrost()
    except WyrdError as error:
        return error.code == "WYRD_SDK_409_BIFROST_CLOSED"
    state.shutdown()
    return False


@pytest.mark.usefixtures("fresh_telemetry_providers")
def test_asgi_lifespan(
    observed_bundle: Path,
    observed_key: str,
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
    cards: Cards,
    monkeypatch: pytest.MonkeyPatch,
    caplog: pytest.LogCaptureFixture,
    tmp_path: Path,
) -> None:
    stale = stale_bundle(observed_bundle, cards, tmp_path)

    def fresh() -> WyrdState:
        return hydrated(observed_bundle, WyrdClient(credential=observed_key))

    def app(state: Any, events: list[str], refuse: bool = False, **options: Any) -> FastAPI:
        @asynccontextmanager
        async def application_lifespan(_: FastAPI) -> AsyncGenerator[None]:
            events.append("startup")
            if refuse:
                raise RuntimeError("the application refused to start")
            yield
            events.append(submit(state.run("model"), 7.0))

        api = FastAPI(lifespan=application_lifespan)
        api.add_middleware(WyrdMiddleware, state=state, **options)
        return api

    # Startup starts the writer and telemetry before the application starts;
    # the application's shutdown evidence is drained after it.
    reset_telemetry_providers(monkeypatch)
    state, events = fresh(), []
    replies = cycle(app(state, events))
    assert [reply["type"] for reply in replies] == [
        "lifespan.startup.complete",
        "lifespan.shutdown.complete",
    ]
    assert events[0] == "startup"
    model = str(state.card_ref("model").uid)
    assert drift(wyrd_server, bifrost, [events[1]]) == {events[1]: {(7.0, model)}}
    assert closed(state)

    # A writer failure fails startup with its code; the application never starts.
    started, events = fresh(), []
    started.start_bifrost()
    [failed] = cycle(app(started, events, telemetry=False))
    assert failed["type"] == "lifespan.startup.failed"
    assert failed["message"].startswith("WYRD_SDK_409_BIFROST_ALREADY_STARTED: ")
    assert events == []
    started.shutdown()

    # An application-owned provider refuses default telemetry, the started
    # writer is closed, and the application's provider stays installed.
    reset_telemetry_providers(monkeypatch)
    owned = TracerProvider()
    trace.set_tracer_provider(owned)
    state, events = fresh(), []
    [failed] = cycle(app(state, events))
    assert failed["type"] == "lifespan.startup.failed"
    assert failed["message"].startswith("WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS: ")
    assert events == [] and closed(state)
    assert trace.get_tracer_provider() is owned

    # The application's own startup failure stays its failure, and Wyrd drains.
    state, events = fresh(), []
    [failed, raised] = cycle(app(state, events, telemetry=False, refuse=True))
    assert failed["type"] == "lifespan.startup.failed"
    assert "the application refused to start" in failed["message"]
    assert str(raised["error"]) == "the application refused to start"
    assert events == ["startup"] and closed(state)

    # A failed drain is logged with its code and reported as shutdown failure.
    agent = hydrated(stale, WyrdClient())
    events = []
    with caplog.at_level(logging.ERROR, logger="wyrd.integrations.asgi"):
        replies = cycle(app(agent, events, telemetry=False))
    assert replies[0]["type"] == "lifespan.startup.complete"
    assert replies[1]["type"] == "lifespan.shutdown.failed"
    assert replies[1]["message"].startswith("WYRD_VALA_403_BIFROST_CARD_SCOPE: ")
    assert "WYRD_VALA_403_BIFROST_CARD_SCOPE" in caplog.text

    # An application that raises during shutdown keeps its error, and its
    # final evidence is still drained; so does a cancelled one.
    for error in (RuntimeError(SECRET), asyncio.CancelledError()):
        state, events = fresh(), []
        replies = cycle(raw(state, events, error))
        assert [reply["type"] for reply in replies] == ["lifespan.startup.complete", "raised"]
        assert type(replies[1]["error"]) is type(error)
        assert drift(wyrd_server, bifrost, events) == {events[0]: {(8.0, model)}}
        assert closed(state)

    # A failed drain then is logged, and the application's error stays primary.
    caplog.clear()
    agent, events = hydrated(stale, WyrdClient()), []
    with caplog.at_level(logging.ERROR, logger="wyrd.integrations.asgi"):
        replies = cycle(raw(agent, events, RuntimeError("primary")))
    assert str(replies[1]["error"]) == "primary"
    assert "WYRD_VALA_403_BIFROST_CARD_SCOPE" in caplog.text

    # Wyrd's own messages and logs name an unexpected error's type, never its body.
    caplog.clear()
    with caplog.at_level(logging.WARNING, logger="wyrd.integrations.asgi"):
        state = fresh()
        [failed] = cycle(raw(state, []))
        assert failed["type"] == "lifespan.startup.failed" and closed(state)
        faulty = Faulty(fresh(), RuntimeError(SECRET), "shutdown")
        replies = cycle(app(faulty, [], telemetry=False))
        assert replies[1]["type"] == "lifespan.shutdown.failed"
        reset_telemetry_providers(monkeypatch)
        trace.set_tracer_provider(TracerProvider())
        [cleanup] = cycle(app(Faulty(fresh(), RuntimeError(SECRET), "shutdown"), []))
    messages = [failed["message"], replies[1]["message"], cleanup["message"], caplog.text]
    assert all("RuntimeError" in text and "hunter2" not in text for text in messages[:2])
    assert not any(
        secret in text for text in messages for secret in ("hunter2", "sk-live", "forged")
    )
    assert cleanup["message"].startswith("WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS: ")

    # The missing otel extra keeps its install remedy.
    reset_telemetry_providers(monkeypatch)
    [failed] = cycle(app(Faulty(fresh(), ImportError(_EXPORTER_EXTRA), "start_telemetry"), []))
    assert failed["message"] == _EXPORTER_EXTRA


def test_asgi_run_isolation(
    observed_with_bifrost: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    state = observed_with_bifrost
    model = str(state.card_ref("model").uid)
    gate = asyncio.Barrier(5)
    entered = asyncio.Event()
    served: list[str] = []
    api = FastAPI()

    @api.get("/observe/{latency}")
    async def observe(latency: float, request: Request) -> dict[str, str]:
        run = request.scope[RUN_SCOPE_KEY]
        served.append(run.run_id)
        await gate.wait()
        return {"run_id": submit(run, latency)}

    @api.get("/stream")
    async def stream(request: Request) -> StreamingResponse:
        run = request.scope[RUN_SCOPE_KEY]

        async def body() -> AsyncIterator[str]:
            yield run.run_id
            submit(run, 50.0)

        return StreamingResponse(body())

    @api.get("/fail")
    async def fail() -> None:
        raise RuntimeError("the handler failed")

    @api.get("/hang")
    async def hang() -> None:
        entered.set()
        await asyncio.Event().wait()

    @api.websocket("/ws")
    async def socket_route(websocket: WebSocket) -> None:
        await websocket.accept()
        run = websocket.scope[RUN_SCOPE_KEY]
        await websocket.send_text(run.run_id)
        submit(run, float(await websocket.receive_text()))
        await websocket.close()

    wrapped = WyrdMiddleware(api, state=state, alias=model_alias)
    refusing = WyrdMiddleware(api, state=state, alias=lambda scope: "absent")
    tracer_provider = TracerProvider()
    spans = InMemorySpanExporter()
    tracer_provider.add_span_processor(SimpleSpanProcessor(spans))
    install_run_correlation(tracer_provider)

    def outside_any_run() -> bool:
        tracer_provider.get_tracer("isolation").start_span("after").end()
        return "wyrd.run_id" not in (spans.get_finished_spans()[-1].attributes or {})

    async def requests() -> tuple[list[str], str]:
        transport = httpx.ASGITransport(app=wrapped)
        async with httpx.AsyncClient(transport=transport, base_url="http://app") as client:
            answers = await asyncio.gather(
                *(client.get(f"/observe/{latency}") for latency in range(1, 6))
            )
            streamed = await client.get("/stream")
            with pytest.raises(RuntimeError, match="the handler failed"):
                await client.get("/fail")
            assert outside_any_run()
            hanging = asyncio.create_task(client.get("/hang"))
            await entered.wait()
            hanging.cancel()
            with pytest.raises(asyncio.CancelledError):
                await hanging
        refused_transport = httpx.ASGITransport(app=refusing)
        async with httpx.AsyncClient(transport=refused_transport, base_url="http://app") as client:
            with pytest.raises(WyrdError) as refused:
                await client.get("/observe/9")
        assert refused.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
        assert outside_any_run()
        return [answer.json()["run_id"] for answer in answers], streamed.text

    concurrent, streamed = asyncio.run(requests())
    assert len(served) == 5, "the refused alias never reached the application"
    assert sorted(served) == sorted(concurrent) and len(set(concurrent)) == 5

    # Without ``with``, the test client sends no lifespan; the state is already started.
    sockets, client = [], TestClient(wrapped)
    for latency in ("60", "70"):
        with client.websocket_connect("/ws") as websocket:
            sockets.append(websocket.receive_text())
            websocket.send_text(latency)
    assert len(set(sockets)) == 2

    passed: list[MutableMapping[str, Any]] = []

    async def other_scope() -> None:
        async def app(scope: MutableMapping[str, Any], receive: Any, send: Any) -> None:
            passed.append(scope)

        async def unused(*_: Any) -> Any:
            raise AssertionError("a passed-through scope reaches the app unchanged")

        await WyrdMiddleware(app, state=state)({"type": "custom"}, unused, unused)

    asyncio.run(other_scope())
    assert passed == [{"type": "custom"}]

    state.flush()
    run_ids = [*concurrent, streamed, *sockets]
    assert drift(wyrd_server, bifrost, run_ids) == {
        **{run_id: {(float(latency), model)} for latency, run_id in enumerate(concurrent, 1)},
        streamed: {(50.0, model)},
        sockets[0]: {(60.0, model)},
        sockets[1]: {(70.0, model)},
    }


@pytest.mark.usefixtures("fresh_telemetry_providers")
def test_asgi_telemetry_ownership(
    observed_bundle: Path,
    observed_key: str,
    wyrd_server: WyrdTestServer,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    def app(state: WyrdState, **options: Any) -> FastAPI:
        api = FastAPI()

        @api.get("/work")
        async def work(request: Request) -> dict[str, str]:
            context = trace.get_current_span().get_span_context()
            return {
                "run_id": request.scope[RUN_SCOPE_KEY].run_id,
                "trace": f"{context.trace_id:032x}",
            }

        # Added last, Wyrd is outermost: the instrumentation's spans start inside the Run.
        api.add_middleware(OpenTelemetryMiddleware)
        api.add_middleware(WyrdMiddleware, state=state, **options)
        return api

    async def serve(api: FastAPI) -> dict[str, str]:
        async with lifespan(api):
            transport = httpx.ASGITransport(app=api)
            async with httpx.AsyncClient(transport=transport, base_url="http://app") as client:
                return (await client.get("/work")).json()

    # Default telemetry: the state's provider exports the instrumentation's
    # spans, and shutdown flushes them before the drain.
    state = hydrated(observed_bundle, WyrdClient(credential=observed_key))
    root = str(state.root_ref.uid)
    served = asyncio.run(serve(app(state)))
    spans = timeline(wyrd_server, served["trace"])
    [server_span] = [span for span in spans if span["kind"] == "server"]
    assert server_span["name"] == "GET /work"
    assert {span["attributes"].get("wyrd.run_id") for span in spans} == {served["run_id"]}
    assert {span["attributes"].get("wyrd.card_uid") for span in spans} == {root}

    # telemetry=False: the application installs, exports, and flushes its own provider.
    reset_telemetry_providers(monkeypatch)
    client = WyrdClient(credential=observed_key).for_workload(root)
    owned, local = TracerProvider(), InMemorySpanExporter()
    owned.add_span_processor(SimpleSpanProcessor(local))
    owned.add_span_processor(SimpleSpanProcessor(span_exporter(client)))
    install_run_correlation(owned)
    trace.set_tracer_provider(owned)
    state = hydrated(observed_bundle, client)
    served = asyncio.run(serve(app(state, telemetry=False)))
    assert trace.get_tracer_provider() is owned
    finished = local.get_finished_spans()
    assert [span.name for span in finished if span.kind == trace.SpanKind.SERVER] == ["GET /work"]
    assert {(span.attributes or {}).get("wyrd.run_id") for span in finished} == {served["run_id"]}
    stored = timeline(wyrd_server, served["trace"])
    assert {span["attributes"].get("wyrd.run_id") for span in stored} == {served["run_id"]}
    owned.shutdown()


@pytest.mark.usefixtures("fresh_telemetry_providers")
def test_fastapi_example(
    wyrd_server: WyrdTestServer, bifrost: Bifrost, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    # The FastAPI guide's app, beside the Quickstart Service pulled as `.wyrd-bundle`.
    WyrdState.declare(str(QUICKSTART / "support-reply.yaml"), bundle_dir=tmp_path / ".wyrd-bundle")
    monkeypatch.chdir(tmp_path)
    example = runpy.run_path(str(FASTAPI_EXAMPLE / "app.py"))
    served = {}
    with TestClient(example["app"]) as client:
        for answer in ("Your refund request is approved.", "You have a guaranteed refund."):
            body = client.post("/replies", json={"answer": answer}).json()
            served[body["run_id"]] = body["verdict"]
    assert sorted(served.values()) == ["failed", "passed"]
    assert closed(example["state"])
    wyrd_server.flush_bifrost()
    rows = bifrost.sql(
        "SELECT run_id, verdict FROM bifrost.verification.results WHERE run_id IN ($1, $2)",
        list(served),
    )
    assert {row["run_id"]: row["verdict"] for row in rows.to_arrow().to_pylist()} == served


def free_port() -> int:
    """Ask the OS for a free loopback port."""
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


@pytest.fixture
def worker_env(
    observed_bundle: Path, observed_key: str, tmp_path: Path
) -> Iterator[dict[str, str]]:
    """The worker application's environment: server URL and Service key only, nothing saved."""
    env = {
        name: value
        for name, value in os.environ.items()
        if name not in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT")
    }
    env.update(
        WYRD_CONFIG_HOME=str(tmp_path),
        WYRD_API_KEY=observed_key,
        WYRD_TEST_BUNDLE=str(observed_bundle),
        PYTHONPATH=os.pathsep.join(sys.path),
    )
    yield env


def test_asgi_worker_lifecycle(
    worker_env: dict[str, str], wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    port = free_port()
    workers = subprocess.Popen(
        [
            *(sys.executable, "-m", "gunicorn", "tests.integration.asgi_app:app"),
            *("--workers", "2", "--worker-class", "uvicorn.workers.UvicornWorker"),
            *("--bind", f"127.0.0.1:{port}", "--graceful-timeout", "30"),
        ],
        cwd=SDK_ROOT,
        env=worker_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    served: dict[int, list[str]] = {}
    try:
        deadline = time.monotonic() + 60
        latency = 0.0
        while len(served) < 2 or min(map(len, served.values())) < 2:
            assert time.monotonic() < deadline, f"two workers did not both serve: {served}"
            try:
                latency += 1
                answer = httpx.get(f"http://127.0.0.1:{port}/observe/{latency}", timeout=5)
            except httpx.TransportError:
                time.sleep(0.2)
                continue
            assert answer.status_code == 200, answer.text
            body = answer.json()
            served.setdefault(body["pid"], []).append(body["run_id"])
    finally:
        workers.send_signal(signal.SIGTERM)
        output, _ = workers.communicate(timeout=60)
    assert workers.returncode == 0, output
    assert output.count("Application shutdown complete") == 2, output
    run_ids = [run_id for runs in served.values() for run_id in runs]
    assert set(drift(wyrd_server, bifrost, run_ids)) == set(run_ids), output
