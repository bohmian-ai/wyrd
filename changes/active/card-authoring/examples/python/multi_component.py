"""Revision-4 sketch: two Components, each with its own two-step binding."""

from evidence import Destination, Recipient, Report, permitted_request
from policy_judge import make_policy_judge
from release_steps import POLICY, checked_step, make_steps
from wyrd import WyrdState
from wyrd.authoring import (
    Artifact,
    ArtifactFile,
    CardRef,
    Component,
    Service,
    Verifier,
)
from wyrd.cards import Cards


def make_service(policy_file: ArtifactFile) -> Service:
    reviewer = make_policy_judge("report-reviewer")
    controls = make_policy_judge("release-controls")
    recipient, policy, destination = make_steps(policy_file, reviewer)
    context = checked_step(
        "policy-context",
        "policy_context",
        {"report": "inputs.report", "purpose": "inputs.purpose"},
        "usable-report",
        "The proposed release contains report text.",
        "has_report",
        True,
    )
    report_verifier = Verifier(
        "report-policy",
        "1.0.0",
        inputs={
            "report": Report,
            "recipient": Recipient,
            "purpose": str,
            "destination": Destination,
        },
        steps=[policy, context],
    )
    access_verifier = Verifier("release-access", "1.0.0")
    access_verifier.add_input("recipient", Recipient).add_input("purpose", str)
    access_verifier.add_input("destination", Destination)
    access_verifier.add_input("notes", {"type": "string"})
    access_verifier.add_step(recipient).add_step(destination)

    service = Service("finance-release", "1.0.0")
    service.add_component(
        Component("report-reviewer", reviewer).verified_by([report_verifier])
    )
    service.add_component(
        Component("release-controls", controls).verified_by([access_verifier])
    )
    return service


def main() -> None:
    policy = Artifact.from_path("company-sharing-policy", "1.0.0", POLICY).extract(True)
    service = make_service(policy.file("extracted/company-sharing-policy.pdf.json"))
    service.validate()
    receipt = Cards().declare(service)
    print(receipt)

    registered = CardRef.artifact("company-sharing-policy", "1.0.0")
    reused = make_service(registered.file("extracted/company-sharing-policy.pdf.json"))
    state = WyrdState.declare(reused)
    request = permitted_request()
    run = state.run()
    report = run.for_card("report-reviewer").observe.verify(
        "report-policy",
        report=request.report,
        recipient=request.recipient,
        purpose=request.purpose,
        destination=request.destination,
    )
    access = run.for_card("release-controls").observe.verify(
        "release-access",
        recipient=request.recipient,
        purpose=request.purpose,
        destination=request.destination,
    )
    assert report.subject == state.card_ref("report-reviewer")
    assert access.subject == state.card_ref("release-controls")
    print("release" if report.verdict == access.verdict == "passed" else "hold")


if __name__ == "__main__":
    main()
