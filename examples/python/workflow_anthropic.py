"""Anthropic research-and-write workflow example; run with `uv run python examples/python/workflow_anthropic.py`."""

from __future__ import annotations

from pathlib import Path

from wyrd import Workflow, tool

WORKFLOW = (
    Path(__file__).resolve().parents[1]
    / "workflows"
    / "research-and-write"
    / "anthropic"
    / "workflow.yaml"
)


@tool(name="web_search", description="Search the web and return result snippets.")
def web_search(query: str) -> list[str]:
    return [f"Result 1 for '{query}'", f"Result 2 for '{query}'"]


def main() -> None:
    # The researcher Agent names `web_search` in its `tool_names`; the
    # decorator above registers it before the Workflow loads.
    workflow = Workflow.from_path(WORKFLOW)
    run = workflow.run("renewable energy storage")

    print(f"status: {run.status}")
    print(f"outputs: {run.outputs}")


if __name__ == "__main__":
    main()
