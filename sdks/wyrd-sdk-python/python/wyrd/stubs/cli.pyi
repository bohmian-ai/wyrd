"""The ``wyrd`` CLI, in process and as the installed executable.

Every function runs the same Rust command implementation as the ``wyrd``
executable, takes the command's options as keyword arguments, returns the
value the command prints with ``--format json``, and raises ``WyrdError``
instead of returning an exit code. Networked commands read their credential
from the ambient chain (``WYRD_ACCESS_TOKEN``, workload identity,
``WYRD_API_KEY``, or ``credentials.toml``) and never take one as an argument;
``server`` re-points only the endpoint.
"""

from collections.abc import Mapping
from os import PathLike
from typing import Any

from .gateway import ProviderCredentialView

def run_wyrd_cli() -> int:
    """Run the ``wyrd`` executable over ``sys.argv`` and return its exit code.

    This is the installed ``wyrd`` console script.
    """
    ...

def plan(path: str | PathLike[str]) -> dict[str, Any]:
    """Validate a local Card tree without contacting a server (``wyrd plan``).

    Returns:
        dict[str, Any]: ``{"ok": True, "cards": [...], "diagnostics": [...]}``.

    Raises:
        WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` with the loader
            diagnostics in ``details`` when the tree cannot be loaded.
    """
    ...

def apply(path: str | PathLike[str], *, server: str | None = None) -> dict[str, Any]:
    """Register a local Card tree and return its receipt (``wyrd apply``).

    Raises:
        WyrdError: For invalid local input, a missing credential, or the
            server's refusal.
    """
    ...

def get(
    *,
    output_dir: str | PathLike[str],
    kind: str | None = None,
    space: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    metadata_only: bool = False,
    server: str | None = None,
) -> dict[str, Any]:
    """Hydrate a Card's reachable graph into ``output_dir`` (``wyrd get``).

    Select by ``uid`` (with ``kind``) or by ``kind``, ``space``, and ``name``;
    ``version`` narrows either. ``metadata_only`` writes an inspectable,
    non-runnable bundle without artifact payloads.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid selector, or the
            client or server error.
    """
    ...

def load(
    *,
    kind: str | None = None,
    space: str | None = None,
    name: str | None = None,
    version: str | None = None,
    uid: str | None = None,
    path: str | PathLike[str] | None = None,
    server: str | None = None,
) -> dict[str, Any]:
    """Load one Card and materialize its artifacts (``wyrd load``).

    Returns:
        dict[str, Any]: ``{"card_ref": {...}, "materialized": True}``.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid selector, or the
            client or server error.
    """
    ...

def issue_key(
    *,
    kind: str,
    name: str,
    version: str,
    space: str,
    label: str | None = None,
    expires_in_seconds: int | None = None,
    server: str | None = None,
) -> dict[str, Any]:
    """Issue an API key bound to one exact Card (``wyrd auth issue-key``).

    Returns:
        dict[str, Any]: The issue-key response; ``key`` holds the plaintext
            API key exactly once.

    Raises:
        WyrdError: For invalid coordinates, a missing credential, or the
            server's refusal.
    """
    ...

def put_provider_credential(
    write: Mapping[str, Any], *, server: str | None = None
) -> ProviderCredentialView:
    """Create or rotate a provider credential (``wyrd gateway credential put``).

    ``write`` may carry a provider key; a rejected body is reported without
    quoting it, and the returned view is redacted.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a body that does not match
            the credential contract, or the client or server error.
    """
    ...

def revoke_provider_credential(name: str, *, server: str | None = None) -> ProviderCredentialView:
    """Terminally revoke a provider credential (``wyrd gateway credential revoke``)."""
    ...

def delete_provider_credential(name: str, *, server: str | None = None) -> None:
    """Delete an unreferenced provider credential; an absent name succeeds."""
    ...

__all__ = [
    "apply",
    "delete_provider_credential",
    "get",
    "issue_key",
    "load",
    "plan",
    "put_provider_credential",
    "revoke_provider_credential",
    "run_wyrd_cli",
]
