"""Revision-4 sketch: explicit immutable versions and registered-policy reuse."""

from pathlib import Path

from constructor_style import make_service
from wyrd import WyrdState
from wyrd.authoring import Artifact, CardRef


def declare_policy_revision(revised_source: Path) -> tuple[WyrdState, WyrdState]:
    existing = CardRef.artifact("company-sharing-policy", "1.0.0")
    original = WyrdState.declare(
        make_service(
            existing.file("extracted/company-sharing-policy.pdf.json"), version="1.0.0"
        )
    )
    revised_policy = Artifact.from_path(
        "company-sharing-policy", "2.0.0", revised_source
    ).extract(True)
    revised = WyrdState.declare(
        make_service(
            revised_policy.file("extracted/company-sharing-policy.pdf.json"),
            version="2.0.0",
        )
    )
    return original, revised


def policy_directory(directory: Path) -> Artifact:
    return Artifact.from_path("company-sharing-policy", "1.0.0", directory).extract(
        True
    )
