# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from pathlib import Path
from typing import Any

from ..cards import CardKind

#### end of imports ####

class WyrdConfig:
    """Workspace defaults loaded from ``wyrd.toml``.

    The file holds a ``[defaults]`` table and optional ``[kind.<Kind>]``
    tables, each with ``space``, ``labels``, and ``annotations``. Nothing is
    applied implicitly; call ``apply_defaults`` on a metadata dict.
    """

    @classmethod
    def load(cls, path: Path | None = None) -> WyrdConfig:
        """Load the workspace config.

        Args:
            path: the exact ``wyrd.toml`` to read; the file must exist. If
                omitted, the nearest ``wyrd.toml`` is searched for from the
                current directory upward, stopping at the nearest ``.git``
                ancestor or ``$HOME``; when none is found, an empty config is
                returned instead of an error.

        Raises:
            WyrdError: ``WYRD_CFG_400_INVALID_TOML`` when an explicit ``path``
                does not exist or the file cannot be read or parsed;
                ``WYRD_CFG_400_SCHEMA_MISMATCH`` for an unknown key, an
                invalid value, or a ``[kind.External]`` table;
                ``WYRD_CFG_400_NAME_DEFAULT_REJECTED`` when a table sets
                ``name``.

        """
        ...

    def apply_defaults(
        self,
        metadata: dict[str, Any],
        kind: CardKind | str,
    ) -> None:
        """Merge workspace defaults into a Card ``metadata`` dict in place.

        Values already in ``metadata`` always win. ``space`` is filled only
        when absent, from the ``[kind.<Kind>]`` table and then ``[defaults]``.
        ``labels`` and ``annotations`` merge per key with the same
        precedence. ``name``, ``version``, ``uid``, and the hashes are never
        read or written.

        Args:
            metadata: a Card ``metadata`` mapping; it must contain ``name``.
            kind: the Card kind whose ``[kind.<Kind>]`` table applies, as a
                ``CardKind`` or its name such as ``"Model"``.

        Raises:
            WyrdError: ``WYRD_CFG_400_SCHEMA_MISMATCH`` for an unknown kind, a
                missing ``name``, or a mapping that is not valid Card metadata.

        """
        ...

    def __repr__(self) -> str: ...

__all__ = ["WyrdConfig"]
