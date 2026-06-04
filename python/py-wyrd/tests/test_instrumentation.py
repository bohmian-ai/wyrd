"""Tests for OtelObserver and WyrdInstrumentor."""

from __future__ import annotations

import threading

from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter
from wyrd import Observer, OtelObserver, WyrdInstrumentor


def _make_provider():
    """Return a TracerProvider with an in-memory exporter."""
    exporter = InMemorySpanExporter()
    provider = TracerProvider()
    provider.add_span_processor(SimpleSpanProcessor(exporter))
    return provider, exporter


def test_otel_observer_agent_start_creates_span():
    provider, exporter = _make_provider()
    obs = OtelObserver(tracer=provider.get_tracer("wyrd"))
    obs.on_agent_start("run-1", None, "my-agent", "hello", None)
    obs.on_agent_finish("run-1", "my-agent", "model_stopped", 1, 50)
    spans = exporter.get_finished_spans()
    assert len(spans) == 1
    assert "wyrd.agent.run" in spans[0].name
    assert spans[0].attributes["wyrd.run_id"] == "run-1"


def test_otel_observer_model_call_creates_span():
    provider, exporter = _make_provider()
    obs = OtelObserver(tracer=provider.get_tracer("wyrd"))
    obs.on_agent_start("run-2", None, "a", "x", None)
    obs.on_model_call("run-2", "a", 0, "openai", "gpt-4o")
    obs.on_model_result("run-2", "a", 0, "stop", False)
    obs.on_agent_finish("run-2", "a", "model_stopped", 1, 30)
    spans = exporter.get_finished_spans()
    names = {span.name for span in spans}
    assert any("model.call" in name for name in names)
    assert any("agent.run" in name for name in names)


def test_otel_observer_concurrent_runs_no_collision():
    provider, exporter = _make_provider()
    obs = OtelObserver(tracer=provider.get_tracer("wyrd"))
    barrier = threading.Barrier(2)
    errors: list[Exception] = []

    def run_agent(run_id: str):
        try:
            barrier.wait()
            obs.on_agent_start(run_id, None, "a", "x", None)
            obs.on_model_call(run_id, "a", 0, "openai", "gpt-4o")
            obs.on_model_result(run_id, "a", 0, "stop", False)
            obs.on_agent_finish(run_id, "a", "model_stopped", 1, 10)
        except Exception as exc:
            errors.append(exc)

    ta = threading.Thread(target=run_agent, args=("concurrent-a",))
    tb = threading.Thread(target=run_agent, args=("concurrent-b",))
    ta.start()
    tb.start()
    ta.join()
    tb.join()

    assert not errors, f"Observer raised during concurrent access: {errors}"
    spans = exporter.get_finished_spans()
    assert len(spans) == 4


def test_otel_observer_lock_isolates_span_store():
    provider, exporter = _make_provider()
    obs = OtelObserver(tracer=provider.get_tracer("wyrd"))
    errors: list[Exception] = []
    barrier = threading.Barrier(10)

    def run(i: int):
        run_id = f"iso-{i}"
        try:
            barrier.wait()
            obs.on_agent_start(run_id, None, "a", "x", None)
            obs.on_agent_finish(run_id, "a", "stop", 1, 5)
        except Exception as exc:
            errors.append(exc)

    threads = [threading.Thread(target=run, args=(i,)) for i in range(10)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()

    assert not errors
    assert len(exporter.get_finished_spans()) == 10


def test_composite_observer_fans_out():
    received_a: list[str] = []
    received_b: list[str] = []

    class Rec(Observer):
        def __init__(self, bucket):
            self._bucket = bucket

        def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id):
            self._bucket.append(run_id)

    from wyrd.otel import _CompositeObserver

    obs = _CompositeObserver([Rec(received_a), Rec(received_b)])
    obs.on_agent_start("fan-out", None, "a", "x", None)
    assert received_a == ["fan-out"]
    assert received_b == ["fan-out"]


def test_instrumentor_no_args_installs_otel_observer():
    WyrdInstrumentor().instrument()


def test_instrumentor_custom_observer_accepted():
    class Noop(Observer):
        pass

    WyrdInstrumentor().instrument(observer=Noop())


def test_instrumentor_multiple_observers():
    received: list[str] = []

    class Rec(Observer):
        def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id):
            received.append(run_id)

    WyrdInstrumentor().instrument(observers=[Rec(), Rec()])
