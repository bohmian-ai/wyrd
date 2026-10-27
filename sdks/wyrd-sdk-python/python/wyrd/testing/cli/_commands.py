"""In-process ``wyrd`` commands whose Python signature differs from the native one.

Re-exported by ``wyrd.testing.cli``; each wrapper packs its keyword options
into the native selector tuple.
"""

from __future__ import annotations

from os import PathLike
from typing import TYPE_CHECKING, Any, Literal

# The native ``_wyrd.testing.cli`` submodule is private and has no stub.
from wyrd._wyrd.testing import cli as _native  # ty: ignore[unresolved-import]

if TYPE_CHECKING:
    from wyrd.cards import CardRef, PullSummary
    from wyrd.client import WyrdClient
    from wyrd.state import CardEnvelope


def get(
    *,
    kind: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    client: WyrdClient | None = None,
) -> CardEnvelope:
    """Read one Card envelope without downloading artifacts (``wyrd get``).

    Select by ``uid`` (with ``kind``) or by ``kind`` and ``name``;
    ``version`` narrows either.

    Args:
        kind: The Card kind.
        name: The Card name for a named selector, or a UID assertion.
        version: The exact Card version.
        uid: The exact Card UID.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The complete Card envelope.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid selector, or the
            client or server error.
    """
    return _native.get((kind, name, version, uid), client)


def pull(
    *,
    output_dir: str | PathLike[str],
    kind: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    metadata_only: bool = False,
    client: WyrdClient | None = None,
) -> PullSummary:
    """Pull a Card's reachable graph into ``output_dir`` (``wyrd pull``).

    Select by ``uid`` (with ``kind``) or by ``kind`` and ``name``;
    ``version`` narrows either. ``metadata_only`` writes an inspectable,
    non-runnable bundle without artifact payloads.

    Args:
        output_dir: The bundle directory to publish.
        kind: The Card kind.
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
    return _native.pull(output_dir, (kind, name, version, uid), metadata_only, client)


def artifact_upload(
    source: str | PathLike[str],
    *,
    name: str,
    version: str | None = None,
    bump: Literal["major", "minor", "patch"] | None = None,
    artifact_kind: str = "file",
    extract: bool = False,
    client: WyrdClient | None = None,
) -> CardRef:
    """Publish one file or directory as an Artifact Card (``wyrd artifact upload``).

    ``extract`` also publishes ``extracted/P.json`` for each supported
    document ``P``.

    Args:
        source: The regular file or directory to publish.
        name: The Artifact Card name.
        version: An exact version or version scope. Omitted, the server assigns one.
        bump: The version bump the server applies to the latest version.
        artifact_kind: The Artifact kind, an open vocabulary.
        extract: Extract supported documents locally and publish the outputs too.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The exact reference of the declared Artifact Card.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid name, version,
            bump, source, or extraction, or the client or server error.
    """
    return _native.artifact_upload(source, (name, version, bump), artifact_kind, extract, client)


def artifact_files(
    *,
    kind: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    client: WyrdClient | None = None,
) -> dict[str, Any]:
    """List the files one Card holds (``wyrd artifact files``).

    Select by ``uid`` (with ``kind``) or by ``kind`` and ``name``;
    ``version`` narrows either. ``kind`` defaults to ``Artifact``.

    Args:
        kind: The Card kind.
        name: The Card name for a named selector, or a UID assertion.
        version: The exact Card version.
        uid: The exact Card UID.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The inventory: ``artifacts``, one mapping per file with
        ``relative_path``, ``size_bytes``, base64 ``sha256``, and
        ``content_type``, in path order.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid selector, or the
            client or server error.
    """
    return _native.artifact_files((kind, name, version, uid), client)
