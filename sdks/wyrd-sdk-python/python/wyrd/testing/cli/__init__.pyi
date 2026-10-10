# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
"""The ``wyrd`` CLI in process: a test surface, built only with ``testing``.

Every function runs the same Rust command implementation as the ``wyrd``
executable, takes the command's options as keyword arguments, returns the
typed result the command prints with ``--format json``, and raises
``WyrdError`` instead of returning an exit code. A networked command takes an
optional ``client`` and runs as its principal; omitted, the server and
credential resolve from the ambient chain (``WYRD_SERVER_URL``, then
``WYRD_ACCESS_TOKEN``, workload identity, ``WYRD_API_KEY``, or
``credentials.toml``) exactly as the executable does.
"""

import datetime
from collections.abc import Mapping
from os import PathLike
from pathlib import Path
from typing import Any, Literal

from ...cards import CardRef, HydrationSummary, RegistrationReceipt
from ...client import WyrdClient
from ...gateway import ProviderCredentialView

class PlanCard:
    """One Card a plan would register, identified as authored."""

    @property
    def kind(self) -> str:
        """Card kind wire name, such as ``"Service"``."""
        ...
    @property
    def space(self) -> str | None:
        """Authored space, or ``None`` for the default space."""
        ...
    @property
    def name(self) -> str:
        """Card name."""
        ...
    @property
    def version(self) -> str | None:
        """Authored version, or ``None`` when the server assigns one."""
        ...

class PlanDiagnostic:
    """One non-fatal loader diagnostic in a plan."""

    @property
    def code(self) -> str:
        """Stable catalog code."""
        ...
    @property
    def severity(self) -> Literal["error", "warning"]:
        """How serious the diagnostic is."""
        ...
    @property
    def path(self) -> Path:
        """Authored file the diagnostic concerns."""
        ...
    @property
    def message(self) -> str:
        """Human-readable problem detail."""
        ...

class PlanReport:
    """Deterministic local registration plan returned by ``plan``."""

    @property
    def ok(self) -> bool:
        """Whether the tree loaded and resolved; always ``True`` when returned."""
        ...
    @property
    def cards(self) -> list[PlanCard]:
        """Cards the tree would register, in registration order."""
        ...
    @property
    def diagnostics(self) -> list[PlanDiagnostic]:
        """Non-fatal loader diagnostics."""
        ...

class LoadOutput:
    """Result of ``load``: the exact Card the selector resolved to."""

    @property
    def card_ref(self) -> CardRef:
        """Exact reference of the loaded Card."""
        ...
    @property
    def materialized(self) -> bool:
        """Whether the artifacts were materialized; always ``True`` when returned."""
        ...

class IssueKeyResponse:
    """A Card-scoped API key returned by ``issue_key``, exactly once."""

    @property
    def key_id(self) -> str:
        """Credential id of the issued key."""
        ...
    @property
    def principal_id(self) -> str:
        """Principal the key authenticates as."""
        ...
    @property
    def key(self) -> str:
        """Plaintext API key."""
        ...
    @property
    def prefix(self) -> str:
        """Log-safe key prefix."""
        ...
    @property
    def card_ref(self) -> CardRef:
        """The Card the key is bound to."""
        ...
    @property
    def created_at(self) -> datetime.datetime:
        """When the key was issued."""
        ...
    @property
    def expires_at(self) -> datetime.datetime:
        """When the key expires."""
        ...

def plan(path: str | PathLike[str]) -> PlanReport:
    """Validate a local Card tree without contacting a server (``wyrd plan``).

    Args:
        path: A Card file or a Card tree directory.

    Returns:
        The planned Cards and non-fatal loader diagnostics.

    Raises:
        WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` with the loader
            diagnostics in ``details`` when the tree cannot be loaded.
    """
    ...

def apply(path: str | PathLike[str], *, client: WyrdClient | None = None) -> RegistrationReceipt:
    """Register a local Card tree and return its receipt (``wyrd apply``).

    Args:
        path: A Card file or a Card tree directory.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The registration receipt.

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
    ...

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
    ...

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
    ...

def put_provider_credential(
    write: Mapping[str, Any], *, client: WyrdClient | None = None
) -> ProviderCredentialView:
    """Create or rotate a provider credential (``wyrd gateway credential put``).

    ``write`` may carry a provider key; a rejected body is reported without
    quoting it, and the returned view is redacted.

    Args:
        write: The provider credential write body.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The redacted credential view.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a body that does not match
            the credential contract, or the client or server error.
    """
    ...

def revoke_provider_credential(
    name: str, *, client: WyrdClient | None = None
) -> ProviderCredentialView:
    """Terminally revoke a provider credential (``wyrd gateway credential revoke``).

    Repeating the revocation is harmless.

    Args:
        name: The provider credential name.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The redacted, revoked credential view.

    Raises:
        WyrdError: For an invalid name, or the client or server error.
    """
    ...

def delete_provider_credential(name: str, *, client: WyrdClient | None = None) -> None:
    """Delete an unreferenced provider credential; an absent name succeeds.

    Args:
        name: The provider credential name.
        client: The principal to act as. Omitted, the ambient chain.

    Raises:
        WyrdError: For an invalid name, or the client or server error such as
            a conflict for a referenced credential.
    """
    ...

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
