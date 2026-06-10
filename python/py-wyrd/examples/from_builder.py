"""Build a workflow with the Python API and execute it locally."""

from wyrd import Agent, Prompt, Workflow, tool


@tool
def web_search(query: str) -> str:
    """Search the web."""
    return f"results for {query}"


def main() -> None:
    planner = Agent(
        name="planner",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=[r'{"summary":"mock summary","steps":["search","write"]}'],
            output={"summary": str, "steps": list[str]},
        ),
        tools=[web_search],
    )
    writer = Agent(
        name="writer",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=["Draft a brief on: ${summary}"],
        ),
    )
    wf = Workflow.sequential("research", planner, writer)
    run = wf.run({"topic": "climate change"})
    print(f"completed {len(run.outcomes)} steps")
    print(f"cross-step params: {run.parameters}")
    print(f"final output: {run.final_output}")


if __name__ == "__main__":
    main()
