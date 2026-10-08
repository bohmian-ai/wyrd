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

from typing import Protocol

from .._card_types import (
    AgentSpec,
    DataSpec,
    DriftBaselineState,
    DriftBaselineStatus,
    ModelSpec,
    OperatorSpec,
    PromptSpec,
    RegisteredAgentCard,
    RegisteredArtifactCard,
    RegisteredAuditCard,
    RegisteredCard,
    RegisteredDataCard,
    RegisteredExperimentCard,
    RegisteredMcpCard,
    RegisteredModelCard,
    RegisteredOperatorCard,
    RegisteredPolicyCard,
    RegisteredPromptCard,
    RegisteredServiceCard,
    RegisteredSourceCard,
    RegisteredTriggerCard,
    RegisteredUntypedCard,
    RegisteredVerifierCard,
    RegisteredWorkflowCard,
    ServiceSpec,
    TriggerSpec,
    TypedCardKind,
    VerificationStatus,
    VerifierSpec,
)
from .._card_types import Metadata as CardMetadata
from .._card_types import Relationships as CardRelationships
from .._card_types import Status as CardStatus
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


class Card(Protocol):
    """Shared authoring capability implemented by native card holders.

    ``DataCard``, ``ModelCard``, and ``PromptCard`` are the registerable
    implementations; pass one to ``Cards.register``. Do not construct
    ``Card`` directly.
    """

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
    "WorkflowCards",
    "CardSummary",
    "CardList",
    "RegistrationOutcome",
    "RegistrationReceipt",
    "HydrationSummary",
    "AgentSpec",
    "CardMetadata",
    "CardRelationships",
    "CardStatus",
    "DataSpec",
    "DriftBaselineState",
    "DriftBaselineStatus",
    "ModelSpec",
    "OperatorSpec",
    "PromptSpec",
    "RegisteredAgentCard",
    "RegisteredCard",
    "RegisteredDataCard",
    "RegisteredModelCard",
    "RegisteredOperatorCard",
    "RegisteredPromptCard",
    "RegisteredServiceCard",
    "RegisteredTriggerCard",
    "RegisteredUntypedCard",
    "RegisteredVerifierCard",
    "RegisteredArtifactCard",
    "RegisteredAuditCard",
    "RegisteredExperimentCard",
    "RegisteredMcpCard",
    "RegisteredPolicyCard",
    "RegisteredSourceCard",
    "RegisteredWorkflowCard",
    "ServiceSpec",
    "TriggerSpec",
    "TypedCardKind",
    "VerificationStatus",
    "VerifierSpec",
]
