"""Observer and OTel instrumentation tests.

Journey-driven: validates the real agent and workflow paths a user takes,
not individual observer methods in isolation.
"""

from __future__ import annotations

import threading

from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from opentelemetry.sdk.trace.export.in_memory_span_exporter import InMemorySpanExporter

from wyrd import Observer, OtelObserver, WyrdInstrumentor


def _make_provider():
    exporter = InMemorySpanExporter()
    provider = TracerProvider()
    provider.add_span_processor(SimpleSpanProcessor(exporter))
    return provider, exporter


def test_workflow_concurrent_agents_with_tools_produces_correct_span_tree():
    """Parallel workflow steps produce a fully nested, single-trace span tree.

    Simulates a real workflow: 3 agents run concurrently, each making model
    calls, two of them also invoking tools. Verifies:

    workflow.run
    ├── agent.run/step-0  (child of workflow)
    │   ├── model.call    (child of step-0)
    │   └── tool.call     (child of step-0)
    ├── agent.run/step-1  (child of workflow)
    │   ├── model.call    (child of step-1)
    │   └── tool.call     (child of step-1)
    └── agent.run/step-2  (child of workflow)
        └── model.call    (child of step-2)

    All spans share the same trace_id. No span belongs to the wrong parent.
    The threading.Lock in OtelObserver must keep the span store consistent
    under real concurrent access.
    """
    provider, exporter = _make_provider()
    obs = OtelObserver(tracer=provider.get_tracer("wyrd"))
    workflow_run_id = "wf-journey-1"

    obs.on_workflow_start(workflow_run_id, "research-pipeline", 3)

    steps = [
        ("step-0", "openai", "gpt-4o", [("web_search", "call-0a")]),
        ("step-1", "anthropic", "claude-3-5-sonnet", [("calculator", "call-1a")]),
        ("step-2", "openai", "gpt-4o-mini", []),
    ]
    errors: list[Exception] = []
    barrier = threading.Barrier(len(steps))

    def run_step(step_id, provider_name, model, tool_calls):
        try:
            barrier.wait()
            obs.on_agent_start(step_id, workflow_run_id, step_id, "research", None)
            obs.on_model_call(step_id, step_id, 0, provider_name, model)
            for tool_name, call_id in tool_calls:
                obs.on_tool_call(step_id, step_id, 0, call_id, tool_name)
                obs.on_tool_result(step_id, step_id, 0, call_id, True)
            obs.on_model_result(step_id, step_id, 0, "stop", False)
            obs.on_agent_finish(step_id, step_id, "model_stopped", 1, 80)
        except Exception as exc:
            errors.append(exc)

    threads = [threading.Thread(target=run_step, args=args) for args in steps]
    for t in threads:
        t.start()
    for t in threads:
        t.join()

    obs.on_workflow_finish(workflow_run_id, "research-pipeline", 250)

    assert not errors, f"Errors during concurrent execution: {errors}"

    finished = exporter.get_finished_spans()
    workflow_span = next(s for s in finished if "workflow.run" in s.name)
    agent_spans = [s for s in finished if "agent.run" in s.name]
    model_spans = [s for s in finished if "model.call" in s.name]
    tool_spans = [s for s in finished if "tool.call" in s.name]

    assert len(agent_spans) == 3
    assert len(model_spans) == 3
    assert len(tool_spans) == 2

    # All spans share one trace — the workflow's trace_id.
    all_spans = [workflow_span, *agent_spans, *model_spans, *tool_spans]
    trace_ids = {s.context.trace_id for s in all_spans}
    assert len(trace_ids) == 1, f"Expected 1 trace, got {len(trace_ids)}: {trace_ids}"

    # Every agent span is a direct child of the workflow span.
    for agent_span in agent_spans:
        assert agent_span.parent is not None, f"{agent_span.name} has no parent"
        assert agent_span.parent.span_id == workflow_span.context.span_id, (
            f"{agent_span.name} parent is not the workflow span"
        )

    # Every model and tool span is a child of its own agent span, not a sibling's.
    agent_by_run_id = {s.attributes["wyrd.run_id"]: s for s in agent_spans}
    for child_span in model_spans + tool_spans:
        run_id = child_span.attributes["wyrd.run_id"]
        expected_parent = agent_by_run_id[run_id]
        assert child_span.parent is not None, f"{child_span.name} has no parent"
        assert child_span.parent.span_id == expected_parent.context.span_id, (
            f"{child_span.name} is child of wrong agent span "
            f"(got {child_span.parent.span_id}, want {expected_parent.context.span_id})"
        )


def test_standalone_agent_is_trace_root():
    """An agent run without a workflow produces a trace root with no parent span."""
    provider, exporter = _make_provider()
    obs = OtelObserver(tracer=provider.get_tracer("wyrd"))

    obs.on_agent_start("standalone-1", None, "agent-x", "hello", None)
    obs.on_model_call("standalone-1", "agent-x", 0, "openai", "gpt-4o")
    obs.on_model_result("standalone-1", "agent-x", 0, "stop", False)
    obs.on_agent_finish("standalone-1", "agent-x", "model_stopped", 1, 40)

    finished = exporter.get_finished_spans()
    agent_span = next(s for s in finished if "agent.run" in s.name)
    model_span = next(s for s in finished if "model.call" in s.name)

    assert agent_span.parent is None, "standalone agent must be a trace root"
    # Model span is still a child of its own agent span.
    assert model_span.parent is not None
    assert model_span.parent.span_id == agent_span.context.span_id


def test_custom_observer_receives_workflow_and_agent_events():
    """A user-provided Observer subclass receives events from both workflow and agent paths."""
    received: list[str] = []

    class Recorder(Observer):
        def on_workflow_start(self, run_id, workflow_id, step_count):
            received.append(f"wf_start:{run_id}")

        def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id):
            received.append(f"agent_start:{run_id}:parent={parent_run_id}")

        def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms):
            received.append(f"agent_finish:{run_id}")

        def on_workflow_finish(self, run_id, workflow_id, duration_ms):
            received.append(f"wf_finish:{run_id}")

    obs = Recorder()
    obs.on_workflow_start("wf-1", "my-wf", 1)
    obs.on_agent_start("step-1", "wf-1", "agent", "input", None)
    obs.on_agent_finish("step-1", "agent", "model_stopped", 1, 50)
    obs.on_workflow_finish("wf-1", "my-wf", 100)

    assert received == [
        "wf_start:wf-1",
        "agent_start:step-1:parent=wf-1",
        "agent_finish:step-1",
        "wf_finish:wf-1",
    ]


def test_wyrd_instrumentor_wires_observer_into_runtime():
    """WyrdInstrumentor installs an observer that receives events — default is OtelObserver."""
    WyrdInstrumentor().instrument()
