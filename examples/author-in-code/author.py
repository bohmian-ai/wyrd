"""Author a Service, its Agent, and its Verifier in code, declare them, verify
two answers, and record each answer in the Service's table."""

from pydantic import BaseModel
from wyrd import WyrdState
from wyrd.verify import (
    Agent,
    Check,
    ComparisonOperator,
    Component,
    ExecutionRoute,
    Prompt,
    Requirement,
    Service,
    Table,
    Verifier,
)


# The Verifier's evidence: one slot per field.
class Inputs(BaseModel):
    answer: str


# A row of the Service's table: one column per field, in order.
class Reply(BaseModel):
    answer: str
    verdict: str


# The Verifier: one requirement over the evidence slot named `answer`.
no_refund_guarantee = Verifier(
    "no-refund-guarantee",
    inputs=Inputs,
    requirements=[
        Requirement(
            "no-refund-guarantee",
            "The answer never promises a guaranteed refund.",
            Check.assert_("inputs.answer", ComparisonOperator.NotContains, "guaranteed refund"),
        )
    ],
)

# The component: an Agent and the Prompt it runs.
refund_agent = Agent(
    "refund-agent",
    prompt=Prompt.openai_chat("gpt-4o", messages="Answer the refund question: {{question}}"),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
)

# The Service: the Agent under the alias `agent`, verified by the Verifier,
# and a table for the answers it gives.
service = Service(
    "refund-replies",
    tables=[Table("refund_replies", Reply)],
    components=[Component("agent", refund_agent, verified_by=[no_refund_guarantee])],
)

# No Card names a version: declaration selects each one and returns them.
state = WyrdState.declare(service)
print(f"service: {state.root_ref}")
print(f"agent:   {state.card_ref('agent')}")

state.start_bifrost()  # the writer; the one declared table is active
try:
    for answer in ("Your refund request is approved.", "You have a guaranteed refund."):
        # A Run whose view is the Agent, where the Verifier is bound.
        with state.run("agent") as run:
            result = run.observe.verify("no-refund-guarantee", answer=answer)
            # Buffered, not yet stored: shutdown() drains it.
            run.bifrost.write(Reply(answer=answer, verdict=str(result.verdict)))
        print(f"{result.verdict}: {answer}")
finally:
    state.shutdown()
