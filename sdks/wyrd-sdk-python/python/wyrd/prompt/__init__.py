"""Provider-native prompts, generation settings, media, and Prompt Cards.

``Prompt`` holds one native request for OpenAI Chat or Responses, Anthropic,
Gemini, Vertex, or an OpenAI-compatible provider, with ``${name}`` text and
``${media:name}`` media variables. ``render()`` returns a ``ProviderRequest``;
``ProviderResponse`` gives typed views of provider replies. ``PromptCard``
stores a prompt as a local ``wyrd/v1`` Prompt Card, and ``PromptReference``
points an Agent at a card or an inline prompt.
"""

from .._wyrd.cards.prompt import (
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
    WyrdError,
)

__all__ = [
    "AnthropicSettings",
    "GeminiSettings",
    "MediaRef",
    "OpenAIResponsesSettings",
    "OpenAISettings",
    "Prompt",
    "PromptCard",
    "PromptCardMetadata",
    "PromptReference",
    "ProviderRequest",
    "ProviderResponse",
    "ResponseFormat",
    "WyrdError",
]
