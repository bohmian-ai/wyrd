"""The finance release, authored in Python.

A finance team's ``report-writer`` Agent prepares customer reports for
outside auditors. Before a report is sent, the ``release-check`` Verifier
judges three requirements, all at once:

- ``no-bank-account``: the report names no bank account (an assertion).
- ``recipient-authorized``: the recipient is authorized (an assertion).
- ``follows-policy``: ``policy-judge`` reads the company sharing policy and
  approves the release (an Agent check).

The program builds every Card in code, declares them in one call, and
verifies a permitted release and one that names a bank account.
"""

from wyrd import WyrdState
from wyrd.authoring import Agent, Artifact, Check, Component, Requirement, Service, Verifier

# The company sharing policy, uploaded from this directory, with the PDF's
# text extracted so the judge can read it.
policy = Artifact.from_path(
    "company-sharing-policy", "1.0.0", "company-sharing-policy.pdf"
).extract(True)

# `policy-judge`: a model that reads the policy with `fs.read` and returns a
# verdict on the release.
policy_judge = Agent(
    "policy-judge",
    "1.0.0",
    prompt={
        "provider": "openai",
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": (
                    "Read /inputs/policy.json with fs.read. Decide whether this release "
                    "follows the company sharing policy, citing the report and the policy. "
                    "Report: {{report}} Recipient: {{recipient}}"
                ),
            }
        ],
        "response_format": "verdict",
    },
    execution_route={"kind": "wyrd_gateway", "deployment": "openai"},
    tool_names=["fs.read"],
    run_config={"max_iterations": 3},
)

# An Agent check: `policy-judge` sees the report and the recipient, and reads
# the policy from `/inputs/policy.json`.
follows_policy = Check.agent(policy_judge).inputs(
    {"report": "inputs.report", "recipient": "inputs.recipient"}
)
follows_policy.add_workspace(
    "/inputs/policy.json", policy.file("extracted/company-sharing-policy.pdf.json")
)

release_check = Verifier(
    "release-check",
    "1.0.0",
    # The evidence the app sends, and the JSON Schema each must match.
    inputs={
        "report": {
            "type": "object",
            "required": ["text"],
            "properties": {"text": {"type": "string"}},
        },
        "recipient": {
            "type": "object",
            "required": ["email", "authorized"],
            "properties": {"email": {"type": "string"}, "authorized": {"type": "boolean"}},
        },
    },
    requirements=[
        # Assertions: compare a value, no model runs.
        Requirement(
            "no-bank-account",
            "The report names no bank account.",
            Check.assert_("inputs.report.text", "not_contains", "BANK-ACCOUNT"),
        ),
        Requirement(
            "recipient-authorized",
            "The recipient is authorized.",
            Check.assert_("inputs.recipient.authorized", "equals", True),
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
    "1.0.0",
    prompt={
        "provider": "openai",
        "model": "gpt-4o",
        "messages": [{"role": "user", "content": "Write the customer report for {{customer}}."}],
    },
    execution_route={"kind": "wyrd_gateway", "deployment": "openai"},
)
service = Service(
    "finance-release",
    "1.0.0",
    components=[Component("report-writer", writer)],
    verified_by=[release_check],
)

# Upload the policy, declare every Card, and get a ready state.
state = WyrdState.declare(service)

for report in (
    "Quarterly total $120.",
    "Quarterly total $120. Paid from BANK-ACCOUNT-DEMO-001.",
):
    result = state.run().observe.verify(
        "release-check",
        report={"text": report},
        recipient={"email": "auditor@example.test", "authorized": True},
    )
    print(report)
    print(f"  {result.verifier.name}: {result.verdict}")
    for requirement in result.requirements:
        if requirement["outcome"] != "passed":
            print(f"    not passed: {requirement['statement']}")
