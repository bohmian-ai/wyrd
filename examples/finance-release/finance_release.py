"""Verify each proposed report release before the application sends it.

The ``finance-release`` Service shares synthetic customer reports with outside
auditors. Before it releases one, it calls the ``release-check`` Verifier once,
and the Verifier judges three requirements together:

- ``recipient-authorized``: the supplied recipient record says the recipient
  is authorized (an assertion).
- ``follows-policy``: ``policy-judge`` reads the company sharing policy PDF and
  judges the report against it (an Agent check, through the Wyrd gateway).
- ``destination-restricted``: the supplied destination record says access is
  restricted (an assertion).

Wyrd returns the verdict; the application decides. Only ``passed`` releases
the report; anything else holds it for review. Nothing is sent anywhere.
"""

import json

from pydantic import BaseModel
from wyrd import WyrdError, WyrdState
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


# The company sharing policy, uploaded once from this directory with its text
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
            "Read /inputs/policy.json with fs.read. Decide whether the company sharing "
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
    .inputs({"report": "inputs.report", "recipient": "inputs.recipient"})
    .add_workspace("/inputs/policy.json", policy.file("extracted/company-sharing-policy.pdf.json"))
)

release_check = Verifier(
    "release-check",
    # Each input must match its model.
    inputs={"report": Report, "recipient": Recipient, "destination": Destination},
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

service = Service("finance-release", verified_by=[release_check])

# Upload the policy, declare every Card, and get a ready state.
state = WyrdState.declare(service)

try:
    with open("releases.json") as file:
        releases = json.load(file)
    # ANCHOR: release
    for release in releases:
        name = release.pop("name")
        try:
            with state.run() as run:
                # One synchronous call judges all three requirements.
                result = run.observe.verify("release-check", **release)
        except WyrdError as error:
            print(f"{name}: {error.code} -> held for review")
            continue
        verifier = result.verifier
        # Only a passed verdict releases the report.
        decision = "released" if result.verdict == "passed" else "held for review"
        print(f"{name}: {result.verdict} ({verifier.name}@{verifier.version}) -> {decision}")
        for requirement in result.requirements:
            if requirement["outcome"] != "passed":
                print(f"  {requirement['id']} {requirement['outcome']}: {requirement['reason']}")
    # ANCHOR_END: release
finally:
    state.shutdown()
