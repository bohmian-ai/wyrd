"""Public Card types and the server-backed Card client.

Use :class:`Cards` for registry operations and the kind-specific views for
typed calls::

    cards = Cards()
    card = cards.model.get(space="risk", name="fraud-model")
    card.load()

``get`` retrieves the persisted Card envelope. ``DataCard.load`` and
``ModelCard.load`` download the registered artifacts when the returned holder
needs its data or model in memory; ``get(..., eager_load=True)`` does both in
one call.
"""

from .._wyrd.cards import (
    CardKind,
    CardList,
    CardRef,
    Cards,
    CardSummary,
    DataCardRegistry,
    DataLoadArgs,
    DataSaveArgs,
    HydrationSummary,
    ModelCardRegistry,
    ModelLoadArgs,
    ModelSaveArgs,
    PromptCardRegistry,
    RegistrationOutcome,
    RegistrationReceipt,
    VersionBump,
    WorkflowCards,
)
from .._wyrd.cards.agent import AgentCard
from ._card import Card

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
    "WorkflowCards",
    "CardSummary",
    "CardList",
    "RegistrationOutcome",
    "RegistrationReceipt",
    "HydrationSummary",
]
