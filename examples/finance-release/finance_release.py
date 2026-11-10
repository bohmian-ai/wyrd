"""The finance release, authored in Python.

A finance team's ``report-writer`` Agent prepares customer reports for
outside auditors. Before a report is sent, the ``release-check`` Verifier
judges three requirements, all at once:

- ``no-bank-account``: the report names no bank account (an assertion).
- ``recipient-authorized``: the recipient is authorized (an assertion).
- ``follows-policy``: ``policy-judge`` reads the company sharing policy and
  approves the release (an Agent check).

The program builds every Card in code, declares them in one call, and
verifies a permitted release and one that names a bank account. No Card names
a version except the policy, so the server selects each one. The synthetic
evidence demonstrates wiring, not real authorization decisions.
"""

from pydantic import BaseModel
from wyrd import WyrdState
from wyrd.verify import (
    Agent,
    AgentRunConfig,
    Artifact,
    Check,
    ComparisonOperator,
    Component,
    ExecutionRoute,
    Prompt,
    Requirement,
    ResponseFormat,
    Service,
    Verifier,
)


# The evidence the app sends; each model is its slot's JSON Schema.
class Report(BaseModel):
    text: str


class Recipient(BaseModel):
    email: str
    authorized: bool


# The company sharing policy, uploaded from this directory, with the PDF's
# text extracted so the judge can read it.
policy = Artifact.from_path(
    "company-sharing-policy", "1.0.0", "company-sharing-policy.pdf"
).extract(True)

# `policy-judge`: a model that reads the policy with `fs.read` and returns a
# verdict on the release.
policy_judge = Agent(
    "policy-judge",
    prompt=Prompt.openai_chat(
        "gpt-4o",
        messages=(
            "Read /inputs/policy.json with fs.read. Decide whether this release "
            "follows the company sharing policy, citing the report and the policy. "
            "Report: {{report}} Recipient: {{recipient}}"
        ),
        response_format=ResponseFormat.verdict(),
    ),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
    tool_names=["fs.read"],
    run_config=AgentRunConfig(max_iterations=3),
)

# An Agent check: `policy-judge` sees the report and the recipient, and reads
# the policy from `/inputs/policy.json`.
follows_policy = (
    Check.agent(policy_judge)
    .inputs({"report": "inputs.report", "recipient": "inputs.recipient"})
    .add_workspace("/inputs/policy.json", policy.file("extracted/company-sharing-policy.pdf.json"))
)

release_check = Verifier(
    "release-check",
    inputs={"report": Report, "recipient": Recipient},
    requirements=[
        # Assertions: compare a value, no model runs.
        Requirement(
            "no-bank-account",
            "The report names no bank account.",
            Check.assert_("inputs.report.text", ComparisonOperator.NotContains, "BANK-ACCOUNT"),
        ),
        Requirement(
            "recipient-authorized",
            "The recipient is authorized.",
            Check.assert_("inputs.recipient.authorized", ComparisonOperator.Equals, True),
        ),
        Requirement(
            "follows-policy",
            "The release follows the company sharing policy.",
            follows_policy,
        ),
    ],
)

# The Service: the Agent that writes the reports, verified by `release-check`.
writer = Agent(
    "report-writer",
    prompt=Prompt.openai_chat("gpt-4o", messages="Write the customer report for {{customer}}."),
    execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
)
service = Service(
    "finance-release",
    components=[Component("report-writer", writer)],
    verified_by=[release_check],
)

# Upload the policy, declare every Card, and get a ready state.
state = WyrdState.declare(service)

try:
    recipient = Recipient(email="auditor@example.test", authorized=True)
    for report in (
        Report(text="Quarterly total $120."),
        Report(text="Quarterly total $120. Paid from BANK-ACCOUNT-DEMO-001."),
    ):
        with state.run() as run:
            result = run.observe.verify("release-check", report=report, recipient=recipient)
        print(report.text)
        print(f"  {result.verifier.name}: {result.verdict}")
        for requirement in result.requirements:
            if requirement["outcome"] != "passed":
                print(f"    not passed: {requirement['statement']}")
finally:
    state.shutdown()
