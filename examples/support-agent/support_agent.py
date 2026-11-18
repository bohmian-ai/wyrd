from openai import OpenAI
from pydantic import BaseModel
from wyrd import WyrdState
from wyrd.verify import (
    Agent,
    Check,
    ComparisonOperator,
    ExecutionRoute,
    Prompt,
    Requirement,
    ResponseFormat,
    Service,
    Verifier,
)

POLICY = "Refunds are possible within 30 days of purchase. Never promise a refund."


# The evidence the application sends for each reply.
class Reply(BaseModel):
    question: str
    answer: str


# The judge: a model that reads the reply and decides whether it follows the
# policy. Wyrd runs it through the `openai` gateway deployment.
policy_judge = Agent(
    "policy-judge",
    prompt=Prompt.openai_chat(
        "gpt-4o-mini",
        messages=f"Policy: {POLICY} Does this support answer follow the policy? "
        "Question: {{question}} Answer: {{answer}}",
        response_format=ResponseFormat.verdict(),
    ),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
)

# The Verifier: two requirements every reply must meet.
reply_check = Verifier(
    "reply-check",
    inputs=Reply,
    requirements=[
        Requirement(
            "no-refund-promise",
            "The answer never promises a guaranteed refund.",
            Check.assert_("inputs.answer", ComparisonOperator.NotContains, "guaranteed refund"),
        ),
        Requirement(
            "follows-policy",
            "The answer follows the refund policy.",
            Check.agent(policy_judge).inputs(
                {"question": "inputs.question", "answer": "inputs.answer"}
            ),
        ),
    ],
)

# Declare the Cards and get a ready state.
state = WyrdState.declare(Service("support", verified_by=[reply_check]))
model = OpenAI()


# Your support agent: any model call.
def support_agent(question: str) -> str:
    completion = model.chat.completions.create(
        model="gpt-4o-mini",
        messages=[
            {"role": "system", "content": f"You are a support agent. {POLICY}"},
            {"role": "user", "content": question},
        ],
    )
    return completion.choices[0].message.content or ""


question = "I bought this 45 days ago. Can I get my money back?"
# The agent's reply, and a reply that breaks the policy.
for answer in (support_agent(question), "Yes, you have a guaranteed refund."):
    with state.run() as run:
        result = run.observe.verify("reply-check", Reply(question=question, answer=answer))
    print(f"{result.verdict}: {answer}")
    for requirement in result.requirements:
        if requirement["outcome"] != "passed":
            print(f"  {requirement['id']}: {requirement['reason']}")
state.shutdown()
