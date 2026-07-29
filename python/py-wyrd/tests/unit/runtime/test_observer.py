"""Tests for wyrd.Observer base class and threading behavior."""

from __future__ import annotations

import threading

from wyrd import Observer


def test_observer_subclass_no_overrides_no_error():
    class Empty(Observer):
        pass

    obs = Empty()
    obs.on_agent_start("r", None, "a", "hi", None)
    obs.on_agent_finish("r", "a", "model_stopped", 1, 100)
    obs.on_agent_error("r", "a", "WYRD_ERR", "something went wrong")
    obs.on_workflow_start("r", "wf", 3)
    obs.on_workflow_finish("r", "wf", 500)


def test_observer_receives_all_event_types():
    received: list[str] = []

    class Recording(Observer):
        def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id):
            received.append(f"start:{run_id}")

        def on_model_call(self, run_id, agent_id, iteration, provider, model):
            received.append(f"model_call:{run_id}:{iteration}")

        def on_model_result(self, run_id, agent_id, iteration, finish_reason, synthetic):
            received.append(f"model_result:{run_id}:{iteration}")

        def on_tool_call(self, run_id, agent_id, iteration, call_id, tool_name):
            received.append(f"tool_call:{run_id}:{call_id}")

        def on_tool_result(self, run_id, agent_id, iteration, call_id, ok):
            received.append(f"tool_result:{run_id}:{call_id}:{ok}")

        def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms):
            received.append(f"finish:{run_id}")

        def on_workflow_start(self, run_id, workflow_id, step_count):
            received.append(f"wf_start:{run_id}")

        def on_workflow_finish(self, run_id, workflow_id, duration_ms):
            received.append(f"wf_finish:{run_id}")

    obs = Recording()
    obs.on_agent_start("r1", None, "a", "input", None)
    obs.on_model_call("r1", "a", 0, "openai", "gpt-4o")
    obs.on_model_result("r1", "a", 0, "stop", False)
    obs.on_tool_call("r1", "a", 0, "call-1", "search")
    obs.on_tool_result("r1", "a", 0, "call-1", True)
    obs.on_agent_finish("r1", "a", "model_stopped", 1, 42)
    obs.on_workflow_start("wf-r1", "my-wf", 2)
    obs.on_workflow_finish("wf-r1", "my-wf", 200)

    assert received == [
        "start:r1",
        "model_call:r1:0",
        "model_result:r1:0",
        "tool_call:r1:call-1",
        "tool_result:r1:call-1:True",
        "finish:r1",
        "wf_start:wf-r1",
        "wf_finish:wf-r1",
    ]


def test_observer_concurrent_calls_different_run_ids():
    events_a: list[str] = []
    events_b: list[str] = []
    lock_a = threading.Lock()
    lock_b = threading.Lock()

    class Tracking(Observer):
        def __init__(self, bucket, lock):
            self._bucket = bucket
            self._lock = lock

        def on_agent_start(self, run_id, parent_run_id, agent_id, input, session_id):
            with self._lock:
                self._bucket.append(f"start:{run_id}")

        def on_agent_finish(self, run_id, agent_id, finish_reason, iterations, duration_ms):
            with self._lock:
                self._bucket.append(f"finish:{run_id}")

    obs_a = Tracking(events_a, lock_a)
    obs_b = Tracking(events_b, lock_b)

    barrier = threading.Barrier(2)

    def run_a():
        barrier.wait()
        obs_a.on_agent_start("run-a", None, "agent-a", "x", None)
        obs_a.on_agent_finish("run-a", "agent-a", "stop", 1, 10)

    def run_b():
        barrier.wait()
        obs_b.on_agent_start("run-b", None, "agent-b", "y", None)
        obs_b.on_agent_finish("run-b", "agent-b", "stop", 1, 10)

    ta = threading.Thread(target=run_a)
    tb = threading.Thread(target=run_b)
    ta.start()
    tb.start()
    ta.join()
    tb.join()

    assert all("run-a" in event for event in events_a)
    assert all("run-b" in event for event in events_b)
