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

from collections.abc import Mapping
from os import PathLike
from pathlib import Path
from typing import Any, Literal

from ...cards import DeclarationReceipt
from ...client import WyrdClient
from ...gateway_admin import ProviderCredentialView
from ._commands import get, pull

class CheckCard:
    """One Card a check would declare, identified as authored."""

    @property
    def kind(self) -> str:
        """Card kind wire name, such as ``"Service"``."""
        ...
    @property
    def name(self) -> str:
        """Card name."""
        ...
    @property
    def version(self) -> str | None:
        """Authored version, or ``None`` when the server assigns one."""
        ...

class CheckDiagnostic:
    """One non-fatal loader diagnostic in a check."""

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

class CheckReport:
    """Deterministic local check report returned by ``declare_check``."""

    @property
    def ok(self) -> bool:
        """Whether the tree loaded and resolved; always ``True`` when returned."""
        ...
    @property
    def cards(self) -> list[CheckCard]:
        """Cards the tree would declare, in declaration order."""
        ...
    @property
    def diagnostics(self) -> list[CheckDiagnostic]:
        """Non-fatal loader diagnostics."""
        ...

class McpHostReport:
    """What ``mcp_install`` did to one host's configuration file."""

    @property
    def host(
        self,
    ) -> Literal["codex", "claude-code", "copilot-cli", "vscode", "cursor", "pi", "hermes"]:
        """The ``--host`` value naming the host."""
        ...
    @property
    def status(
        self,
    ) -> Literal[
        "added", "updated", "unchanged", "not_detected", "conflict", "unreadable", "unwritable"
    ]:
        """``added``, ``updated``, or ``unchanged`` when the host is connected;
        otherwise why its file was left as it was."""
        ...
    @property
    def path(self) -> Path | None:
        """The host's configuration file, or ``None`` when it was not detected."""
        ...
    @property
    def detail(self) -> str | None:
        """Why the host was not connected and what to do next, or ``None``."""
        ...

class McpInstallReport:
    """Per-host outcomes returned by ``mcp_install``, in selection order."""

    @property
    def hosts(self) -> list[McpHostReport]:
        """One outcome per distinct selected host."""
        ...

def declare_check(path: str | PathLike[str]) -> CheckReport:
    """Validate a local Card tree without contacting a server (``wyrd declare --check``).

    Args:
        path: A Card file or a Card tree directory.

    Returns:
        The checked Cards and non-fatal loader diagnostics.

    Raises:
        WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` with the loader
            diagnostics in ``details`` when the tree cannot be loaded.
    """
    ...

def declare(path: str | PathLike[str], *, client: WyrdClient | None = None) -> DeclarationReceipt:
    """Declare a local Card tree and return its receipt (``wyrd declare``).

    Args:
        path: A Card file or a Card tree directory.
        client: The principal to act as. Omitted, the ambient chain.

    Returns:
        The declaration receipt.

    Raises:
        WyrdError: For invalid local input, a missing credential, or the
            server's refusal.
    """
    ...

def mcp_install(hosts: list[str], *, server: str | None = None) -> McpInstallReport:
    """Connect the named MCP hosts to Wyrd (``wyrd mcp install --host ...``).

    Hosts are located from ``HOME``, ``CODEX_HOME``, ``COPILOT_HOME``,
    ``XDG_CONFIG_HOME``, ``PI_CODING_AGENT_DIR``, and ``HERMES_HOME``. A host that cannot be connected is reported in the
    result with its file left as it was, never raised. No credential is
    written.

    Args:
        hosts: ``codex``, ``claude-code``, ``copilot-cli``, ``vscode``,
            ``cursor``, ``pi``, or ``hermes``; repeats are installed once.
        server: The Wyrd server retained in each host's launch command.
            Omitted, the host follows the global client endpoint.

    Returns:
        One outcome per distinct selected host.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown or empty host
            selection, or ``WYRD_CLIENT_400_CONFIG_INVALID`` for an invalid
            ``server``.
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
    "CheckCard",
    "CheckDiagnostic",
    "CheckReport",
    "McpHostReport",
    "McpInstallReport",
    "declare",
    "declare_check",
    "delete_provider_credential",
    "get",
    "mcp_install",
    "pull",
    "put_provider_credential",
    "revoke_provider_credential",
]
