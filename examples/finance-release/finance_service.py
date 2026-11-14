from pathlib import Path

from pydantic import BaseModel
from wyrd.verify import (
    Agent,
    AgentRunConfig,
    Artifact,
    Check,
    ComparisonOperator,
    ExecutionRoute,
    Prompt,
    Requirement,
    ResponseFormat,
    Service,
    Verifier,
)


# The evidence the application sends for each release.
class Report(BaseModel):
    text: str


class Recipient(BaseModel):
    email: str
    authorized: bool


class Destination(BaseModel):
    url: str
    restricted: bool


class Release(BaseModel):
    report: Report
    recipient: Recipient
    destination: Destination


# The company sharing policy. The program saves it beside itself the first
# time; edit the file and run again to publish a new policy version.
POLICY = """# Company sharing policy

Reports may be shared outside the company only for external-audit purposes.
A shared report must never contain a bank account identifier such as BANK-ACCOUNT-DEMO-001.
"""
policy_file = Path("company-sharing-policy.md")
if not policy_file.exists():
    policy_file.write_text(POLICY)
policy = Artifact.from_path("company-sharing-policy", policy_file)

# ANCHOR: run-config
# `policy-judge`: a model that reads the policy with `fs.read` and returns a
# verdict on the report. Its model calls go through the `openai` gateway
# deployment.
policy_judge = Agent(
    "policy-judge",
    prompt=Prompt.openai_chat(
        "gpt-4o",
        messages=(
            "Read /inputs/policy.md with fs.read. Decide whether the company sharing "
            "policy permits sharing this report with this recipient, citing the report "
            "and the policy line you apply. Report: {{report}} Recipient: {{recipient}}"
        ),
        response_format=ResponseFormat.verdict(),
    ),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
    tool_names=["fs.read"],
    run_config=AgentRunConfig(max_iterations=3),
)
# ANCHOR_END: run-config

follows_policy = (
    Check.agent(policy_judge)
    .inputs({"report": "inputs.release.report", "recipient": "inputs.release.recipient"})
    .add_workspace("/inputs/policy.md", policy.file("company-sharing-policy.md"))
)

release_check = Verifier(
    "release-check",
    # Each release must match its model.
    inputs={"release": Release},
    # ANCHOR: requirements
    requirements=[
        Requirement(
            "recipient-authorized",
            "The recipient is authorized to receive the report.",
            Check.assert_("inputs.release.recipient.authorized", ComparisonOperator.Equals, True),
        ),
        Requirement(
            "follows-policy",
            "The company sharing policy permits this release.",
            follows_policy,
        ),
        Requirement(
            "destination-restricted",
            "The destination restricts access to the report.",
            Check.assert_("inputs.release.destination.restricted", ComparisonOperator.Equals, True),
        ),
    ],
    # ANCHOR_END: requirements
)

service = Service("finance-release", verified_by=[release_check])

