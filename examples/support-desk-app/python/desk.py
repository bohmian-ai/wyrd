"""The support desk: an OpenAI Agents SDK agent that answers customers through
the Wyrd gateway, inside one Wyrd Run per question.

Everything the agent is comes from the Cards in ``service/``, downloaded into
``.wyrd-bundle`` by ``deploy.py``: the model and prompt from
``support-prompt``, the gateway deployment from ``support-agent``, and the
three Verifiers: ``no-refund-promise`` on the Service, and ``ticket-complete``
and ``follows-policy`` on the agent.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from agents import Agent, OpenAIChatCompletionsModel, Runner, set_trace_processors
from openai import AsyncOpenAI, DefaultAsyncHttpxClient
from opentelemetry import trace
from wyrd import verify
from wyrd.client import WyrdClient
from wyrd.observe import Run
from wyrd.otel import WyrdAgentsTraceProcessor
from wyrd.state import WyrdState

ROOT = Path(__file__).resolve().parents[1]
SERVICE = ROOT / "service"
BUNDLE = ROOT / ".wyrd-bundle"
TICKETS = "vala.datasets.tickets"

tracer = trace.get_tracer("support-desk")


class Desk:
    """The hydrated Service, its Bifrost writer and telemetry, and a gateway client."""

    def __init__(self, client: WyrdClient | None = None) -> None:
        # ANCHOR: init
        self.client = client or WyrdClient()
        self.state = WyrdState.from_path(BUNDLE, self.client)
        self.state.start_bifrost()
        # Spans export to Wyrd through the SDK's OTLP exporter; the Agents SDK's
        # own traces become spans too, under the request's Run.
        self.state.start_telemetry()
        set_trace_processors([WyrdAgentsTraceProcessor()])
        # ANCHOR_END: init
        # The gateway speaks the OpenAI API. `http_auth` swaps the placeholder
        # key for a fresh Wyrd access token on every request.
        self.gateway = AsyncOpenAI(
            base_url=f"{self.client.server_url}/v1",
            api_key="wyrd",
            http_client=DefaultAsyncHttpxClient(auth=self.client.http_auth),
        )
        prompt = self.state.card("prompt").spec
        deployment = self.state.card("agent").spec["execution_route"]["deployment"]
        self.model = f"{deployment}/{prompt['model']}"
        self.template: str = prompt["messages"][0]["content"]

    async def answer(self, question: str) -> tuple[Run, str]:
        """Answer ``question`` in a new Run of the Service and record the ticket under it."""
        # ANCHOR: answer
        run = self.state.run()
        with run, tracer.start_as_current_span("support-desk.ask"):
            # These headers tie the gateway's record of the call to this Run.
            headers = {
                "wyrd-run-id": run.run_id,
                "wyrd-card-uid": str(self.state.card_ref("agent").uid),
            }
            model = OpenAIChatCompletionsModel(
                self.model, self.gateway.with_options(default_headers=headers)
            )
            agent = Agent(name="support-agent", model=model)
            result = await Runner.run(agent, self.template.replace("{{question}}", question))
            answer = str(result.final_output)
            run.observe.record(TICKETS, {"question": question, "answer": answer})
        # ANCHOR_END: answer
        return run, answer

    def check(self, run: Run, ticket: dict[str, Any]) -> dict[str, verify.VerificationResult]:
        """Run the two model-free Verifiers on ``ticket`` now and return their results.

        ``no-refund-promise`` is bound to the Service, so it runs on the Run
        itself. ``ticket-complete`` is bound to the agent, so it runs on the
        Run's agent view, which shares the same ``run_id``.
        """
        # ANCHOR: check
        return {
            "no-refund-promise": run.observe.verify("no-refund-promise", answer=ticket["answer"]),
            "ticket-complete": run.for_card("agent").observe.verify(
                "ticket-complete", ticket=ticket
            ),
        }
        # ANCHOR_END: check

    def submit(self, run: Run, letter: str) -> verify.Execution:
        """Queue ``follows-policy`` on the Markdown ``letter`` and return its execution, without waiting."""
        # ANCHOR: submit
        execution = run.for_card("agent").observe.submit(
            "follows-policy", letter=verify.File(letter.encode(), filename="letter.md")
        )
        assert isinstance(execution, verify.Execution)  # Run evidence always queues one
        # ANCHOR_END: submit
        return execution
