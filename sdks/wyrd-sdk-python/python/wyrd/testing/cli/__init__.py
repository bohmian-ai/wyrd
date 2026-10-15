"""The ``wyrd`` CLI in process: a test surface, built only with ``testing``.

Every function runs the same Rust command implementation as the ``wyrd``
executable, takes the command's options as keyword arguments, returns the
typed result the command prints with ``--format json``, and raises
``WyrdError`` instead of returning an exit code. A networked command takes an
optional ``client`` and runs as its principal; omitted, the server and
credential resolve from the ambient chain exactly as the executable does.
"""

from wyrd._wyrd.testing.cli import (
    IssueKeyResponse,
    LoadOutput,
    McpHostReport,
    McpInstallReport,
    PlanCard,
    PlanDiagnostic,
    PlanReport,
    apply,
    delete_provider_credential,
    mcp_install,
    plan,
    put_provider_credential,
    revoke_provider_credential,
)

from ._commands import get, issue_key, load

__all__ = [
    "IssueKeyResponse",
    "LoadOutput",
    "McpHostReport",
    "McpInstallReport",
    "PlanCard",
    "PlanDiagnostic",
    "PlanReport",
    "apply",
    "delete_provider_credential",
    "get",
    "issue_key",
    "load",
    "mcp_install",
    "plan",
    "put_provider_credential",
    "revoke_provider_credential",
]
