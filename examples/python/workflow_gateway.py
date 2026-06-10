"""OpenAI gateway workflow example; run with `uv run python examples/python/workflow_gateway.py`."""

from __future__ import annotations

import os

from wyrd import Agent, Prompt, Workflow


def main() -> None:
    gateway_url = (
        os.environ.get("GATEWAY_BASE_URL")
        or os.environ.get("OPENAI_BASE_URL")
        or "https://api.openai.com/v1"
    )
    prompt = Prompt.openai_chat(
        "gpt-4o-mini",
        system="You are a helpful assistant.",
        messages=["Summarise: {{topic}}"],
        variables=["topic"],
    )

    researcher = Agent(
        name="researcher",
        version="0.1.0",
        prompt=prompt,
        provider_base_url=gateway_url,
    )
    writer = Agent(
        name="writer",
        version="0.1.0",
        prompt=prompt,
    )

    workflow = Workflow.sequential("gateway-demo", researcher, writer)
    workflow.set_version("0.1.0")
    run = workflow.run("renewable energy")

    print(f"steps completed: {len(run.outcomes)}")
    if run.final_step_id:
        print(f"final step status: {run.outcomes[run.final_step_id].status}")


if __name__ == "__main__":
    main()
