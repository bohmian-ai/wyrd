"""Scoped observation surface reachable from an offline ``WyrdState``.

These tests stay server-free: opening a run performs no network IO, and every
refusal happens before admission. Accepted observations belong to the gated
journey lanes, not here.
"""

import asyncio
import importlib
import json
import sys
import threading
import time
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, cast
from uuid import UUID

import pytest
import wyrd
import wyrd.observe
import wyrd.otel
from opentelemetry import context as otel_context
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter
from wyrd.client import WyrdClient
from wyrd.observe import Observe, Run
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState
from wyrd.verify import ArtifactRef, Execution, File


def test_run_targets_the_root_service_with_a_uuidv7_identity(state: WyrdState) -> None:
    """One run is one invocation anchored on the bundle's root Service Card."""
    run = state.run()
    assert isinstance(run, Run)
    assert UUID(run.run_id).version == 7
    assert run.alias == "root"
    assert isinstance(run.observe, Observe)


def test_scoped_views_share_one_invocation_and_keep_their_subjects(state: WyrdState) -> None:
    """``for_card`` returns immutable siblings under one invocation identity."""
    run = state.run()
    model = run.for_card("model")
    backup = run.for_card("backup")
    assert model.run_id == run.run_id == backup.run_id
    assert (run.alias, model.alias, backup.alias) == ("root", "model", "backup")


def test_each_run_is_its_own_invocation(state: WyrdState) -> None:
    """Two runs from one state are distinct invocations."""
    assert state.run().run_id != state.run().run_id


def test_unknown_alias_is_refused(state: WyrdState) -> None:
    """An alias the bundle does not register fails locally."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().for_card("missing")
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"


def test_record_refuses_a_table_outside_vala_datasets(state: WyrdState) -> None:
    """``record`` writes registered caller-owned tables only."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.record("vala.drift.observations", {"value": 1})
    assert raised.value.code == "WYRD_SDK_400_INVALID_OBSERVATION"


def test_flush_requires_a_started_writer(state: WyrdState) -> None:
    """An explicit drain of a state that never started Bifrost is an error."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.flush()
    assert raised.value.code == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_shutdown_without_startup_succeeds(state: WyrdState) -> None:
    """Graceful shutdown is idempotent: a never-started state has no rows."""
    assert state.shutdown() is None
    assert state.shutdown() is None


@pytest.mark.parametrize("key", [1, 1.5, True, None])
def test_record_refuses_a_non_string_mapping_key(state: WyrdState, key: object) -> None:
    """``json.dumps`` would stringify the key, so it is refused before the writer."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.record("vala.datasets.latency", {key: 1.0})
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


# ── Initial Card selection ──────────────────────────────────────────────────


def test_run_alias_opens_on_that_card(state: WyrdState) -> None:
    """``run("model")`` opens a fresh invocation whose first view is ``model``."""
    model = state.run("model")
    assert model.alias == "model"
    assert UUID(model.run_id).version == 7


def test_views_from_an_alias_run_share_its_invocation(state: WyrdState) -> None:
    """A sibling view taken from ``run("model")`` keeps its run id."""
    model = state.run("model")
    assert model.for_card("backup").run_id == model.run_id


def test_run_without_an_alias_opens_on_the_root_service(state: WyrdState) -> None:
    """``run(None)`` is the same as ``run()``: the root Service view."""
    assert state.run(None).alias == "root"


def test_run_alias_refuses_an_unknown_alias(state: WyrdState) -> None:
    """An unknown initial alias fails locally, before any scope is entered."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run("missing")
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"


def test_verify_refuses_an_unbound_verifier_before_any_network_call(state: WyrdState) -> None:
    """A Verifier name not bound to the view's subject fails locally."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run("model").observe.verify("missing", {"question": "q"})
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_VERIFIER"


@pytest.mark.parametrize(
    ("args", "kwargs"),
    [
        (({"answer": "yes"},), {"answer": "yes"}),
        (("one", "two"), {}),
        ((float("nan"),), {}),
        ((), {"answer": object()}),
    ],
    ids=["positional-and-keyword", "two-positional", "nan", "unrepresentable"],
)
def test_verify_and_submit_refuse_unconvertible_inputs_before_any_lookup(
    state: WyrdState, args: tuple[object, ...], kwargs: dict[str, Any]
) -> None:
    """Python values convert before the Verifier is resolved, so even an unbound name
    reports the input refusal."""
    observe = state.run().observe
    for call in (observe.verify, observe.submit):
        with pytest.raises(wyrd.WyrdError) as raised:
            call("missing", *args, **kwargs)
        assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


