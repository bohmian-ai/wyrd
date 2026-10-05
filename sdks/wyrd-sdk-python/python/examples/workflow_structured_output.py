"""Two-step structured-output workflow against the live OpenAI API."""

from __future__ import annotations

from pydantic import BaseModel

from wyrd import Agent, Prompt, Workflow


class Plan(BaseModel):
    summary: str
    steps: list[str]


def main() -> None:
    planner = Agent(
        prompt=Prompt(
            provider="openai",
            model="gpt-5.4-nano-2026-03-17",
            system="You produce structured plans.",
            messages=["Plan: ${topic}"],
            output=Plan,
        ),
        name="planner",
    )
    writer = Agent(
        prompt=Prompt(
            provider="openai",
            model="gpt-5.4-nano-2026-03-17",
            system="You write 2-sentence briefs.",
            messages=["Write a brief from this summary: ${summary}"],
        ),
        name="writer",
    )
    wf = (
        Workflow.sequential("demo", planner, writer)
        .with_inputs({"topic": ""})
        .with_step_inputs("planner", {"topic": "input.topic"})
        .with_step_inputs("writer", {"summary": "steps.planner.output.structured.summary"})
        .with_outputs(
            {"plan": "steps.planner.output.structured", "brief": "steps.writer.output.text"}
        )
    )
    run = wf.run({"topic": "the Rust borrow checker"})
    print("status:", run.status)
    print("outputs:", run.outputs)


if __name__ == "__main__":
    main()
