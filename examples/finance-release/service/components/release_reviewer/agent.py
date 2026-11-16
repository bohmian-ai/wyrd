"""The release-reviewing component's definition."""

from wyrd.verify import Agent, ExecutionRoute, Prompt

agent = Agent(
    "release-reviewer",
    prompt=Prompt.openai_chat(
        "gpt-4o",
        messages=(
            "Review this proposed customer-report release and explain its business "
            "purpose and access restrictions: {{request}}"
        ),
    ),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
)
