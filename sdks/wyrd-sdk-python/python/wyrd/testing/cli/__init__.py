"""The ``wyrd`` CLI in process: a test surface, built only with ``testing``.

Every function runs the same Rust command implementation as the ``wyrd``
executable, takes the command's options as keyword arguments, returns the
typed result the command prints with ``--format json``, and raises
``WyrdError`` instead of returning an exit code. A networked command takes an
optional ``client`` and runs as its principal; omitted, the server and
credential resolve from the ambient chain exactly as the executable does.
"""

from __future__ import annotations

from os import PathLike

from wyrd._wyrd.testing.cli import (
    IssueKeyResponse,
    LoadOutput,
    PlanCard,
    PlanDiagnostic,
    PlanReport,
    apply,
    delete_provider_credential,
    plan,
    put_provider_credential,
    revoke_provider_credential,
)
from wyrd._wyrd.testing.cli import get as _get
from wyrd._wyrd.testing.cli import issue_key as _issue_key
from wyrd._wyrd.testing.cli import load as _load
from wyrd.cards import HydrationSummary
from wyrd.client import WyrdClient


def get(
    *,
    output_dir: str | PathLike[str],
    kind: str | None = None,
    space: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    metadata_only: bool = False,
    client: WyrdClient | None = None,
) -> HydrationSummary:
    """Hydrate a Card's reachable graph into ``output_dir`` (``wyrd get``)."""
    return _get(output_dir, (kind, space, name, version, uid), metadata_only, client)


def load(
    *,
    kind: str | None = None,
    space: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    path: str | PathLike[str] | None = None,
    client: WyrdClient | None = None,
) -> LoadOutput:
    """Load one Card and materialize its artifacts (``wyrd load``)."""
    return _load((kind, space, name, version, uid), path, client)


def issue_key(
    *,
    kind: str,
    name: str,
    version: str,
    space: str,
    label: str | None = None,
    expires_in_seconds: int | None = None,
    client: WyrdClient | None = None,
) -> IssueKeyResponse:
    """Issue an API key bound to one exact Card (``wyrd auth issue-key``)."""
    return _issue_key((kind, name, version, space), label, expires_in_seconds, client)


__all__ = [
    "IssueKeyResponse",
    "LoadOutput",
    "PlanCard",
    "PlanDiagnostic",
    "PlanReport",
    "apply",
    "delete_provider_credential",
    "get",
    "issue_key",
    "load",
    "plan",
    "put_provider_credential",
    "revoke_provider_credential",
]
