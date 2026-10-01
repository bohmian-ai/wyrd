"""Scoped observation surface reachable from an offline ``WyrdState``.

These tests stay server-free: opening a run performs no network IO, and every
emit is proven by the error it raises before admission. Durable emit behavior
belongs to the gated journey lanes, not here.
"""

import asyncio
import contextvars
import sys
from dataclasses import dataclass
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
from wyrd.eval import MediaRef
from wyrd.observe import Observe, Run
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState

from .support import TinyDataInterface, TinyModelInterface, build_complete_bundle


def _state(tmp_path: Path) -> WyrdState:
    """Hydrate the shared complete fixture bundle with its custom interfaces.

    The interfaces are fresh per call so one test's loader telemetry cannot leak
    into another's assertions.
    """
    return WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces={
            "model": TinyModelInterface(),
            "backup": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
    )


ORIGINAL_GET_VALUE = otel_context.get_value
ORIGINAL_DETACH = otel_context.detach


def _code(error: BaseException) -> str:
    """Return the stable Wyrd error code carried by a raised boundary error."""
    assert isinstance(error, wyrd.WyrdError)
    return error.code


def test_run_targets_the_root_service_with_a_uuidv7_identity(tmp_path: Path) -> None:
    """One run is one invocation anchored on the bundle's root Service Card."""
    run = _state(tmp_path).run()
    assert isinstance(run, Run)
    assert UUID(run.run_id).version == 7
    assert run.card_ref.startswith("default/Service/service@1.0.0")
    assert isinstance(run.observe, Observe)


def test_scoped_views_share_one_invocation_and_keep_their_subjects(tmp_path: Path) -> None:
    """``for_card`` returns immutable siblings under one invocation identity."""
    run = _state(tmp_path).run()
    model = run.for_card("model")
    backup = run.for_card("backup")
    assert model.run_id == run.run_id == backup.run_id
    assert model.card_ref != backup.card_ref
    assert run.card_ref.startswith("default/Service/service@1.0.0")


def test_each_run_is_its_own_invocation(tmp_path: Path) -> None:
    """Two runs from one state are distinct invocations."""
    state = _state(tmp_path)
    assert state.run().run_id != state.run().run_id


def test_unknown_alias_is_refused(tmp_path: Path) -> None:
    """An alias the bundle does not register fails locally."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().for_card("missing")
    assert _code(raised.value) == "WYRD_SDK_404_UNKNOWN_ALIAS"


def test_drift_accepts_a_mapping_and_reaches_the_writer(tmp_path: Path) -> None:
    """A flat mapping converts, then fails only because Bifrost is not started."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({"latency_ms": 12.5, "tier": "gold"})
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_drift_accepts_a_dataclass_instance(tmp_path: Path) -> None:
    """A dataclass instance is reduced to its fields before admission."""

    @dataclass
    class Features:
        """Test-only flat feature payload for the dataclass conversion path."""

        latency_ms: float
        cached: bool

    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift(Features(latency_ms=3.0, cached=True))
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_drift_refuses_a_payload_that_is_not_an_object(tmp_path: Path) -> None:
    """A sequence is not a feature map and is refused at the boundary."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift([1, 2, 3])
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_drift_refuses_a_nested_feature_value(tmp_path: Path) -> None:
    """Feature values are scalars; a nested object reports the offending key."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({"nested": {"inner": 1}})
    assert _code(raised.value) == "WYRD_SDK_400_INVALID_OBSERVATION"


