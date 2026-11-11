"""Revision-4 sketch: constructor fields for a Service-level binding."""

from evidence import Destination, Recipient, Report
from policy_judge import make_policy_judge
from release_steps import POLICY, make_steps
from wyrd.authoring import Artifact, ArtifactFile, Binding, Component, Service, Verifier


def make_service(policy_file: ArtifactFile, version: str = "1.0.0") -> Service:
    judge = make_policy_judge()
    verifier = Verifier(
        "sensitive-report-release",
        version,
        inputs={
            "report": Report,
            "recipient": Recipient,
            "purpose": str,
            "destination": Destination,
        },
        steps=make_steps(policy_file, judge),
        decide="all",
    )
    return Service(
        "finance-assistant",
        version,
        components=[Component("policy-judge", judge)],
        verified_by=[Binding(verifier)],
    )


def local_policy_service() -> Service:
    policy = Artifact.from_path("company-sharing-policy", "1.0.0", POLICY).extract(True)
    return make_service(policy.file("extracted/company-sharing-policy.pdf.json"))
