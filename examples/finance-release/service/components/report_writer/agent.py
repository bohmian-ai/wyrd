"""The report-writing component's definition."""

from wyrd.verify import Agent, ExecutionRoute, Prompt

agent = Agent(
    "report-writer",
    prompt=Prompt.openai_chat(
        "gpt-4o",
        messages="Write a customer report using only these source facts: {{source}}.",
    ),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
)
