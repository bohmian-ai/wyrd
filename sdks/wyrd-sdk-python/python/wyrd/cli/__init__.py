"""The ``wyrd`` CLI, in process and as the installed executable.

Every function runs the same Rust command implementation as the ``wyrd``
executable, takes the command's options as keyword arguments, returns the
value the command prints with ``--format json``, and raises ``WyrdError``
instead of returning an exit code. Networked commands read their credential
from the ambient chain (``WYRD_ACCESS_TOKEN``, workload identity,
``WYRD_API_KEY``, or ``credentials.toml``); ``server`` re-points only the
endpoint.
"""

from __future__ import annotations

from os import PathLike
from typing import Any

from wyrd._wyrd.cli import (
    apply,
    delete_provider_credential,
    plan,
    put_provider_credential,
    revoke_provider_credential,
    run_wyrd_cli,
)
from wyrd._wyrd.cli import get as _get
from wyrd._wyrd.cli import issue_key as _issue_key
from wyrd._wyrd.cli import load as _load


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
    """Hydrate a Card's reachable graph into ``output_dir`` (``wyrd get``)."""
    return _get(output_dir, (kind, space, name, version, uid), metadata_only, server)


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
    """Load one Card and materialize its artifacts (``wyrd load``)."""
    return _load((kind, space, name, version, uid), path, server)


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
    """Issue an API key bound to one exact Card (``wyrd auth issue-key``)."""
    return _issue_key((kind, name, version, space), label, expires_in_seconds, server)


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