def test_drift_refuses_a_malformed_session_id(tmp_path: Path) -> None:
    """``session_id`` is a UUID and is parsed before anything is enqueued."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({"score": 1.0}, session_id="not-a-uuid")
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_eval_accepts_context_and_media(tmp_path: Path) -> None:
    """Context and media descriptors convert before admission."""
    media = [MediaRef(id="page", kind="document", uri="s3://bucket/page.pdf")]
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"}, media=media)
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_eval_refuses_a_span_without_its_trace(tmp_path: Path) -> None:
    """A span id is only meaningful inside its trace."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"}, span_id="00f067aa0ba902b7")
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_record_refuses_a_table_outside_vala_datasets(tmp_path: Path) -> None:
    """``record`` writes registered caller-owned tables only."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.record("vala.drift.observations", {"value": 1})
    assert _code(raised.value) == "WYRD_SDK_400_INVALID_OBSERVATION"


def test_flush_requires_a_started_writer(tmp_path: Path) -> None:
    """An explicit drain of a state that never started Bifrost is an error."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).flush()
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_shutdown_without_startup_succeeds(tmp_path: Path) -> None:
    """Graceful shutdown is idempotent: a never-started state has no rows."""
    state = _state(tmp_path)
    assert state.shutdown() is None
    assert state.shutdown() is None


@pytest.mark.parametrize("key", [1, 1.5, True, None])
def test_drift_refuses_a_non_string_mapping_key(tmp_path: Path, key: object) -> None:
    """``json.dumps`` would stringify the key, so it is refused before the writer."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({key: 1.0})
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_eval_checks_keys_after_reduction(tmp_path: Path) -> None:
    """Eval refuses a non-``str`` mapping key; a dataclass reduces to ``str`` keys."""

    @dataclass
    class Context:
        """Test-only context whose reduced keys are its field names."""

        answer: str

    run = _state(tmp_path).run()
    with pytest.raises(wyrd.WyrdError) as raised:
        run.observe.eval({1: "yes"})
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"
    with pytest.raises(wyrd.WyrdError) as raised:
        run.observe.eval(Context(answer="yes"))
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_explicit_span_without_trace_is_refused_inside_an_active_span(tmp_path: Path) -> None:
    """An explicit id disables the active-span lookup, so it is never half-merged."""
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    with tracer.start_as_current_span("unit"), pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"}, span_id="00f067aa0ba902b7")
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_active_span_reaches_the_writer(tmp_path: Path) -> None:
    """A valid active span is read without failing the emit before admission."""
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    with tracer.start_as_current_span("unit"), pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"})
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


# ── Initial Card selection ──────────────────────────────────────────────────


def test_run_card_selects_the_initial_view_and_shares_its_invocation(tmp_path: Path) -> None:
    """``run(card=...)`` opens on that Card; later views share its invocation."""
    state = _state(tmp_path)
    model = state.run(card="model")
    assert model.card_ref == state.run().for_card("model").card_ref
    assert UUID(model.run_id).version == 7
    backup = model.for_card("backup")
    assert backup.run_id == model.run_id
    assert model.card_ref != backup.card_ref
    assert state.run(card=None).card_ref.startswith("default/Service/service@1.0.0")


def test_run_card_refuses_an_unknown_alias(tmp_path: Path) -> None:
    """An unknown initial alias fails locally, before any scope is entered."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run(card="missing")
    assert _code(raised.value) == "WYRD_SDK_404_UNKNOWN_ALIAS"


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


def _wyrd_processors(provider: TracerProvider) -> int:
    """Count the Wyrd correlation processors registered on ``provider``."""
    return sum(
        type(processor).__name__ == "_RunCorrelationProcessor"
        for processor in provider._active_span_processor._span_processors
    )


