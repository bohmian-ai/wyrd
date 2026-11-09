"""Revision-4 sketch: incremental edits with the same canonical fields."""

from evidence import Destination, Recipient, Report
from policy_judge import make_policy_judge
from release_steps import POLICY, make_steps
from wyrd.authoring import Artifact, ArtifactFile, Component, Service, Verifier


def make_service(policy_file: ArtifactFile, version: str = "1.0.0") -> Service:
    judge = make_policy_judge()
    verifier = Verifier("sensitive-report-release", version).decide("all")
    verifier.add_input("report", Report).add_input("recipient", Recipient)
    verifier.add_input("purpose", str).add_input("destination", Destination)
    for step in make_steps(policy_file, judge):
        verifier.add_step(step)

    service = Service("finance-assistant", version)
    service.add_component(Component("policy-judge", judge))
    service.add_verified_by(verifier)
    return service


def local_policy_service() -> Service:
    policy = Artifact.from_path("company-sharing-policy", "1.0.0", POLICY).extract(True)
    return make_service(policy.file("extracted/company-sharing-policy.pdf.json"))
