"""Revision-4 sketch: canonical actions/checks and string references."""

from pathlib import Path

from wyrd.authoring import (
    Action,
    Agent,
    ArtifactFile,
    Check,
    CodeSource,
    Requirement,
    Step,
)

CHECKS = Path(__file__).with_name("release_checks.py")
REPOSITORY = Path(__file__).resolve().parents[5]
POLICY = REPOSITORY / "fixtures/cards/card_authoring/company-sharing-policy.pdf"


def checked_step(
    id: str,
    entrypoint: str,
    inputs: dict[str, str],
    requirement_id: str,
    statement: str,
    field: str,
    expected: bool,
) -> Step:
    return Step(
        id,
        Action.code(CodeSource.file(CHECKS), entrypoint).inputs(inputs),
        requirements=[
            Requirement(
                requirement_id,
                statement,
                Check.assert_(f"steps.{id}.output.{field}", "equals", expected),
            ),
        ],
    )


def make_steps(policy_file: ArtifactFile, judge: Agent) -> list[Step]:
    recipient = checked_step(
        "recipient",
        "recipient_facts",
        {"recipient": "inputs.recipient", "purpose": "inputs.purpose"},
        "recipient-authorized",
        "The recipient is authorized for this purpose.",
        "authorized",
        True,
    )
    policy = checked_step(
        "policy",
        "report_facts",
        {"report": "inputs.report"},
        "no-bank-account-identifier",
        "The report contains no prohibited bank identifier.",
        "contains_bank_identifier",
        False,
    )
    policy.add_requirement(
        Requirement(
            "sharing-policy-satisfied",
            "The release satisfies the company sharing policy.",
            Check.agent(judge)
            .inputs(
                {
                    name: f"inputs.{name}"
                    for name in ("report", "recipient", "purpose", "destination")
                }
            )
            .workspace({"/inputs/policy.json": policy_file}),
        )
    )
    destination = checked_step(
        "destination",
        "destination_facts",
        {"destination": "inputs.destination"},
        "destination-controls-satisfied",
        "The destination has appropriate access controls.",
        "appropriate",
        True,
    )
    return [recipient, policy, destination]