def test_entering_a_run_returns_it_and_correlates_active_and_child_spans(
    tmp_path: Path, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """The active span and every span started in scope carry the exact identity."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = _state(tmp_path).run(card="model")
    with tracer.start_as_current_span("outer"):
        with run as entered:
            assert entered is run
            with tracer.start_as_current_span("child"):
                tracer.start_span("grandchild", attributes={"wyrd.card_ref": "x"}).end()
        tracer.start_span("after").end()
    expected = (run.card_ref, run.run_id)
    assert _correlation(exporter) == {
        "grandchild": expected,
        "child": expected,
        "after": None,
        "outer": expected,
    }


def test_nested_card_scopes_share_the_run_and_restore_the_outer_card(
    tmp_path: Path, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """An inner Card scope restores the outer Card when it exits."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = _state(tmp_path).run()
    model = run.for_card("model")
    with run:
        tracer.start_span("service").end()
        with model:
            tracer.start_span("model").end()
        tracer.start_span("restored").end()
    tracer.start_span("outside").end()
    assert _correlation(exporter) == {
        "service": (run.card_ref, run.run_id),
        "model": (model.card_ref, run.run_id),
        "restored": (run.card_ref, run.run_id),
        "outside": None,
    }


def test_scope_survives_await_and_isolates_concurrent_tasks(
    tmp_path: Path, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Context follows ``await`` and tasks; concurrent scopes never cross."""
    provider, exporter = spans
    tracer = provider.get_tracer("framework")
    run = _state(tmp_path).run()
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
    root = (run.card_ref, run.run_id)
    assert _correlation(exporter) == {
        "task-model": (views["model"].card_ref, run.run_id),
        "task-model-late": (views["model"].card_ref, run.run_id),
        "task-backup": (views["backup"].card_ref, run.run_id),
        "task-backup-late": (views["backup"].card_ref, run.run_id),
        "spawned": root,
        "after": None,
    }


async def _span_later(tracer: Any, name: str) -> None:
    """Yield once, then start and end one span in this task's context."""
    await asyncio.sleep(0)
    tracer.start_span(name).end()


def test_global_and_private_providers_receive_one_processor_each(
    tmp_path: Path, spans: tuple[TracerProvider, InMemorySpanExporter]
) -> None:
    """Registration is idempotent per provider, through entry or the explicit hook."""
    provider, exporter = spans
    run = _state(tmp_path).run()
    with run, run:
        pass
    assert install_run_correlation() is True
    assert _wyrd_processors(provider) == 1

    private = TracerProvider()
    private_exporter = InMemorySpanExporter()
    private.add_span_processor(SimpleSpanProcessor(private_exporter))
    assert install_run_correlation(private) is True
    assert install_run_correlation(private) is True
    assert _wyrd_processors(private) == 1
    with run:
        private.get_tracer("framework").start_span("private").end()
    assert _correlation(private_exporter) == {"private": (run.card_ref, run.run_id)}


def test_unsupported_providers_are_refused_without_raising() -> None:
    """API-only, proxy, and failing providers report False instead of raising."""

    class Failing:
        """A provider whose registration raises."""

        def add_span_processor(self, processor: object) -> None:
            raise RuntimeError("registration failed")

    assert install_run_correlation(object()) is False
    assert install_run_correlation(trace.ProxyTracerProvider()) is False
    failing = Failing()
    assert install_run_correlation(failing) is False
    assert install_run_correlation(failing) is False


def test_missing_opentelemetry_is_a_no_op(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    """Without the optional package a run still enters, exits, and emits normally."""
    for module in ("opentelemetry", "opentelemetry.context", "opentelemetry.trace"):
        monkeypatch.setitem(sys.modules, module, None)
    assert install_run_correlation() is False
    run = _state(tmp_path).run(card="model")
    with run as entered, pytest.raises(wyrd.WyrdError) as raised:
        entered.observe.drift({"latency_ms": 1.0})
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_enrichment_and_detach_failures_never_escape(
    tmp_path: Path,
    spans: tuple[TracerProvider, InMemorySpanExporter],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A failing context lookup or detach is contained; user errors propagate."""
    provider, exporter = spans
    run = _state(tmp_path).run()

    def broken(*_args: object, **_kwargs: object) -> None:
        raise RuntimeError("telemetry broke")

    def scope() -> None:
        with run:
            monkeypatch.setattr(otel_context, "get_value", broken)
            provider.get_tracer("framework").start_span("unenriched").end()
            monkeypatch.setattr(otel_context, "detach", broken)

    # A failed detach leaves its scope attached; a copied context keeps that
    # leak out of the tests that follow on this thread.
    contextvars.copy_context().run(scope)
    monkeypatch.setattr(otel_context, "get_value", ORIGINAL_GET_VALUE)
    monkeypatch.setattr(otel_context, "detach", ORIGINAL_DETACH)
    assert _correlation(exporter) == {"unenriched": None}

    with pytest.raises(ValueError, match="user failure"), run:
        raise ValueError("user failure")
    assert run.__exit__(None, None, None) is False
