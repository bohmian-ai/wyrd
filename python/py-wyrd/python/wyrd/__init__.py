"""Public Python package for Wyrd."""

from . import data, model, prompt
from ._wyrd import _init
from ._wyrd.providers import _init_default_providers
from .agent import Agent
from .callbacks import CallbackOutcome
from .data import DataCard, Split, WyrdError
from .error import AgentError, SessionError, ToolError
from .model import ModelCard, ModelSignature, SampleInput
from .observer import Observer
from .prompt import (
    AnthropicSettings,
    GeminiSettings,
    MediaRef,
    OpenAIResponsesSettings,
    OpenAISettings,
    Prompt,
    PromptCard,
    PromptCardMetadata,
    PromptRef,
    ProviderRequest,
    ResponseFormat,
)
from .providers import ProviderRegistry, mock_registry
from .run import AgentRun, FinishReason, RunConfig
from .session import NoSession, Role, SessionMemory, SessionTurn
from .tool import local_registry, tool

_init()
_init_default_providers()

__all__ = [
    "Agent",
    "AgentError",
    "AgentRun",
    "CallbackOutcome",
    "DataCard",
    "FinishReason",
    "MediaRef",
    "OpenAIResponsesSettings",
    "OpenAISettings",
    "AnthropicSettings",
    "GeminiSettings",
    "ModelCard",
    "ModelSignature",
    "NoSession",
    "Observer",
    "Prompt",
    "PromptCard",
    "PromptCardMetadata",
    "PromptRef",
    "ProviderRegistry",
    "ProviderRequest",
    "ResponseFormat",
    "Role",
    "RunConfig",
    "SampleInput",
    "SessionError",
    "SessionMemory",
    "SessionTurn",
    "Split",
    "ToolError",
    "WyrdError",
    "data",
    "local_registry",
    "mock_registry",
    "model",
    "prompt",
    "tool",
]
