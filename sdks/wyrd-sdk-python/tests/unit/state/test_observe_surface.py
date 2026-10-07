"""Scoped observation surface reachable from an offline ``WyrdState``.

These tests stay server-free: opening a run performs no network IO, and every
refusal happens before admission. Accepted observations belong to the gated
journey lanes, not here.
"""

import asyncio
import subprocess
import sys
import threading
from collections.abc import Iterator
from pathlib import Path
from typing import Any
from uuid import UUID

import pytest
import wyrd
from opentelemetry import context as otel_context
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter
from pydantic import BaseModel
from wyrd.observe import Observe, Run
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState


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


def test_drift_refuses_a_payload_that_is_not_an_object(state: WyrdState) -> None:
    """A sequence is not a feature map and is refused at the boundary."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.drift([1, 2, 3])
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


def test_drift_refuses_a_nested_feature_value(state: WyrdState) -> None:
    """Feature values are scalars; a nested object reports the offending key."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.drift({"nested": {"inner": 1}})
    assert raised.value.code == "WYRD_SDK_400_INVALID_OBSERVATION"


class _PydanticFeatures(BaseModel):
    """Test-only Pydantic payload whose ``model_dump_json()`` reaches Rust as-is."""

    latency: float
    tier: str


@pytest.mark.parametrize(
    ("payload", "code"),
    [
        ({"Latency": 1.0}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"has space": 1.0}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"latency": None}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"latency": [1.0]}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"latency": float("nan")}, "WYRD_SPEC_400_VALIDATION"),
        ({"latency": float("inf")}, "WYRD_SPEC_400_VALIDATION"),
        (_PydanticFeatures(latency=float("nan"), tier="gold"), "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"count": 2**53 + 1}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"count": -(2**53) - 1}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"count": 2**63}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"count": 2**64}, "WYRD_SDK_400_INVALID_OBSERVATION"),
        ({"count": -(2**64)}, "WYRD_SDK_400_INVALID_OBSERVATION"),
    ],
    ids=[
        "uppercase-name",
        "spaced-name",
        "null",
        "nested-array",
        "nan",
        "infinity",
        "pydantic-nan",
        "above-exact-float",
        "below-exact-float",
        "beyond-i64",
        "beyond-u64",
        "below-i64",
    ],
)
def test_drift_refuses_unrepresentable_payloads_before_admission(
    state: WyrdState, payload: object, code: str
) -> None:
    """Invalid names, null or nested values, and unrepresentable numbers fail first.

    The state never started Bifrost, so reaching the queue would raise
    ``WYRD_SDK_400_BIFROST_NOT_STARTED``; a validation code proves refusal
    happened before admission.
    """
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.drift(payload)
    assert raised.value.code == code


def test_drift_refuses_a_malformed_session_id(state: WyrdState) -> None:
    """``session_id`` is a UUID and is parsed before anything is enqueued."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.drift({"score": 1.0}, session_id="not-a-uuid")
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


def test_eval_refuses_a_span_without_its_trace(state: WyrdState) -> None:
    """A span id is only meaningful inside its trace."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.eval({"answer": "yes"}, span_id="00f067aa0ba902b7")
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


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
def test_drift_refuses_a_non_string_mapping_key(state: WyrdState, key: object) -> None:
    """``json.dumps`` would stringify the key, so it is refused before the writer."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.drift({key: 1.0})
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


def test_eval_refuses_a_non_string_mapping_key(state: WyrdState) -> None:
    """Eval context keys are strings; ``json.dumps`` would silently stringify ``1``."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.eval({1: "yes"})
    assert raised.value.code == "WYRD_SPEC_400_VALIDATION"


def test_explicit_span_without_trace_is_refused_inside_an_active_span(state: WyrdState) -> None:
    """An explicit id disables the active-span lookup, so it is never half-merged."""
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    with tracer.start_as_current_span("unit"), pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.eval({"answer": "yes"}, span_id="00f067aa0ba902b7")
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


def test_verify_refuses_a_context_list_for_an_eval_verifier(state: WyrdState) -> None:
    """An Eval Verifier judges one context object, so a list is refused locally."""
    with pytest.raises(wyrd.WyrdError) as raised:
        state.run().observe.verify("ok-check", [{"ok": True}])
    assert raised.value.code == "WYRD_SDK_400_INVALID_OBSERVATION"


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
        card_ref, run_id = attributes.get("wyrd.card_ref"), attributes.get("wyrd.run_id")
        out[span.name] = None if card_ref is None else (str(card_ref), str(run_id))
    return out


def _ref(state: WyrdState, view: Run) -> str:
    """The exact Card reference text a view's spans carry."""
    return str(state.card_ref(view.alias))


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
                tracer.start_span("grandchild", attributes={"wyrd.card_ref": "x"}).end()
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


MISSING_OPENTELEMETRY = """\
import sys

sys.modules["opentelemetry"] = None
bundle, support = sys.argv[1:]
sys.path.insert(0, support)
from support import TinyDataInterface, TinyModelInterface
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState

assert install_run_correlation() is False
state = WyrdState.from_path(
    bundle,
    interfaces={
        "model": TinyModelInterface(),
        "backup": TinyModelInterface(),
        "training_data": TinyDataInterface(),
    },
)
with state.run("model") as run:
    assert run.alias == "model"
"""


def test_missing_opentelemetry_is_a_no_op(complete_bundle: Path) -> None:
    """Without the optional package, installation reports False and a run still scopes."""
    subprocess.run(
        [
            sys.executable,
            "-c",
            MISSING_OPENTELEMETRY,
            str(complete_bundle),
            str(Path(__file__).parent),
        ],
        check=True,
    )


def _drift_reaches_the_ordinary_boundary(run: Run) -> None:
    """Emit one explicit Drift observation and assert its ordinary offline error."""
    with pytest.raises(wyrd.WyrdError) as raised:
        run.observe.drift({"latency_ms": 1.0})
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
            _drift_reaches_the_ordinary_boundary(entered)
        with pytest.raises(ValueError, match="app"), run:
            raise ValueError("app")

    monkeypatch.setattr(otel_context, "attach", _broken)
    with run as entered:
        _drift_reaches_the_ordinary_boundary(entered)


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
        _drift_reaches_the_ordinary_boundary(entered)
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
        _drift_reaches_the_ordinary_boundary(model_run)
        with pytest.raises(ValueError, match="app"), model_run:
            raise ValueError("app")
        _drift_reaches_the_ordinary_boundary(model_run)


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