def test_submit_refuses_an_unbound_verifier_before_any_network_call(state: WyrdState) -> None:
    """``submit`` resolves the bound Verifier locally, like ``verify``."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run("model").observe.submit("missing", answer="yes", policy=File("policy.pdf"))
    assert raised.value.code == "WYRD_SDK_404_UNKNOWN_VERIFIER"


def test_file_construction_reads_nothing(tmp_path: Path) -> None:
    """A ``File`` names evidence lazily: a missing path, bytes, and ``PathLike`` all
    construct without IO; only an unknown source type or format is refused."""
    File(tmp_path / "absent.pdf")
    File(str(tmp_path / "absent"), format="pdf", filename="policy.pdf")
    File(b"%PDF-1.7", format="pdf")
    for bad in ({"source": 1}, {"source": b"x", "format": "not-a-format"}):
        with pytest.raises(wyrd.WyrdError) as raised:
            File(**cast(Any, bad))
        assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


def test_artifact_ref_names_a_registered_artifact_file() -> None:
    """``ArtifactRef`` takes the exact Artifact reference and an optional path."""
    artifact = wyrd.CardRef("Artifact", "decks", "1.0.0")
    ArtifactRef(artifact)
    ArtifactRef(artifact, "q3.pdf")


def test_execution_connect_refuses_a_malformed_id_before_resolving_a_client() -> None:
    """An execution ID is parsed locally before any client or network work."""
    with pytest.raises(wyrd.WyrdError) as raised:
        Execution.connect("not-an-execution-id")
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


EXECUTION_ID = "0190b2a4-0000-7000-8000-000000000000"


class _ExecutionServer(BaseHTTPRequestHandler):
    """A local Wyrd server for one execution: it refuses the API-key exchange
    while ``refuse`` is set, answers it ``exchange_seconds`` late, and answers
    the status read with ``running``."""

    refuse = True
    exchange_seconds = 0.0
    requests: list[str] = []

    def do_POST(self) -> None:  # noqa: N802 - http.server's handler name
        self.rfile.read(int(self.headers.get("content-length", 0)))
        type(self).requests.append(self.path)
        time.sleep(type(self).exchange_seconds)
        if type(self).refuse:
            self._send(401, {"code": "WYRD_AUTH_401_UNAUTHENTICATED", "detail": "refused"})
        else:
            self._send(200, {"access_token": "t", "token_type": "Bearer", "expires_in": 3600})

    def do_GET(self) -> None:  # noqa: N802 - http.server's handler name
        type(self).requests.append(self.path)
        self._send(200, {"execution_id": EXECUTION_ID, "status": "running"})

    def _send(self, status: int, body: dict[str, object]) -> None:
        payload = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args: object) -> None:
        """Keep the test output quiet."""


@pytest.fixture
def execution_server() -> Iterator[str]:
    """Serve ``_ExecutionServer`` on a free port, refusing exchanges at first."""
    _ExecutionServer.refuse, _ExecutionServer.requests = True, []
    _ExecutionServer.exchange_seconds = 0.0
    server = ThreadingHTTPServer(("127.0.0.1", 0), _ExecutionServer)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    yield f"http://127.0.0.1:{server.server_address[1]}"
    server.shutdown()


def test_execution_constructs_offline_and_connects_on_first_read(execution_server: str) -> None:
    """``Execution(id, client)`` sends nothing until its first read, which
    exchanges the retained client's key; a refused first connection is
    retried by the next read, and a connected handle is reused."""
    execution = Execution(EXECUTION_ID, WyrdClient(execution_server, "wyrd_sk_t_v_s"))
    assert execution.id == EXECUTION_ID
    assert _ExecutionServer.requests == []

    with pytest.raises(wyrd.WyrdError):
        execution.status()
    assert _ExecutionServer.requests == ["/auth/token"]

    _ExecutionServer.refuse = False
    assert execution.status()["status"] == "running"
    assert execution.status()["status"] == "running"
    status = f"/v1/verification/executions/{EXECUTION_ID}"
    assert _ExecutionServer.requests == ["/auth/token", "/auth/token", status, status]


def test_execution_connect_exchanges_now(execution_server: str) -> None:
    """``Execution.connect`` connects eagerly: a refused exchange raises before
    any read, and an accepted one completes before it returns."""
    with pytest.raises(wyrd.WyrdError):
        Execution.connect(EXECUTION_ID, WyrdClient(execution_server, "wyrd_sk_t_v_s"))
    _ExecutionServer.refuse = False
    Execution.connect(EXECUTION_ID, WyrdClient(execution_server, "wyrd_sk_t_v_s"))
    assert _ExecutionServer.requests == ["/auth/token", "/auth/token"]


SLOW_EXCHANGE_SECONDS = 3.0
"""How late the slow exchange answers; far above every bounded wait below."""

WAIT_BUDGET_SECONDS = 0.2
"""The caller's wait budget, far below the slow exchange."""

WAIT_TIMEOUT = "WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT"


def test_execution_zero_wait_starts_no_connection(execution_server: str) -> None:
    """An expired budget times out before the first connection exchanges."""
    execution = Execution(EXECUTION_ID, WyrdClient(execution_server, "wyrd_sk_t_v_s"))
    with pytest.raises(wyrd.WyrdError) as raised:
        execution.wait(0)
    assert raised.value.code == WAIT_TIMEOUT
    assert _ExecutionServer.requests == []


def test_execution_first_wait_bounds_a_slow_exchange_and_retries_after(
    execution_server: str,
) -> None:
    """A slow first exchange loses to the wait budget without a status read,
    and the abandoned connection leaves the next read free to connect."""
    _ExecutionServer.refuse, _ExecutionServer.exchange_seconds = False, SLOW_EXCHANGE_SECONDS
    execution = Execution(EXECUTION_ID, WyrdClient(execution_server, "wyrd_sk_t_v_s"))
    started = time.monotonic()
    with pytest.raises(wyrd.WyrdError) as raised:
        execution.wait(WAIT_BUDGET_SECONDS)
    assert raised.value.code == WAIT_TIMEOUT
    assert time.monotonic() - started < SLOW_EXCHANGE_SECONDS / 2
    assert _ExecutionServer.requests == ["/auth/token"]

    _ExecutionServer.exchange_seconds = 0.0
    assert execution.status()["status"] == "running"
    assert execution.status()["status"] == "running"
    status = f"/v1/verification/executions/{EXECUTION_ID}"
    assert _ExecutionServer.requests == ["/auth/token", "/auth/token", status, status]


def test_execution_wait_behind_another_first_connection_keeps_its_budget(
    execution_server: str,
) -> None:
    """A wait queued behind another caller's slow first connection times out
    within its own budget instead of waiting for that connection."""
    _ExecutionServer.refuse, _ExecutionServer.exchange_seconds = False, SLOW_EXCHANGE_SECONDS
    execution = Execution(EXECUTION_ID, WyrdClient(execution_server, "wyrd_sk_t_v_s"))
    first = threading.Thread(target=execution.status)
    first.start()
    while _ExecutionServer.requests != ["/auth/token"]:
        time.sleep(0.01)
    started = time.monotonic()
    with pytest.raises(wyrd.WyrdError) as raised:
        execution.wait(WAIT_BUDGET_SECONDS)
    assert raised.value.code == WAIT_TIMEOUT
    assert time.monotonic() - started < SLOW_EXCHANGE_SECONDS / 2
    first.join()
    assert _ExecutionServer.requests == [
        "/auth/token",
        f"/v1/verification/executions/{EXECUTION_ID}",
    ]


