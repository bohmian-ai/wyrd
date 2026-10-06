"""Public Python package for Wyrd."""

from typing import Protocol

from . import cards, client, config, data, model, prompt, state
from ._wyrd import AgentError, SessionError, ToolError, WyrdError
from .agent import (
    Agent,
    AgentRun,
    FinishReason,
    NoSession,
    Role,
    RunConfig,
    SessionMemory,
    SessionTurn,
    Workflow,
    WorkflowRun,
    local_registry,
    tool,
)
from .cards import AgentCard, CardKind, CardRef, Cards, RegistrationReceipt
from .cli import run_wyrd_cli
from .client import WyrdClient
from .config import WyrdConfig
from .data import DataCard, Split
from .model import ModelCard, ModelSignature, SampleInput
from .prompt import (
    AnthropicSettings,
    GeminiSettings,
    MediaRef,
    OpenAIResponsesSettings,
    OpenAISettings,
    Prompt,
    PromptCard,
    PromptCardMetadata,
    PromptReference,
    ProviderRequest,
    ProviderResponse,
    ResponseFormat,
)
from .state import CardEnvelope, HydratedArtifact, WyrdState


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


cards.Card = Card

__all__ = [
    "Agent",
    "AgentCard",
    "AgentError",
    "AgentRun",
    "AnthropicSettings",
    "Card",
    "CardKind",
    "CardRef",
    "Cards",
    "DataCard",
    "FinishReason",
    "GeminiSettings",
    "MediaRef",
    "ModelCard",
    "ModelSignature",
    "NoSession",
    "OpenAIResponsesSettings",
    "OpenAISettings",
    "Prompt",
    "PromptCard",
    "PromptCardMetadata",
    "PromptReference",
    "ProviderRequest",
    "ProviderResponse",
    "ResponseFormat",
    "RegistrationReceipt",
    "Role",
    "RunConfig",
    "SampleInput",
    "SessionError",
    "SessionMemory",
    "SessionTurn",
    "Split",
    "ToolError",
    "Workflow",
    "WorkflowRun",
    "WyrdError",
    "CardEnvelope",
    "HydratedArtifact",
    "WyrdState",
    "WyrdClient",
    "cards",
    "client",
    "config",
    "data",
    "local_registry",
    "model",
    "prompt",
    "state",
    "run_wyrd_cli",
    "tool",
    "WyrdConfig",
]
