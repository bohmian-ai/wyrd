#### begin imports ####
from pathlib import Path
from typing import Any

from .cards import CardKind

#### end of imports ####

class WyrdConfig:
    """Workspace configuration loaded from `wyrd.toml`."""

    @classmethod
    def load(cls, path: Path | None = None) -> WyrdConfig:
        """Load the workspace config.

        Two **asymmetric** missing-file paths:

        * ``path=None``: ancestor-walk from CWD, bounded by the
          nearest ``.git`` ancestor or ``$HOME``. Missing
          ``wyrd.toml`` returns an **empty config** (no error).
        * ``path=Path(...)``: read that exact file. Missing -> ``wyrd.WyrdError``
          with code ``WYRD_CFG_400_INVALID_TOML``.
        """
        ...

    def apply_defaults(
        self,
        metadata: dict[str, Any],
        kind: CardKind | str,
    ) -> None:
        """In-place merge of workspace defaults into a metadata dict."""
        ...

    def __repr__(self) -> str: ...

__all__ = ["WyrdConfig"]
