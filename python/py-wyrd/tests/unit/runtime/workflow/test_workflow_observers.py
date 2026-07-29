from __future__ import annotations

from pathlib import Path

import pytest
from wyrd import Agent, Observer, OtelObserver, Prompt, Workflow


class RecordingObserver(Observer):
    def __init__(self) -> None:
        self.events: list[str] = []
        self.requests: list[object] = []
        self.responses: list[object] = []

    def on_workflow_start(self, run_id, workflow_id, step_count) -> None:
        self.events.append(f"workflow_start:{workflow_id}:{step_count}")

    def on_workflow_finish(self, run_id, workflow_id, duration_ms) -> None:
        self.events.append(f"workflow_finish:{workflow_id}")

    def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id) -> None:
        self.events.append(f"agent_start:{agent_id}:parent={parent_run_id is not None}")

    def on_model_call(self, run_id, agent_id, iteration, provider, model, request) -> None:
        self.events.append(f"model_call:{agent_id}:{provider}")
        self.requests.append(request)

    def on_model_result(
        self, run_id, agent_id, iteration, finish_reason, synthetic, response
    ) -> None:
        self.events.append(f"model_result:{agent_id}:{synthetic}")
        self.responses.append(response)

    def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms) -> None:
        self.events.append(f"agent_finish:{agent_id}:{finish_reason}")


def _agent(name: str, message: str) -> Agent:
    return Agent(
        prompt=Prompt(messages=[message], model="mock-model", provider="mock"),
        name=name,
        id=name,
    )


def _workflow(observer: Observer | None = None) -> Workflow:
    planner = _agent("planner", "plan ${input}")
    writer = _agent("writer", "write ${input}")
    observers = [] if observer is None else [observer]
    return Workflow.sequential("research", planner, writer, observers=observers)


def test_workflow_accepts_observers_list() -> None:
    observer = RecordingObserver()
    wf = _workflow(observer)

    assert wf.run("topic").final_output == "write topic"
    assert "workflow_start:research:2" in observer.events


def test_workflow_rejects_non_observer_type() -> None:
    agent = _agent("planner", "hello")

    with pytest.raises(TypeError, match="expected Observer subclass"):
        Workflow.sequential("bad", agent, observers=[object()])


def test_workflow_observer_receives_workflow_events() -> None:
    observer = RecordingObserver()

    _workflow(observer).run("topic")

    assert "workflow_start:research:2" in observer.events
    assert "workflow_finish:research" in observer.events


def test_workflow_observer_receives_child_agent_events() -> None:
    observer = RecordingObserver()

    _workflow(observer).run("topic")

    assert "agent_start:planner:parent=True" in observer.events
    assert "model_call:planner:mock" in observer.events
    assert "model_result:writer:False" in observer.events
    assert any(event.startswith("agent_finish:writer:") for event in observer.events)


def test_workflow_observer_typed_provider_request_and_response() -> None:
    observer = RecordingObserver()

    _workflow(observer).run("topic")

    assert observer.requests
    assert observer.responses
    assert observer.requests[0].openai().model == "mock-model"
    assert observer.responses[0].openai().usage.total_tokens == 0


def test_workflow_loaded_from_yaml_has_empty_observers(tmp_path: Path) -> None:
    observer = RecordingObserver()
    path = tmp_path / "workflow.yaml"
    wf = Workflow(name="empty", observers=[observer])
    wf.set_version("0.1.0")
    wf.save(path)

    loaded = Workflow.load(path)

    assert "observer" not in path.read_text()
    assert loaded.to_yaml()
    assert observer.events == []


def test_otel_observer_in_workflow() -> None:
    wf = _workflow(OtelObserver())

    assert wf.run("topic").final_output == "write topic"
