"""Revision-4 sketch: string references and explicit dependency IDs."""

from release_steps import CHECKS
from wyrd.authoring import Action, Check, CodeSource, Requirement, Step, Verifier


def order_existing_steps(
    verifier: Verifier,
    recipient: Step,
    policy: Step,
    destination: Step,
) -> Verifier:
    verifier.add_step(recipient).add_step(policy).add_step(destination)
    summary = Step(
        "summarize-findings",
        Action.code(CodeSource.file(CHECKS), "summarize_findings").inputs(
            {
                "recipient": "steps.recipient.output",
                "policy": "steps.policy.output",
                "destination": "steps.destination.output",
            }
        ),
        depends_on=["recipient", "policy", "destination"],
    )
    verifier.add_step(summary)
    verifier.add_requirement(
        Requirement(
            "audit-purpose",
            "The release is requested for an external audit.",
            Check.assert_("inputs.purpose", "equals", "external-audit"),
        )
    )
    return verifier
