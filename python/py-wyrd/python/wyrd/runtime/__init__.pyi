# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from ..cards import CardRef

#### end of imports ####

class StateCard:
    """One Card projection in a local WyrdState."""

    @property
    def card_ref(self) -> CardRef:
        """Exact Card reference."""
        ...

    @property
    def aliases(self) -> list[str]:
        """All aliases for this Card."""
        ...

class WyrdState:
    """Complete, local, non-executing Card graph."""

    @staticmethod
    def from_path(path: str) -> WyrdState:
        """Load and validate a complete hydrated bundle."""
        ...

    @property
    def root(self) -> CardRef:
        """Exact root Card reference."""
        ...

    @property
    def aliases(self) -> list[str]:
        """Persisted aliases in stable order."""
        ...

    def get(self, alias: str) -> StateCard | None:
        """Resolve one persisted alias."""
        ...

    def __getitem__(self, alias: str) -> StateCard:
        """Resolve one persisted alias with subscription syntax."""
        ...

__all__ = ["StateCard", "WyrdState"]
