"""Public Card types and the server-backed Card client.

Use :class:`Cards` for registry operations and the kind-specific views for
typed calls::

    cards = Cards()
    card = cards.model.get(space="ml", name="fraud-model")
    card.load()

``get`` retrieves the persisted Card envelope. ``DataCard.load`` and
``ModelCard.load`` download the registered artifacts when the returned holder
needs its data or model in memory.
"""

from typing import Protocol

from .._wyrd.cards import (
    CardKind,
    CardList,
    CardRef,
    Cards,
    CardSummary,
    DataCardRegistry,
    DataLoadArgs,
    DataSaveArgs,
    ModelCardRegistry,
    ModelLoadArgs,
    ModelSaveArgs,
    PromptCardRegistry,
    RegistrationOutcome,
    RegistrationReceipt,
    VersionBump,
)
from .._wyrd.cards.agent import AgentCard


class Card(Protocol):
    """Shared authoring capability implemented by native card holders."""

    space: str
    name: str
    version: str
    uid: str

    def _to_card_envelope_json(self) -> str:
        """Return the holder's single Wyrd envelope conversion."""
        ...


__all__ = [
    "AgentCard",
    "Card",
    "CardKind",
    "CardRef",
    "VersionBump",
    "DataSaveArgs",
    "ModelSaveArgs",
    "DataLoadArgs",
    "ModelLoadArgs",
    "Cards",
    "DataCardRegistry",
    "ModelCardRegistry",
    "PromptCardRegistry",
    "CardSummary",
    "CardList",
    "RegistrationOutcome",
    "RegistrationReceipt",
]
