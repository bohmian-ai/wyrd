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
    purpose: str
    report: Report
    recipient: Recipient
    destination: Destination


# The company sharing policy, uploaded from this directory with its text
# extracted so the judge can read it.
policy = Artifact.from_path("company-sharing-policy", "company-sharing-policy.pdf").extract(True)

# ANCHOR: run-config
# `policy-judge`: a model that reads the policy with `fs.read` and returns a
# verdict on the report. Its model calls go through the `openai` gateway
# deployment.
policy_judge = Agent(
    "policy-judge",
    prompt=Prompt.openai_chat(
        "gpt-4o",
        messages=(
            "Read /inputs/company-sharing-policy.pdf.json with fs.read. Decide whether the company sharing "
            "policy permits sharing this report with this recipient for this purpose, "
            "citing the report and the policy line you apply. Purpose: {{purpose}} "
            "Report: {{report}} Recipient: {{recipient}}"
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
    .inputs({"purpose": "inputs.purpose", "report": "inputs.report", "recipient": "inputs.recipient"})
    .add_workspace("/inputs/company-sharing-policy.pdf.json", policy.file("extracted/company-sharing-policy.pdf.json"))
)

# ANCHOR: verifier
release_check = Verifier(
    "release-check",
    inputs=Release,
    # ANCHOR: requirements
    requirements=[
        Requirement(
            "recipient-authorized",
            "The recipient is authorized to receive the report.",
            Check.assert_("inputs.recipient.authorized", ComparisonOperator.Equals, True),
        ),
        Requirement(
            "follows-policy",
            "The company sharing policy permits this release.",
            follows_policy,
        ),
        Requirement(
            "destination-restricted",
            "The destination restricts access to the report.",
            Check.assert_("inputs.destination.restricted", ComparisonOperator.Equals, True),
        ),
    ],
    # ANCHOR_END: requirements
)
# ANCHOR_END: verifier

service = Service("finance-release", verified_by=[release_check])
