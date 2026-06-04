"""Anthropic sequential workflow example; run with `uv run python examples/python/workflow_anthropic.py`."""

from __future__ import annotations

from wyrd import Agent, Prompt, Workflow, tool


@tool(name="web_search", description="Search the web and return result snippets.")
def web_search(query: str) -> list[str]:
    return [f"Result 1 for '{query}'", f"Result 2 for '{query}'"]


def main() -> None:
    researcher = Agent(
        name="researcher",
        version="0.1.0",
        prompt=Prompt.anthropic(
            "claude-3-5-haiku-20241022",
            system=(
                "You are a concise researcher. Use the web_search tool to find "
                "3 key facts about the topic, then summarise them."
            ),
            messages=["Research: {{topic}}"],
            variables=["topic"],
        ),
        tools=[web_search],
    )
    writer = Agent(
        name="writer",
        version="0.1.0",
        prompt=Prompt.anthropic(
            "claude-3-5-haiku-20241022",
            system=(
                "You are a concise writer. Turn the research notes into "
                "a two-sentence summary."
            ),
            messages=["Write a summary from: {{research}}"],
            variables=["research"],
        ),
    )

    workflow = Workflow.sequential("research-and-write", researcher, writer)
    workflow.set_version("0.1.0")
    run = workflow.run("renewable energy storage")

    print(f"steps completed: {len(run.outcomes)}")
    if run.final_step_id:
        print(f"final step status: {run.outcomes[run.final_step_id].status}")


if __name__ == "__main__":
    main()
