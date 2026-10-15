"""In-process ``wyrd`` commands whose Python signature differs from the native one.

Re-exported by ``wyrd.testing.cli``; each wrapper packs its keyword options
into the native selector tuple.
"""

from __future__ import annotations

from os import PathLike
from typing import TYPE_CHECKING

# The native ``_wyrd.testing.cli`` submodule is private and has no stub.
from wyrd._wyrd.testing import cli as _native  # ty: ignore[unresolved-import]

if TYPE_CHECKING:
    from wyrd.cards import HydrationSummary
    from wyrd.client import WyrdClient
    from wyrd.testing.cli import IssueKeyResponse, LoadOutput


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
    """Hydrate a Card's reachable graph into ``output_dir`` (``wyrd get``).

    Select by ``uid`` (with ``kind``) or by ``kind``, ``space``, and ``name``;
    ``version`` narrows either. ``metadata_only`` writes an inspectable,
    non-runnable bundle without artifact payloads.

    Args:
        output_dir: The bundle directory to publish.
        kind: The Card kind.
        space: The Card space for a named selector, or a UID assertion.
        name: The Card name for a named selector, or a UID assertion.
        version: The exact Card version.
        uid: The exact Card UID.
        metadata_only: ``True`` writes Card metadata without artifact payloads.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        What was published.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid selector, or the
            client or server error.
    """
    return _native.get(output_dir, (kind, space, name, version, uid), metadata_only, client)


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
    """Load one Card and materialize its artifacts (``wyrd load``).

    Args:
        kind: The Card kind.
        space: The Card space for a named selector, or a UID assertion.
        name: The Card name for a named selector, or a UID assertion.
        version: The exact Card version.
        uid: The exact Card UID.
        path: The directory that receives the artifacts. Omitted, a managed
            temporary directory.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The exact Card the selector resolved to.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid selector, or the
            client or server error.
    """
    return _native.load((kind, space, name, version, uid), path, client)


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
    """Issue an API key bound to one exact Card (``wyrd auth issue-key``).

    Args:
        kind: The Card kind.
        name: The Card name.
        version: The exact Card version.
        space: The Card space.
        label: An optional label stored with the key.
        expires_in_seconds: An optional lifetime override, in seconds.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The issued key; its plaintext is returned exactly once.

    Raises:
        WyrdError: For invalid coordinates, a missing credential, or the
            server's refusal.
    """
    return _native.issue_key((kind, name, version, space), label, expires_in_seconds, client)