def test_verification_types_are_exported_and_judge_surfaces_are_gone() -> None:
    """The verify/submit types live only in ``wyrd.verify``; Judgment, drift/eval
    emits, and the Eval media module are removed with no alias."""
    assert set(wyrd.verify.__all__) == {
        "ArtifactRef",
        "Execution",
        "File",
        "Recorded",
        "VerificationResult",
    }
    for name in wyrd.verify.__all__:
        assert not hasattr(wyrd, name)
        assert not hasattr(wyrd.observe, name)
    assert not hasattr(wyrd.observe, "Judgment")
    assert not hasattr(Observe, "drift")
    assert not hasattr(Observe, "eval")
    with pytest.raises(ModuleNotFoundError):
        importlib.import_module("wyrd.eval")


def test_the_verification_module_is_not_importable() -> None:
    """Direct judgment lives on ``run.observe.verify``; no handle module remains (AC-046)."""
    with pytest.raises(ModuleNotFoundError):
        __import__("wyrd.verification")


# ── Run scope: ambient OpenTelemetry span correlation ───────────────────────


@pytest.fixture
def spans(monkeypatch: pytest.MonkeyPatch) -> tuple[TracerProvider, InMemorySpanExporter]:
    """A real SDK provider standing in as the global one, exporting to memory.

    Patching the global lookup rather than calling ``set_tracer_provider``
    keeps each test's provider private: OpenTelemetry sets the global once.
    """
    provider = TracerProvider()
    exporter = InMemorySpanExporter()
    provider.add_span_processor(SimpleSpanProcessor(exporter))
    monkeypatch.setattr(trace, "get_tracer_provider", lambda: provider)
    return provider, exporter


def _correlation(exporter: InMemorySpanExporter) -> dict[str, tuple[str, str] | None]:
    """Map each finished span name to its Wyrd correlation, if any."""
    out: dict[str, tuple[str, str] | None] = {}
    for span in exporter.get_finished_spans():
        attributes = span.attributes or {}
        card_uid, run_id = attributes.get("wyrd.card_uid"), attributes.get("wyrd.run_id")
        out[span.name] = None if card_uid is None else (str(card_uid), str(run_id))
    return out


def _ref(state: WyrdState, view: Run) -> str:
    """The Card UID a view's spans carry."""
    return str(state.card_ref(view.alias).uid)


