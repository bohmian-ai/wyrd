"""An OpenAI Agents SDK run, traced through ``WyrdAgentsTraceProcessor``, lands in Bifrost.

The application installs the processor itself and runs the agent inside a Wyrd
Run with ``start_telemetry`` running; the agent's spans read back attributed to
that Run. The agent's model is local, so no provider is called.
"""

from __future__ import annotations

from typing import Any

import pytest
from agents import Agent, ModelResponse, Runner, Usage, set_trace_processors, set_tracing_disabled
from agents.models.interface import Model
from agents.tracing import generation_span
from openai.types.responses import ResponseOutputMessage, ResponseOutputText
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.otel import WyrdAgentsTraceProcessor
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration


class LocalModel(Model):
    """A model answering ``hi`` that records its call in a generation span, as a provider model does."""

    async def get_response(self, *args: Any, **kwargs: Any) -> ModelResponse:
        """Answer ``hi`` inside a ``local-model`` generation span using 3 input tokens."""
        with generation_span(model="local-model", usage={"input_tokens": 3, "output_tokens": 1}):
            return ModelResponse(
                output=[
                    ResponseOutputMessage(
                        id="msg_local",
                        type="message",
                        role="assistant",
                        status="completed",
                        content=[ResponseOutputText(type="output_text", text="hi", annotations=[])],
                    )
                ],
                usage=Usage(requests=1, input_tokens=3, output_tokens=1, total_tokens=4),
                response_id=None,
            )

    def stream_response(self, *args: Any, **kwargs: Any) -> Any:
        """The local model does not stream."""
        raise NotImplementedError


class SpanRow(BaseModel):
    """The ``vala.traces.spans`` columns the agent story reads."""

    name: str
    trace_id: bytes
    gen_ai_request_model: str | None
    gen_ai_usage_input_tokens: int | None


@pytest.mark.usefixtures("fresh_tracer_provider")
def test_agents_sdk_run_exports_spans_attributed_to_the_wyrd_run(
    wyrd_server: WyrdTestServer, bifrost: Bifrost, observed: WyrdState
) -> None:
    observed.start_telemetry()
    set_tracing_disabled(False)
    set_trace_processors([WyrdAgentsTraceProcessor()])
    run = observed.run("agent")

    with run:
        result = Runner.run_sync(Agent(name="assistant", model=LocalModel()), "hello")
    observed.shutdown()
    wyrd_server.flush_bifrost()

    assert result.final_output == "hi"
    rows = bifrost.sql(
        "SELECT name, trace_id, gen_ai_request_model, gen_ai_usage_input_tokens "
        "FROM vala.traces.spans WHERE run_id = $1 AND scope_name = 'wyrd.agents'",
        [run.run_id],
        model=SpanRow,
    )
    spans = {row.name: row for row in rows}
    assert spans["assistant"].gen_ai_request_model is None
    assert (
        spans["generation"].gen_ai_request_model,
        spans["generation"].gen_ai_usage_input_tokens,
    ) == ("local-model", 3)
    assert len({row.trace_id for row in rows}) == 1
