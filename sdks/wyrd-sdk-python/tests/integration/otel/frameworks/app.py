"""An agent application, one process per invocation, traced into Wyrd by its framework.

``python -m tests.integration.otel.frameworks.app FRAMEWORK SETUP SCENARIO`` builds one Pydantic AI, Google ADK,
or OpenAI Agents SDK agent on a scripted local model, installs telemetry the
way the docs recipe does, runs ``SCENARIO``, and prints one JSON report naming
each case's trace and Run ids. Wyrd's endpoint and credential come only from
the environment, as in a deployment, and the bundle path from
``WYRD_TEST_BUNDLE``.

A real process matters: OpenTelemetry installs one global provider per
process, and Google ADK binds its tracer at import, so each setup gets its own.

``SETUP`` is ``managed`` (``WyrdState.start_telemetry``) or ``owned`` (an
application ``TracerProvider`` with ``span_exporter`` and
``install_run_correlation``). ``SCENARIO`` is ``run`` (one tool-calling turn
inside an enclosing span), ``edges`` (model and tool failures, a stream that
completes after its Run exits, nested and concurrent Runs, and an
uncorrelated turn), ``live`` (managed only: turns and observations before and
after a live ``state.flush``, which the parent reads back in between, then a
turn after ``state.shutdown``), or ``gateway`` (OpenAI Agents only: the model
client reaches the Wyrd gateway with ``WyrdClient.http_auth``).
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import threading
from collections.abc import Awaitable, Callable
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from opentelemetry import trace
from wyrd.client import WyrdClient
from wyrd.state import WyrdState

from ...support import hydrated

MODEL = "local-model"
"""The scripted model's name; its spans carry it as ``gen_ai.request.model``."""

PROMPT = "SECRET-PROMPT"
"""Sentinel prompt content no span may carry."""

TOOL_RESULT = "SECRET-RESULT"
"""Sentinel tool output no span may carry."""

Ask = Callable[[str, bool], Awaitable[str]]