def test_entering_a_run_returns_it_and_correlates_active_and_child_spans(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """The active span and every span started in scope carry the exact identity."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run("model")
    with tracer.start_as_current_span("outer"):
        with run as entered:
            assert entered is run
            with tracer.start_as_current_span("child"):
                tracer.start_span("grandchild", attributes={"wyrd.card_uid": "x"}).end()
        tracer.start_span("after").end()
    expected = (_ref(state, run), run.run_id)
    assert _correlation(exporter) == {
        "grandchild": expected,
        "child": expected,
        "after": None,
        "outer": expected,
    }


def test_nested_card_scopes_share_the_run_and_restore_the_outer_card(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """An inner Card scope restores the outer Card when it exits."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run()
    model = run.for_card("model")
    with run:
        tracer.start_span("service").end()
        with model:
            tracer.start_span("model").end()
        tracer.start_span("restored").end()
    tracer.start_span("outside").end()
    assert _correlation(exporter) == {
        "service": (_ref(state, run), run.run_id),
        "model": (_ref(state, model), run.run_id),
        "restored": (_ref(state, run), run.run_id),
        "outside": None,
    }


def test_scope_survives_await_and_isolates_concurrent_tasks(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Context follows ``await`` and tasks; concurrent scopes never cross."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run()
    views = {"model": run.for_card("model"), "backup": run.for_card("backup")}

    async def scoped(alias: str) -> None:
        with views[alias]:
            await asyncio.sleep(0)
            tracer.start_span(f"task-{alias}").end()
            await asyncio.sleep(0)
            tracer.start_span(f"task-{alias}-late").end()

    async def main() -> None:
        with run:
            await asyncio.gather(scoped("model"), scoped("backup"))
            spawned = asyncio.create_task(_span_later(tracer, "spawned"))
        await spawned
        tracer.start_span("after").end()

    asyncio.run(main())
    root = (_ref(state, run), run.run_id)
    assert _correlation(exporter) == {
        "task-model": (_ref(state, views["model"]), run.run_id),
        "task-model-late": (_ref(state, views["model"]), run.run_id),
        "task-backup": (_ref(state, views["backup"]), run.run_id),
        "task-backup-late": (_ref(state, views["backup"]), run.run_id),
        "spawned": root,
        "after": None,
    }


def test_concurrent_tasks_entering_the_same_run_exit_independently(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Two tasks share one Run object; each exit clears only its own task's scope."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run("model")

    async def main() -> None:
        first_entered, second_entered, first_exited = (asyncio.Event() for _ in range(3))

        async def first() -> None:
            with run:
                first_entered.set()
                await second_entered.wait()
            tracer.start_span("first-exited").end()
            first_exited.set()

        async def second() -> None:
            await first_entered.wait()
            with run:
                second_entered.set()
                await first_exited.wait()
                tracer.start_span("second-entered").end()
            tracer.start_span("second-exited").end()

        await asyncio.gather(first(), second())

    asyncio.run(main())
    tracer.start_span("after").end()
    assert _correlation(exporter) == {
        "first-exited": None,
        "second-entered": (_ref(state, run), run.run_id),
        "second-exited": None,
        "after": None,
    }


def test_captured_otel_context_carries_the_scope_into_a_plain_thread(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """A thread attaching a context captured in scope stamps the scope's exact pair."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run("model")

    def worker(captured: Any) -> None:
        otel_context.attach(captured)
        tracer.start_span("threaded").end()

    with run:
        thread = threading.Thread(target=worker, args=(otel_context.get_current(),))
        thread.start()
        thread.join()
    assert _correlation(exporter) == {"threaded": (_ref(state, run), run.run_id)}


async def _span_later(tracer: Any, name: str) -> None:
    """Yield once, then start and end one span in this task's context."""
    await asyncio.sleep(0)
    tracer.start_span(name).end()


def test_explicit_installation_on_a_private_provider_is_idempotent(
    state: WyrdState,
) -> None:
    """Installing twice reports success both times and correlates spans once."""
    private = TracerProvider()
    exporter = InMemorySpanExporter()
    private.add_span_processor(SimpleSpanProcessor(exporter))
    assert install_run_correlation(private) is True
    assert install_run_correlation(private) is True
    run = state.run()
    with run:
        private.get_tracer("framework").start_span("private").end()
    assert _correlation(exporter) == {"private": (_ref(state, run), run.run_id)}


def test_repeated_entry_reinstalls_nothing_on_the_global_provider(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Entering a run twice keeps one correlation per span on the global provider."""
    provider, exporter = spans
    run = state.run()
    with run, run:
        provider.get_tracer("framework").start_span("doubly-entered").end()
    assert install_run_correlation() is True
    assert _correlation(exporter) == {"doubly-entered": (_ref(state, run), run.run_id)}


class _Raising:
    """A provider whose processor registration raises."""

    def add_span_processor(self, processor: object) -> None:
        raise RuntimeError("registration failed")


class _Unmarkable:
    """A provider that accepts processors but cannot carry the Wyrd marker."""

    __slots__ = ()

    def add_span_processor(self, processor: object) -> None:
        raise AssertionError("an unmarkable provider is never registered")


def test_unsupported_providers_are_refused_without_raising() -> None:
    """API-only, proxy, unmarkable, and failing providers report False instead of raising."""
    assert install_run_correlation(object()) is False
    assert install_run_correlation(trace.ProxyTracerProvider()) is False
    assert install_run_correlation(trace.ProxyTracerProvider()) is False
    assert install_run_correlation(_Unmarkable()) is False
    assert install_run_correlation(_Raising()) is False


@pytest.mark.parametrize(
    ("exc_type", "exc_value"), [(None, None), (ValueError, ValueError("app"))], ids=["ok", "error"]
)
def test_run_exit_never_suppresses(
    state: WyrdState, exc_type: type[BaseException] | None, exc_value: BaseException | None
) -> None:
    """``Run.__exit__`` returns False, so an exception raised in scope propagates."""
    run = state.run()
    run.__enter__()
    assert run.__exit__(exc_type, exc_value, None) is False


@pytest.fixture
def without_opentelemetry() -> Iterator[None]:
    """``wyrd.otel`` re-imported as though the optional package were not installed, then restored."""
    with pytest.MonkeyPatch.context() as patch:
        patch.setitem(sys.modules, "opentelemetry", None)
        importlib.reload(wyrd.otel)
        yield
    importlib.reload(wyrd.otel)


@pytest.mark.usefixtures("without_opentelemetry")
def test_missing_opentelemetry_is_a_no_op(state: WyrdState) -> None:
    """Without the optional package, installation reports False and a run still scopes."""
    assert wyrd.otel.install_run_correlation() is False
    with state.run("model") as run:
        assert run.alias == "model"


def test_otel_imports_without_the_agents_sdk() -> None:
    """Without the optional Agents SDK, ``wyrd.otel`` still imports and builds its processor."""
    with pytest.MonkeyPatch.context() as patch:
        patch.setitem(sys.modules, "agents", None)
        patch.setitem(sys.modules, "agents.tracing", None)
        importlib.reload(wyrd.otel)
        assert wyrd.otel.WyrdAgentsTraceProcessor.__bases__ == (object,)
        wyrd.otel.WyrdAgentsTraceProcessor().shutdown()
    importlib.reload(wyrd.otel)


def _record_reaches_the_ordinary_boundary(run: Run) -> None:
    """Record one explicit row and assert its ordinary offline error."""
    with pytest.raises(wyrd.WyrdError) as raised:
        run.observe.record("vala.datasets.latency", {"latency_ms": 1.0})
    assert raised.value.code == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def _broken(*_args: object, **_kwargs: object) -> None:
    """Stand in for an OpenTelemetry call that fails."""
    raise RuntimeError("telemetry broke")


def test_registration_and_attach_failures_never_block_observations(
    state: WyrdState, monkeypatch: pytest.MonkeyPatch
) -> None:
    """API-only, unmarkable, raising, and failing-attach paths leave emits untouched."""
    run = state.run("model")
    for provider in (object(), _Unmarkable(), _Raising()):
        monkeypatch.setattr(trace, "get_tracer_provider", lambda provider=provider: provider)
        with run as entered:
            _record_reaches_the_ordinary_boundary(entered)
        with pytest.raises(ValueError, match="app"), run:
            raise ValueError("app")

    monkeypatch.setattr(otel_context, "attach", _broken)
    with run as entered:
        _record_reaches_the_ordinary_boundary(entered)


def test_failing_enrichment_never_blocks_spans_or_observations(
    state: WyrdState,
    spans: tuple[TracerProvider, InMemorySpanExporter],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A failing span-start lookup leaves the span unenriched and the emit untouched."""
    provider, exporter = spans
    with state.run() as entered, monkeypatch.context() as patch:
        patch.setattr(otel_context, "get_value", _broken)
        provider.get_tracer("framework").start_span("unenriched").end()
        _record_reaches_the_ordinary_boundary(entered)
    assert _correlation(exporter) == {"unenriched": None}


def test_user_errors_propagate_out_of_a_run_scope(state: WyrdState) -> None:
    """The run scope re-raises the user's exception unchanged."""
    with pytest.raises(ValueError, match="user failure"), state.run():
        raise ValueError("user failure")


@pytest.fixture
def model_run(state: WyrdState) -> Iterator[Run]:
    """A ``model`` run whose scope is cleared once the test restores OpenTelemetry."""
    run = state.run("model")
    yield run
    run.__exit__()


def test_exit_context_update_failure_never_blocks_observations(
    model_run: Run, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A failing exit attach never raises, masks a user error, or blocks emits."""
    with monkeypatch.context() as patch:
        with model_run:
            patch.setattr(otel_context, "attach", _broken)
        _record_reaches_the_ordinary_boundary(model_run)
        with pytest.raises(ValueError, match="app"), model_run:
            raise ValueError("app")
        _record_reaches_the_ordinary_boundary(model_run)


def test_mismatched_exit_changes_nothing(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Exiting a view that is not the innermost scope keeps the outer pair."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run()
    run.__enter__()
    run.for_card("model").__exit__()
    tracer.start_span("mismatched").end()
    run.__exit__()
    tracer.start_span("cleared").end()
    assert _correlation(exporter) == {
        "mismatched": (_ref(state, run), run.run_id),
        "cleared": None,
    }


def test_nested_entry_never_overwrites_an_active_span_correlation(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """A nested Card scope leaves already-correlated active spans untouched."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = state.run()
    model = run.for_card("model")
    with tracer.start_as_current_span("outer"):
        with run:
            with model:
                tracer.start_span("nested").end()
            with tracer.start_as_current_span("child"):
                with model:
                    tracer.start_span("inner").end()
            tracer.start_span("restored").end()
    root, nested = (_ref(state, run), run.run_id), (_ref(state, model), run.run_id)
    assert _correlation(exporter) == {
        "nested": nested,
        "inner": nested,
        "child": root,
        "restored": root,
        "outer": root,
    }


# ── Managed telemetry setup, owned providers, lifecycle, and the Agents bridge ──


UNREACHABLE = "http://127.0.0.1:9"


@pytest.fixture
def unset_global_provider(monkeypatch: pytest.MonkeyPatch) -> Iterator[None]:
    """Unset OpenTelemetry's once-only global provider, and shut down what a test installs."""
    from opentelemetry.util._once import Once

    monkeypatch.setattr(trace, "_TRACER_PROVIDER", None)
    monkeypatch.setattr(trace, "_TRACER_PROVIDER_SET_ONCE", Once())
    yield
    installed = trace._TRACER_PROVIDER
    if isinstance(installed, TracerProvider):
        installed.shutdown()


@pytest.fixture
def client_state(complete_bundle: Path, interfaces: dict[str, Any]) -> WyrdState:
    """The complete bundle bound to an explicit client whose server is never dialled."""
    client = WyrdClient(server_url=UNREACHABLE, credential="wyrd_unit_unused")
    return WyrdState.from_path(complete_bundle, client, interfaces=interfaces)


def _second_state(complete_bundle: Path, interfaces: dict[str, Any]) -> WyrdState:
    """Another state over the same bundle with its own client."""
    client = WyrdClient(server_url=UNREACHABLE, credential="wyrd_unit_other")
    return WyrdState.from_path(complete_bundle, client, interfaces=interfaces)


@pytest.mark.usefixtures("unset_global_provider")
@pytest.mark.parametrize("case", ["repeat", "overlap", "other_state", "application", "lost_race"])
def test_telemetry_setup(
    case: str,
    client_state: WyrdState,
    complete_bundle: Path,
    interfaces: dict[str, Any],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """One managed provider per application: repeats are idempotent, others are refused intact."""
    if case == "repeat":
        client_state.start_telemetry()
        installed = trace.get_tracer_provider()
        client_state.start_telemetry()
        assert trace.get_tracer_provider() is installed
        assert isinstance(installed, TracerProvider)
        return
    if case == "overlap":
        # The first call holds setup open until the second call enters setup
        # too, or, once setup is serialized, until a short wait passes.
        install = wyrd.otel._start_telemetry
        entered: list[object] = []
        second = threading.Event()

        def installing(client: Any) -> Any:
            entered.append(client)
            if len(entered) == 1:
                second.wait(1)
            else:
                second.set()
            return install(client)

        monkeypatch.setattr(wyrd.otel, "_start_telemetry", installing)
        failures: list[BaseException] = []

        def start() -> None:
            try:
                client_state.start_telemetry()
            except BaseException as failure:
                failures.append(failure)

        threads = [threading.Thread(target=start) for _ in range(2)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(10)
        assert failures == []
        assert len(entered) == 1
        assert isinstance(trace.get_tracer_provider(), TracerProvider)
        return
    if case == "other_state":
        _second_state(complete_bundle, interfaces).start_telemetry()
    elif case == "application":
        trace.set_tracer_provider(TracerProvider())
    else:
        # Another installer wins between the check and the install; the losing
        # provider must be released rather than left exporting in the background.
        released: list[TracerProvider] = []
        monkeypatch.setattr(trace, "set_tracer_provider", lambda provider: None)
        monkeypatch.setattr(TracerProvider, "shutdown", lambda provider: released.append(provider))
    existing = trace.get_tracer_provider()
    with pytest.raises(wyrd.WyrdError) as refused:
        client_state.start_telemetry()
    assert refused.value.code == "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS"
    assert trace.get_tracer_provider() is existing
    if case == "lost_race":
        assert len(released) == 1


@pytest.mark.usefixtures("unset_global_provider")
def test_telemetry_setup_names_the_otel_extra_without_the_sdk(client_state: WyrdState) -> None:
    """Managed setup without the OpenTelemetry SDK names the extra that installs it."""
    with pytest.MonkeyPatch.context() as patch:
        patch.setitem(sys.modules, "opentelemetry.sdk.trace", None)
        with pytest.raises(ImportError, match=r"wyrd-ai\[otel\]"):
            client_state.start_telemetry()
    # The failure is not remembered: once the SDK imports, setup succeeds.
    client_state.start_telemetry()
    assert isinstance(trace.get_tracer_provider(), TracerProvider)


class _FailOnce:
    """A provider whose first registration raises and later ones succeed."""

    def __init__(self) -> None:
        self.processors: list[object] = []
        self.calls = 0

    def add_span_processor(self, processor: object) -> None:
        self.calls += 1
        if self.calls == 1:
            raise RuntimeError("transient registration failure")
        self.processors.append(processor)


def test_owned_provider_correlation(state: WyrdState) -> None:
    """A failed registration is retryable, success is idempotent, and owned settings stay."""
    flaky = _FailOnce()
    assert install_run_correlation(flaky) is False
    assert install_run_correlation(flaky) is True
    assert install_run_correlation(flaky) is True
    assert len(flaky.processors) == 1

    from opentelemetry.sdk.resources import Resource
    from opentelemetry.sdk.trace.sampling import ALWAYS_OFF

    exporter = InMemorySpanExporter()
    owned = TracerProvider(resource=Resource.create({"service.name": "owned"}))
    owned.add_span_processor(SimpleSpanProcessor(exporter))
    assert install_run_correlation(owned) is True
    run, model = state.run(), state.run().for_card("model")

    async def scoped(view: Run, name: str) -> None:
        with view:
            await asyncio.sleep(0)
            owned.get_tracer("framework").start_span(name).end()

    async def both() -> None:
        await asyncio.gather(scoped(run, "a"), scoped(model, "b"))

    asyncio.run(both())
    owned.get_tracer("framework").start_span("unscoped").end()
    assert _correlation(exporter) == {
        "a": (_ref(state, run), run.run_id),
        "b": (_ref(state, model), model.run_id),
        "unscoped": None,
    }
    assert all(
        span.resource.attributes["service.name"] == "owned"
        for span in exporter.get_finished_spans()
    )

    unsampled = TracerProvider(sampler=ALWAYS_OFF)
    dropped = InMemorySpanExporter()
    unsampled.add_span_processor(SimpleSpanProcessor(dropped))
    assert install_run_correlation(unsampled) is True
    with run:
        unsampled.get_tracer("framework").start_span("dropped").end()
    assert dropped.get_finished_spans() == ()


@pytest.mark.usefixtures("unset_global_provider")
@pytest.mark.parametrize(
    ("outcome", "method", "code", "native_code"),
    [
        (
            False,
            "flush",
            "WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT",
            "WYRD_SDK_400_BIFROST_NOT_STARTED",
        ),
        (
            TimeoutError,
            "flush",
            "WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT",
            "WYRD_SDK_400_BIFROST_NOT_STARTED",
        ),
        (
            RuntimeError,
            "flush",
            "WYRD_SDK_503_TELEMETRY_FLUSH_FAILED",
            "WYRD_SDK_400_BIFROST_NOT_STARTED",
        ),
        (True, "flush", "WYRD_SDK_400_BIFROST_NOT_STARTED", None),
        (False, "shutdown", "WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT", None),
        (RuntimeError, "shutdown", "WYRD_SDK_503_TELEMETRY_FLUSH_FAILED", None),
        (True, "shutdown", None, None),
    ],
    ids=[
        "false",
        "timeout",
        "raises",
        "native-only",
        "shutdown-false",
        "shutdown-raises",
        "shutdown-ok",
    ],
)
def test_managed_telemetry_lifecycle(
    client_state: WyrdState,
    monkeypatch: pytest.MonkeyPatch,
    outcome: object,
    method: str,
    code: str | None,
    native_code: str | None,
) -> None:
    """The managed provider flushes first; every failure is reported and the drain still runs.

    Bifrost is never started here, so a flush's native drain reports
    ``BIFROST_NOT_STARTED`` and a shutdown's drain succeeds: either result in
    the error proves the drain was attempted after the provider failed.
    """
    client_state.start_telemetry()
    provider = trace.get_tracer_provider()
    assert isinstance(provider, TracerProvider)
    flushes: list[str] = []

    def force_flush(timeout_millis: int = 30000) -> bool:
        flushes.append("otel")
        if isinstance(outcome, type):
            raise outcome("SECRET payload in a provider error")
        return bool(outcome)

    monkeypatch.setattr(provider, "force_flush", force_flush)
    active = provider.get_tracer("framework").start_span("active")
    if code is None:
        getattr(client_state, method)()
    else:
        with pytest.raises(wyrd.WyrdError) as failed:
            getattr(client_state, method)()
        assert failed.value.code == code
        assert "SECRET" not in str(failed.value)
        if code.startswith("WYRD_SDK_50"):
            assert failed.value.details["stage"] == "opentelemetry"
            assert failed.value.details["native_code"] == native_code
    assert flushes == ["otel"]
    assert active.is_recording()
    active.end()
    assert trace.get_tracer_provider() is provider


def _stale_flush(
    state: WyrdState, provider: TracerProvider, tracer: Any, exporter: Any, method: str
) -> None:
    """Call ``state.<method>`` while an earlier flush has drained but not settled.

    The earlier flush runs on its own thread and is held after the provider's
    flush returns. The call under test ends span ``late-<method>`` first, and
    the earlier flush is released only once that call's thread is waiting.
    """
    drained, settle = threading.Event(), threading.Event()
    flush = provider.force_flush

    def held(timeout_millis: int = 30000) -> bool:
        flushed = flush(timeout_millis)
        if not settle.is_set():
            drained.set()
            settle.wait(5)
        return flushed

    earlier = threading.Thread(target=lambda: _ignore_errors(state.flush))
    caller = threading.get_ident()

    def release_when_waiting() -> None:
        while not settle.is_set():
            frame = sys._current_frames().get(caller)
            if frame is not None and frame.f_code.co_name == "wait":
                settle.set()
            time.sleep(0.001)

    with pytest.MonkeyPatch.context() as patch:
        patch.setattr(provider, "force_flush", held)
        patch.setattr(wyrd.otel, "_FLUSH_TIMEOUT_MILLIS", 5000)
        try:
            earlier.start()
            assert drained.wait(5)
            tracer.start_span(f"late-{method}").end()
            watcher = threading.Thread(target=release_when_waiting, daemon=True)
            watcher.start()
            if method == "flush":
                with pytest.raises(wyrd.WyrdError) as failed:
                    state.flush()
                assert failed.value.code == "WYRD_SDK_400_BIFROST_NOT_STARTED"
            else:
                state.shutdown()
            exported = {span.name for span in exporter.get_finished_spans()}
            assert f"late-{method}" in exported
        finally:
            settle.set()
            earlier.join(5)


def _ignore_errors(call: Any) -> None:
    """Run ``call``, discarding the Wyrd error a Bifrost-less flush raises."""
    try:
        call()
    except wyrd.WyrdError:
        pass


class _HeldExporter(InMemorySpanExporter):
    """An in-memory exporter whose exports wait until the test releases them."""

    def __init__(self) -> None:
        super().__init__()
        self.entered = threading.Event()
        self.release = threading.Event()

    def export(self, spans: Any) -> Any:
        self.entered.set()
        self.release.wait()
        return super().export(spans)


@pytest.mark.usefixtures("unset_global_provider")
def test_managed_telemetry_flush_deadline(
    client_state: WyrdState, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A held stock export times out within the bound; spans and the drain keep working."""
    exporter = _HeldExporter()
    monkeypatch.setattr(wyrd.otel, "span_exporter", lambda client: exporter)
    monkeypatch.setattr(wyrd.otel, "_FLUSH_TIMEOUT_MILLIS", 200)
    # Frees the held export if the flush ignores its bound, so a regression fails, not hangs.
    fallback = threading.Timer(5, exporter.release.set)
    fallback.start()
    client_state.start_telemetry()
    provider = trace.get_tracer_provider()
    assert isinstance(provider, TracerProvider)
    tracer = provider.get_tracer("framework")
    active = tracer.start_span("active")
    tracer.start_span("before").end()
    run = client_state.run()

    for _ in range(2):  # a repeated flush waits on the held attempt rather than adding one
        started = time.monotonic()
        with pytest.raises(wyrd.WyrdError) as failed:
            client_state.flush()
        assert time.monotonic() - started < 2
        assert failed.value.code == "WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT"
        assert failed.value.details["native_code"] == "WYRD_SDK_400_BIFROST_NOT_STARTED"
        assert exporter.entered.is_set()
        assert not exporter.release.is_set()
        with run:
            tracer.start_span("during").end()
    flushing = [t for t in threading.enumerate() if t.name == "wyrd-telemetry-flush"]
    assert len(flushing) == 1

    exporter.release.set()
    fallback.cancel()
    flushing[0].join(5)

    # An earlier flush has drained the queue but not yet settled when a later
    # call enters with a newly ended span: the later call must flush again.
    for method in ("flush", "shutdown"):
        _stale_flush(client_state, provider, tracer, exporter, method)
    assert active.is_recording()
    assert active.is_recording()
    active.end()
    assert trace.get_tracer_provider() is provider
    assert provider.force_flush()
    exported = {span.name: span for span in exporter.get_finished_spans()}
    assert set(exported) == {"before", "during", "late-flush", "late-shutdown", "active"}
    assert dict(exported["during"].attributes or {})["wyrd.run_id"] == run.run_id


def test_application_provider_is_never_flushed_by_state(
    client_state: WyrdState,
    spans: tuple[TracerProvider, InMemorySpanExporter],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A state that did not install telemetry flushes and shuts down only its writer."""
    provider, _ = spans
    monkeypatch.setattr(provider, "force_flush", _broken)
    monkeypatch.setattr(provider, "shutdown", _broken)
    client_state.shutdown()


class _ExportHeaders(BaseHTTPRequestHandler):
    """A local OTLP/HTTP receiver recording each export's Wyrd token header."""

    tokens: list[str] = []

    def do_POST(self) -> None:  # noqa: N802 - http.server's handler name
        self.rfile.read(int(self.headers["content-length"]))
        type(self).tokens.append(self.headers["x-wyrd-access-token"])
        self.send_response(200)
        self.send_header("content-type", "application/x-protobuf")
        self.send_header("content-length", "0")
        self.end_headers()

    def log_message(self, format: str, *args: object) -> None:
        """Keep the test output quiet."""


class _RotatingClient:
    """A client stand-in returning a new access token on each request."""

    workload_uid = None
    deployment_environment = None

    def __init__(self, server_url: str) -> None:
        self.server_url = server_url
        self.issued = 0

    def access_token(self) -> str:
        self.issued += 1
        return f"token-{self.issued}"


def test_exporter_token_freshness() -> None:
    """Every export asks the client for its current token; none is copied at setup."""
    _ExportHeaders.tokens = []
    server = ThreadingHTTPServer(("127.0.0.1", 0), _ExportHeaders)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        client = _RotatingClient(f"http://127.0.0.1:{server.server_address[1]}")
        provider = TracerProvider()
        provider.add_span_processor(
            SimpleSpanProcessor(wyrd.otel.span_exporter(cast("WyrdClient", client)))
        )
        assert client.issued == 0
        provider.get_tracer("framework").start_span("first").end()
        provider.get_tracer("framework").start_span("second").end()
        provider.shutdown()
    finally:
        server.shutdown()
    assert _ExportHeaders.tokens == ["Bearer token-1", "Bearer token-2"]


# The Agents SDK callbacks, driven with real SDK span data and stand-in span handles.


def _agents_span(
    span_id: str,
    data: Any,
    *,
    parent_id: str | None = None,
    started_at: str | None = None,
    ended_at: str | None = None,
    error: dict[str, Any] | None = None,
) -> Any:
    """A stand-in Agents SDK span carrying only what the processor reads."""
    from types import SimpleNamespace

    return SimpleNamespace(
        span_id=span_id,
        trace_id="trace_1",
        parent_id=parent_id,
        span_data=data,
        started_at=started_at,
        ended_at=ended_at,
        error=error,
    )


def _finished(exporter: InMemorySpanExporter) -> dict[str, Any]:
    """Finished spans by name."""
    return {span.name: span for span in exporter.get_finished_spans()}


def test_agents_callback_lifecycle(
    state: WyrdState, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Hierarchy, timestamps, GenAI data, safe errors, scope, and lifecycle edges survive."""
    from types import SimpleNamespace

    from agents.tracing.span_data import (
        AgentSpanData,
        CustomSpanData,
        FunctionSpanData,
        GenerationSpanData,
        ResponseSpanData,
    )
    from wyrd.otel import WyrdAgentsTraceProcessor

    provider, exporter = spans
    processor = WyrdAgentsTraceProcessor()
    trace_handle = cast("Any", SimpleNamespace(trace_id="trace_1", name="workflow"))
    run_a, run_b = state.run(), state.run()

    with provider.get_tracer("app").start_as_current_span("enclosing") as enclosing, run_a:
        processor.on_trace_start(trace_handle)
        agent = _agents_span(
            "agent", AgentSpanData(name="assistant"), started_at="2026-01-01T00:00:00+00:00"
        )
        processor.on_span_start(agent)
        processor.on_span_start(agent)  # duplicate start of a live id is ignored
        tool = _agents_span(
            "tool",
            FunctionSpanData(name="lookup", input="SECRET-ARGS", output="SECRET-RESULT"),
            parent_id="agent",
            started_at="not a timestamp",
        )
        processor.on_span_start(tool)
        generation = _agents_span(
            "generation",
            GenerationSpanData(
                input=[{"content": "SECRET-PROMPT"}], output=[{"content": "SECRET-REPLY"}]
            ),
            parent_id="missing-parent",
        )
        processor.on_span_start(generation)
    # Late data and completion in another Run's scope.
    generation.span_data.model = "local-model"
    generation.span_data.usage = {"input_tokens": 3, "output_tokens": 1}
    with run_b:
        processor.on_span_end(generation)
        tool.error = {"message": "SECRET-ERROR", "data": {"secret": "SECRET-DATA"}}
        processor.on_span_end(tool)
        processor.on_span_end(tool)  # duplicate end is harmless
        processor.on_span_end(_agents_span("never-started", CustomSpanData(name="x", data={})))
    response = _agents_span(
        "response",
        ResponseSpanData(
            response=cast(
                "Any",
                SimpleNamespace(
                    model="resp-model", usage=SimpleNamespace(input_tokens=5, output_tokens=2)
                ),
            )
        ),
        parent_id="agent",
    )
    processor.on_span_start(response)
    processor.on_span_end(response)
    # With content capture off the SDK withholds the response but still supplies usage.
    withheld = _agents_span("withheld", ResponseSpanData(), parent_id="agent")
    processor.on_span_start(withheld)
    withheld.span_data.usage = {"input_tokens": 7, "output_tokens": 0}
    processor.on_span_end(withheld)
    custom = _agents_span(
        "custom", CustomSpanData(name="checkpoint", data={"secret": "SECRET"}), parent_id="agent"
    )
    processor.on_span_start(custom)
    processor.on_span_end(custom)
    agent.ended_at = "2026-01-01T00:00:02+00:00"
    processor.on_span_end(agent)
    processor.on_trace_end(trace_handle)

    finished = _finished(exporter)
    names = [span.name for span in exporter.get_finished_spans()]
    assert len(names) == len(set(names)), names
    root = finished["workflow"]
    assert root.parent.span_id == enclosing.get_span_context().span_id
    assert {span.context.trace_id for span in finished.values()} == {
        enclosing.get_span_context().trace_id
    }
    agent_span = finished["invoke_agent assistant"]
    assert agent_span.parent.span_id == root.context.span_id
    assert agent_span.start_time == 1_767_225_600 * 10**9
    assert agent_span.end_time == 1_767_225_602 * 10**9
    assert agent_span.attributes["gen_ai.agent.name"] == "assistant"
    tool_span = finished["execute_tool lookup"]
    assert tool_span.parent.span_id == agent_span.context.span_id
    assert tool_span.attributes["gen_ai.tool.name"] == "lookup"
    assert tool_span.status.status_code == trace.StatusCode.ERROR
    assert tool_span.attributes["error.type"] == "_OTHER"
    chat = finished["chat local-model"]
    assert chat.parent.span_id == root.context.span_id  # unresolved parent falls back to the trace
    assert dict(chat.attributes) == {
        "gen_ai.operation.name": "chat",
        "gen_ai.request.model": "local-model",
        "gen_ai.usage.input_tokens": 3,
        "gen_ai.usage.output_tokens": 1,
        "wyrd.card_uid": _ref(state, run_a),
        "wyrd.run_id": run_a.run_id,
    }
    assert finished["chat resp-model"].attributes["gen_ai.usage.output_tokens"] == 2
    assert dict(finished["chat"].attributes) == {
        "gen_ai.operation.name": "chat",
        "gen_ai.usage.input_tokens": 7,
        "gen_ai.usage.output_tokens": 0,
    }
    assert not any(key.startswith("gen_ai") for key in finished["checkpoint"].attributes)
    for span in finished.values():
        rendered = f"{dict(span.attributes)} {span.status.description} {span.events}"
        assert "SECRET" not in rendered, span.name
    scoped = {name: span.attributes.get("wyrd.run_id") for name, span in finished.items()}
    assert scoped == {
        "enclosing": run_a.run_id,
        "workflow": run_a.run_id,
        "invoke_agent assistant": run_a.run_id,
        "execute_tool lookup": run_a.run_id,
        "chat local-model": run_a.run_id,
        "chat resp-model": None,
        "chat": None,
        "checkpoint": None,
    }

    # Shutdown ends unfinished spans with ERROR, is idempotent, and ignores late callbacks.
    exporter.clear()
    interrupted_handle = cast("Any", SimpleNamespace(trace_id="trace_2", name="interrupted"))
    processor.on_trace_start(interrupted_handle)
    processor.shutdown()
    processor.shutdown()
    processor.on_span_start(_agents_span("late", CustomSpanData(name="late", data={})))
    processor.on_trace_end(interrupted_handle)
    processor.force_flush()
    (interrupted,) = exporter.get_finished_spans()
    assert interrupted.name == "interrupted"
    assert interrupted.status.status_code == trace.StatusCode.ERROR
    assert interrupted.status.description == wyrd.otel._INTERRUPTED


def test_agents_callbacks_are_thread_safe(
    spans: tuple[TracerProvider, InMemorySpanExporter],
) -> None:
    """Concurrent callbacks on one processor start and end every span exactly once."""
    from agents.tracing.span_data import CustomSpanData
    from wyrd.otel import WyrdAgentsTraceProcessor

    _, exporter = spans
    processor = WyrdAgentsTraceProcessor()
    barrier = threading.Barrier(8)

    def work(worker: int) -> None:
        barrier.wait()
        for index in range(50):
            span = _agents_span(
                f"{worker}-{index}", CustomSpanData(name=f"{worker}-{index}", data={})
            )
            processor.on_span_start(span)
            processor.on_span_start(span)
            processor.on_span_end(span)
            processor.on_span_end(span)

    threads = [threading.Thread(target=work, args=(worker,)) for worker in range(8)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    names = [span.name for span in exporter.get_finished_spans()]
    assert len(names) == 400 == len(set(names))
