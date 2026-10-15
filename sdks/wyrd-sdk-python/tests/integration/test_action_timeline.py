"""A Service's action is one trace, read back through ``Bifrost.trace_timeline``.

The ``assistant`` Service, instrumented with ``start_telemetry``, records
spans, writes a Run observation and a table row, calls a model through the
Gateway, and verifies its answer with an Eval LLM judge, all in one span.
The timeline read returns that work as one connected, time-ordered tree.
"""

from collections.abc import Callable, Sequence
from pathlib import Path

import openai
import pytest
from opentelemetry import trace
from opentelemetry.propagate import inject
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, SpanRecord, TableConfig
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from .support import ancestry, hydrated, timeline

pytestmark = pytest.mark.integration

SERVICE_SCOPE = "journey.service"
"""Instrumentation scope of the Service's own spans."""

GATEWAY_SCOPE = "wyrd.gateway"
"""Scope of the gateway's captured model-call attempt rows."""

ACTIONS = "vala.datasets.actions"
"""The table the Service writes one row to."""


class Action(BaseModel):
    """One row of the actions table."""

    action: str


def outline(spans: Sequence[SpanRecord]) -> str:
    """One ``scope kind name`` line per span, for a readable assertion message."""
    return "\n".join(f"{span['scope']['name']} {span['kind']} {span['name']}" for span in spans)


def holds_in_order(names: Sequence[str], steps: Sequence[Callable[[str], bool]]) -> bool:
    """Whether ``names`` holds every one of ``steps`` in order, gaps allowed."""
    remaining = iter(names)
    return all(any(step(name) for name in remaining) for step in steps)


def test_service_action_timeline_is_one_connected_tree(
    wyrd_server: WyrdTestServer,
    assistant_bundle: Path,
    assistant_key: str,
    fresh_tracer_provider: None,
    metadata_capture: None,
) -> None:
    Bifrost(TableConfig(Action, ACTIONS)).register()
    service = WyrdClient(credential=assistant_key)
    state = hydrated(assistant_bundle, service)
    state.start_bifrost()
    state.start_telemetry()
    writer = Bifrost(client=service)
    tracer = trace.get_tracer(SERVICE_SCOPE)

    with tracer.start_as_current_span("service.action") as action:
        with tracer.start_as_current_span("answer"):
            pass
        state.run("model").observe.drift({"latency": 12.5})
        writer.use_table_by_name(ACTIONS)
        writer.insert({"action": "answered"})
        with tracer.start_as_current_span("model.ask"):
            headers: dict[str, str] = {}
            inject(headers)
            openai.OpenAI(
                base_url=f"{service.server_url}/v1",
                api_key=service.access_token(),
                max_retries=0,
            ).chat.completions.create(
                model="openai/gpt-4o",
                messages=[{"role": "user", "content": "hi"}],
                extra_headers=headers,
            )
        judgment = state.run("agent").observe.verify("answer-is-judged", {"answer": "yes"})
        assert judgment.passed
    writer.shutdown()
    state.shutdown()

    trace_id = format(action.get_span_context().trace_id, "032x")
    read = timeline(wyrd_server, trace_id)
    starts = [span["start_time"] for span in read]
    assert starts == sorted(starts), read
    spans = [span for span in read if span["trace_id"] == trace_id]
    linked = [span for span in read if span["trace_id"] != trace_id]
    roots = [span for span in spans if span.get("parent_span_id") is None]
    assert [(root["name"], root["scope"]["name"]) for root in roots] == [
        ("service.action", SERVICE_SCOPE)
    ]
    for span in spans:
        assert ancestry(spans, span)[-1] == roots[0], span["name"]
    assert any(span["name"] == "answer" for span in spans)
    producers = [
        span for span in spans if span["kind"] == "producer" and span["name"] == "bifrost.enqueue"
    ]
    assert len(producers) >= 2, outline(spans)
    for producer in producers:
        assert any(
            consumer["name"] == "bifrost.gate.write"
            and consumer["kind"] == "consumer"
            and any(
                (link["trace_id"], link["span_id"]) == (producer["trace_id"], producer["span_id"])
                for link in consumer.get("links", [])
            )
            for consumer in linked
        ), outline(linked)
    attempts = [
        [each["name"] for each in ancestry(spans, span)]
        for span in spans
        if span["scope"]["name"] == GATEWAY_SCOPE
    ]
    assert len(attempts) == 2, attempts
    assert any(
        holds_in_order(
            names,
            [
                lambda name: name == "gateway.call",
                lambda name: name == "request",
                lambda name: name == "model.ask",
            ],
        )
        for names in attempts
    ), attempts
    assert any(
        holds_in_order(
            names,
            [
                lambda name: name == "gateway.call",
                lambda name: name.startswith("verification."),
                lambda name: name == "request",
                lambda name: name == "service.action",
            ],
        )
        for names in attempts
    ), attempts