class LocalModel(BaseHTTPRequestHandler):
    """A scripted OpenAI Chat Completions and Gemini ``generateContent`` upstream.

    A first turn offering tools calls ``lookup``; every later turn answers
    ``done``. Each answer reports 7 input and 2 output tokens. A request
    naming ``FAIL-MODEL`` fails with 500, and one naming ``FAIL-TOOL`` passes
    that topic to the tool, which raises.
    """

    def do_POST(self) -> None:
        body = json.loads(self.rfile.read(int(self.headers["content-length"] or 0)) or b"{}")
        text = json.dumps(body)
        if "FAIL-MODEL" in text:
            self._send(500, {"error": {"code": 500, "message": "upstream failed"}})
        elif self.path.endswith("/chat/completions"):
            self._openai(body, text)
        else:
            self._gemini(body, text)

    def _send(self, status: int, payload: Any, events: list[str] | None = None) -> None:
        """Answer with ``payload`` as JSON, or with serialized ``events`` as server-sent events."""
        data = (
            "".join(f"data: {event}\n\n" for event in events).encode()
            if events is not None
            else json.dumps(payload).encode()
        )
        self.send_response(status)
        self.send_header("content-type", "text/event-stream" if events else "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _openai(self, body: dict[str, Any], text: str) -> None:
        """Answer one Chat Completions turn, streamed when the request asks."""
        topic = "FAIL-TOOL" if "FAIL-TOOL" in text else "x"
        message: dict[str, Any]
        if body.get("tools") and not any(m.get("role") == "tool" for m in body["messages"]):
            call = {"name": "lookup", "arguments": json.dumps({"topic": topic})}
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [{"id": "call_1", "type": "function", "function": call}],
            }
            finish = "tool_calls"
        else:
            message = {"role": "assistant", "content": "done"}
            finish = "stop"
        usage = {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9}
        head = {"id": "c", "created": 1, "model": body["model"]}
        if not body.get("stream"):
            choice = {"index": 0, "message": message, "finish_reason": finish}
            self._send(
                200, {**head, "object": "chat.completion", "choices": [choice], "usage": usage}
            )
            return
        if "tool_calls" in message:
            message["tool_calls"] = [{"index": 0, **message["tool_calls"][0]}]
        chunk = {**head, "object": "chat.completion.chunk"}
        self._send(
            200,
            None,
            [
                json.dumps(
                    {**chunk, "choices": [{"index": 0, "delta": message, "finish_reason": None}]}
                ),
                json.dumps(
                    {**chunk, "choices": [{"index": 0, "delta": {}, "finish_reason": finish}]}
                ),
                json.dumps({**chunk, "choices": [], "usage": usage}),
                "[DONE]",
            ],
        )

    def _gemini(self, body: dict[str, Any], text: str) -> None:
        """Answer one ``generateContent`` turn, as one event when streamed."""
        topic = "FAIL-TOOL" if "FAIL-TOOL" in text else "x"
        if body.get("tools") and "functionResponse" not in text:
            parts: list[Any] = [{"functionCall": {"name": "lookup", "args": {"topic": topic}}}]
        else:
            parts = [{"text": "done"}]
        answer = {
            "candidates": [{"content": {"role": "model", "parts": parts}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 7, "candidatesTokenCount": 2},
            "modelVersion": MODEL,
        }
        self._send(200, answer, [json.dumps(answer)] if ":stream" in self.path else None)

    def log_message(self, format: str, *args: object) -> None:
        """Keep per-request access lines out of the report."""


def lookup(topic: str) -> str:
    """Look a topic up."""
    if topic == "FAIL-TOOL":
        raise RuntimeError("lookup failed")
    return f"{TOOL_RESULT} {topic}"


def pydantic_ai(url: str) -> Ask:
    """A Pydantic AI agent with native instrumentation that omits content."""
    from pydantic_ai import Agent, Tool
    from pydantic_ai.models.instrumented import InstrumentationSettings
    from pydantic_ai.models.openai import OpenAIChatModel
    from pydantic_ai.providers.openai import OpenAIProvider

    Agent.instrument_all(InstrumentationSettings(include_content=False))
    model = OpenAIChatModel(MODEL, provider=OpenAIProvider(base_url=f"{url}/v1", api_key="local"))
    agent = Agent(model, tools=[Tool(lookup, takes_ctx=False)])

    async def ask(prompt: str, stream: bool) -> str:
        if not stream:
            return (await agent.run(prompt)).output
        async with agent.run_stream(prompt) as result:
            return await result.get_output()

    return ask


def google_adk(url: str) -> Ask:
    """A Google ADK agent; ADK's native spans omit content via its environment switch."""
    from google.adk.agents import LlmAgent
    from google.adk.agents.run_config import RunConfig, StreamingMode
    from google.adk.models.google_llm import Gemini
    from google.adk.runners import InMemoryRunner
    from google.genai import types

    agent = LlmAgent(name="assistant", model=Gemini(model=MODEL, base_url=url), tools=[lookup])
    runner = InMemoryRunner(agent=agent)

    async def ask(prompt: str, stream: bool) -> str:
        session = await runner.session_service.create_session(app_name=runner.app_name, user_id="u")
        mode = StreamingMode.SSE if stream else StreamingMode.NONE
        answer = ""
        async for event in runner.run_async(
            user_id="u",
            session_id=session.id,
            new_message=types.Content(role="user", parts=[types.Part(text=prompt)]),
            run_config=RunConfig(streaming_mode=mode),
        ):
            answer = (
                event.content.parts[0].text or answer
                if event.content and event.content.parts
                else answer
            )
        return answer

    return ask


def openai_agents(url: str) -> Ask:
    """An OpenAI Agents SDK agent whose traces the Wyrd processor alone receives."""
    from agents import (
        Agent,
        OpenAIChatCompletionsModel,
        Runner,
        function_tool,
        set_trace_processors,
    )
    from openai import AsyncOpenAI
    from wyrd.otel import WyrdAgentsTraceProcessor

    set_trace_processors([WyrdAgentsTraceProcessor()])
    client = AsyncOpenAI(base_url=f"{url}/v1", api_key="local", max_retries=0)
    agent = Agent(
        name="assistant",
        model=OpenAIChatCompletionsModel(MODEL, client),
        tools=[function_tool(lookup)],
    )

    async def ask(prompt: str, stream: bool) -> str:
        if not stream:
            return str((await Runner.run(agent, prompt)).final_output)
        result = Runner.run_streamed(agent, prompt)
        async for _ in result.stream_events():
            pass
        return str(result.final_output)

    return ask


def gateway() -> Ask:
    """The OpenAI Agents SDK agent on the ambient Wyrd gateway's ``gpt-4o``, without tools."""
    from agents import Agent, OpenAIChatCompletionsModel, Runner, set_trace_processors
    from openai import AsyncOpenAI, DefaultAsyncHttpxClient
    from wyrd.otel import WyrdAgentsTraceProcessor

    set_trace_processors([WyrdAgentsTraceProcessor()])
    wyrd = WyrdClient()
    client = AsyncOpenAI(
        base_url=f"{wyrd.server_url}/v1",
        api_key="wyrd",
        http_client=DefaultAsyncHttpxClient(auth=wyrd.http_auth),
        max_retries=0,
    )
    agent = Agent(name="assistant", model=OpenAIChatCompletionsModel("openai/gpt-4o", client))

    async def ask(prompt: str, stream: bool) -> str:
        return str((await Runner.run(agent, prompt)).final_output)

    return ask


async def run(state: WyrdState, ask: Ask) -> dict[str, Any]:
    """One turn in a Run inside the application's enclosing ``request`` span."""
    with trace.get_tracer("framework-app").start_as_current_span("request") as span:
        current = state.run("agent")
        with current:
            output = await ask(PROMPT, False)
    return {"run": {"trace_id": _trace_id(span), "run_id": current.run_id, "output": output}}


async def edges(state: WyrdState, ask: Ask) -> dict[str, Any]:
    """Failures, a stream finishing after its Run exits, nested and concurrent Runs, no Run."""
    tracer = trace.get_tracer("framework-app")
    report: dict[str, Any] = {}
    for label, prompt in (("model_failure", "FAIL-MODEL"), ("tool_failure", "FAIL-TOOL")):
        with tracer.start_as_current_span(label) as span:
            current = state.run("agent")
            with current:
                try:
                    await ask(prompt, False)
                    error = None
                except Exception as failure:
                    error = type(failure).__name__
        report[label] = {"trace_id": _trace_id(span), "run_id": current.run_id, "error": error}

    with tracer.start_as_current_span("stream") as span:
        current = state.run("agent")
        with current:
            streaming = asyncio.ensure_future(ask("hello", True))
        output = await streaming
    report["stream"] = {"trace_id": _trace_id(span), "run_id": current.run_id, "output": output}

    with tracer.start_as_current_span("nested") as span:
        outer = state.run("agent")
        with outer:
            inner = state.run("agent")
            with inner:
                await ask("inner", False)
            await ask("outer", False)
    report["nested"] = {"trace_id": _trace_id(span), "outer": outer.run_id, "inner": inner.run_id}

    async def concurrent() -> str:
        current = state.run("agent")
        with current:
            await ask("hello", False)
        return current.run_id

    with tracer.start_as_current_span("concurrent") as span:
        run_ids = await asyncio.gather(concurrent(), concurrent())
    report["concurrent"] = {"trace_id": _trace_id(span), "run_ids": run_ids}

    with tracer.start_as_current_span("uncorrelated") as span:
        await ask("hello", False)
    report["uncorrelated"] = {"trace_id": _trace_id(span)}
    return report


async def live(state: WyrdState, ask: Ask) -> dict[str, Any]:
    """Work before and after a live ``state.flush``, then after ``state.shutdown``.

    Once the flush returns, the first turn and observation are printed as one
    report line and the process waits on stdin while the parent reads them
    back. A span stays open across the flush and ends before shutdown. After
    shutdown, one more turn exports through the same global provider, flushed
    by the application.
    """
    state.start_bifrost()
    held = trace.get_tracer("framework-app").start_span("held")
    first = await observed(state, ask)
    state.flush()
    print(json.dumps({"first": first, "card_uid": str(state.card_ref("agent").uid)}), flush=True)
    sys.stdin.readline()
    second = await observed(state, ask)
    held.end()
    state.shutdown()
    after = (await run(state, ask))["run"]
    provider: Any = trace.get_tracer_provider()
    assert provider.force_flush()
    return {"second": second, "after": after, "held": _trace_id(held)}


async def observed(state: WyrdState, ask: Ask) -> dict[str, Any]:
    """One turn, then one drift observation through the state's native writer."""
    turn = (await run(state, ask))["run"]
    observation = state.run("model")
    observation.observe.drift({"latency": 1.0})
    return {**turn, "observation": observation.run_id}


def _trace_id(span: Any) -> str:
    """``span``'s W3C trace id as 32 lowercase hex digits."""
    return format(span.get_span_context().trace_id, "032x")


def main(framework: str, setup: str, scenario: str) -> None:
    """Install ``setup``, run ``scenario`` on ``framework``, shut down, and print the report."""
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), LocalModel)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{upstream.server_address[1]}"
    state = hydrated(Path(os.environ["WYRD_TEST_BUNDLE"]))
    if setup == "managed":
        state.start_telemetry()
    else:
        from opentelemetry.sdk.trace import TracerProvider
        from opentelemetry.sdk.trace.export import BatchSpanProcessor
        from wyrd.otel import install_run_correlation, span_exporter

        provider = TracerProvider()
        provider.add_span_processor(BatchSpanProcessor(span_exporter(WyrdClient())))
        install_run_correlation(provider)
        trace.set_tracer_provider(provider)
    builders = {
        "pydantic_ai": pydantic_ai,
        "google_adk": google_adk,
        "openai_agents": openai_agents,
    }
    ask = gateway() if scenario == "gateway" else builders[framework](url)
    scenarios = {"edges": edges, "live": live}
    report = asyncio.run(scenarios.get(scenario, run)(state, ask))
    if setup == "owned":
        provider.shutdown()
    elif scenario != "live":  # live shuts its state down itself
        state.shutdown()
    report["card_uid"] = str(state.card_ref("agent").uid)
    print(json.dumps(report))


if __name__ == "__main__":
    main(*sys.argv[1:4])
