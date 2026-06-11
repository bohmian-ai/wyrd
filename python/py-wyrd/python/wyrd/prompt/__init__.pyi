# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from collections.abc import Mapping
from typing import Any

from ..cards import CardRef
from .._wyrd import JsonDict, PathLike, WyrdError

#### end of imports ####

class MediaRef:
    """Reference to media content used by `Prompt.bind_media`.

    Create a media reference with one of the static constructors and bind it
    to a prompt placeholder written as `${media:name}`. Text placeholders
    written as `{{name}}` are unaffected by media binding; the two token
    namespaces are disjoint.

    Provider support depends on media kind and source. OpenAI accepts image
    URLs, image base64 data, image files, document base64 data, and document
    files, but rejects document URLs. Anthropic accepts image and document
    URLs, base64 data, and file ids. Gemini and Vertex accept inline base64
    data and file data; URL sources must be `gs://` or Gemini file API URIs
    with an explicit MIME type.

    Examples:
        >>> from wyrd import MediaRef, Prompt
        >>> prompt = Prompt("see: ${media:logo}", "gpt-4o", provider="openai")
        >>> bound = prompt.bind_media("logo", MediaRef.image_url("https://x/logo.png"))
        >>> assert bound.media_variables == []
    """

    kind: str
    source_type: str

    @staticmethod
    def image_url(url: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct an image reference from a provider-accessible URL.

        Args:
            url (str): Remote image URL passed to the provider.
            mime_type (str | None): Optional MIME type. Required when `url`
                is a Gemini or Vertex `gs://` reference or Gemini file API URI.

        Returns:
            MediaRef: Image reference with `source_type == "url"`.
        """
        ...

    @staticmethod
    def image_bytes(mime_type: str, data: bytes) -> MediaRef:
        """Construct an image reference from raw bytes.

        The bytes are base64-encoded eagerly and the input bytes are not
        retained.

        Args:
            mime_type (str): Image MIME type, such as `image/png`.
            data (bytes): Raw image bytes.

        Returns:
            MediaRef: Image reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def image_base64(mime_type: str, data: str) -> MediaRef:
        """Construct an image reference from existing base64 data.

        Args:
            mime_type (str): Image MIME type, such as `image/png`.
            data (str): Base64 payload without a `data:` prefix.

        Returns:
            MediaRef: Image reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def image_file(uri: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct an image reference from a provider file id or URI.

        Args:
            uri (str): OpenAI file id, Anthropic file id, Gemini file API URI,
                or Vertex file URI.
            mime_type (str | None): Optional MIME type. Required for Gemini
                and Vertex.

        Returns:
            MediaRef: Image reference with `source_type == "file"`.
        """
        ...

    @staticmethod
    def image_path(path: PathLike) -> MediaRef:
        """Construct an image reference by eagerly reading a local file.

        MIME type is inferred from the extension. Supported image extensions
        are `png`, `jpg`, `jpeg`, `gif`, and `webp`.

        Args:
            path (PathLike): Regular local file no larger than 20 MiB.

        Returns:
            MediaRef: Image reference with `source_type == "base64"`.

        Raises:
            WyrdError: If the path is not a regular file, exceeds 20 MiB, has
                an unsupported extension, or cannot be read.
        """
        ...

    @staticmethod
    def document_url(url: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct a document reference from a provider-accessible URL.

        OpenAI rejects document URLs at bind time because its chat file part
        has no document-URL primitive.

        Args:
            url (str): Remote document URL, `gs://` URI, or Gemini file API URI.
            mime_type (str | None): Optional MIME type. Required for Gemini
                and Vertex URL sources.

        Returns:
            MediaRef: Document reference with `source_type == "url"`.

        Raises:
            WyrdError: Later binding to OpenAI raises
                `WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER`.
        """
        ...

    @staticmethod
    def document_bytes(mime_type: str, data: bytes) -> MediaRef:
        """Construct a document reference from raw bytes.

        The bytes are base64-encoded eagerly and accepted by all supported
        prompt providers.

        Args:
            mime_type (str): Document MIME type, such as `application/pdf`.
            data (bytes): Raw document bytes.

        Returns:
            MediaRef: Document reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def document_base64(mime_type: str, data: str) -> MediaRef:
        """Construct a document reference from existing base64 data.

        Args:
            mime_type (str): Document MIME type, such as `application/pdf`.
            data (str): Base64 payload without a `data:` prefix.

        Returns:
            MediaRef: Document reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def document_file(uri: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct a document reference from a provider file id or URI.

        Args:
            uri (str): OpenAI file id, Anthropic file id, Gemini file API URI,
                or Vertex file URI.
            mime_type (str | None): Optional MIME type. Required for Gemini
                and Vertex.

        Returns:
            MediaRef: Document reference with `source_type == "file"`.
        """
        ...

    @staticmethod
    def document_path(path: PathLike) -> MediaRef:
        """Construct a document reference by eagerly reading a local file.

        MIME type is inferred from the extension. Supported document
        extensions include `pdf`, `txt`, `md`, `json`, `csv`, `html`, and
        `htm`.

        Args:
            path (PathLike): Regular local file no larger than 20 MiB.

        Returns:
            MediaRef: Document reference with `source_type == "base64"`.

        Raises:
            WyrdError: If the path is not a regular file, exceeds 20 MiB, has
                an unsupported extension, or cannot be read.
        """
        ...

    def __repr__(self) -> str:
        """Return a concise representation without payload bytes or URLs.

        Returns:
            str: Redacted media reference representation.
        """
        ...

class ProviderRequest:
    """Opaque provider-native request returned by prompt rendering."""

    provider: str
    messages: list[JsonDict]
    message: JsonDict | None
    system: Any

    def model_dump(self) -> JsonDict:
        """Return the native provider request as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible provider request.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the native provider request as JSON.

        Returns:
            str: Serialized provider request JSON.
        """
        ...

    def openai(self) -> OpenAiChatRequest:
        """Return the typed OpenAI Chat request accessor."""
        ...

    def openai_responses(self) -> OpenAiResponsesRequest:
        """Return the typed OpenAI Responses request accessor."""
        ...

    def anthropic(self) -> AnthropicMessagesRequest:
        """Return the typed Anthropic Messages request accessor."""
        ...

    def gemini(self) -> GeminiRequest:
        """Return the typed Google Gemini request accessor."""
        ...

    def vertex(self) -> VertexRequest:
        """Return the typed Vertex AI request accessor."""
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the provider request.
        """
        ...

class ProviderResponse:
    """Typed provider response wrapper. Use provider accessors for typed field access."""

    @property
    def provider(self) -> str: ...
    def openai(self) -> OpenAiChatResponse: ...
    def openai_responses(self) -> OpenAiResponsesResponse: ...
    def anthropic(self) -> AnthropicMessagesResponse: ...
    def gemini(self) -> GeminiResponse: ...
    def vertex(self) -> VertexResponse: ...
    def model_dump(self) -> JsonDict: ...
    def model_dump_json(self) -> str: ...

class OpenAiChatRequest:
    @property
    def model(self) -> str: ...
    @property
    def messages(self) -> list[OpenAiChatMessage]: ...
    @property
    def stream(self) -> bool | None: ...
    @property
    def parallel_tool_calls(self) -> bool | None: ...

class OpenAiChatResponse:
    @property
    def id(self) -> str: ...
    @property
    def object(self) -> str: ...
    @property
    def created(self) -> int: ...
    @property
    def model(self) -> str: ...
    @property
    def system_fingerprint(self) -> str | None: ...
    @property
    def service_tier(self) -> str | None: ...
    @property
    def usage(self) -> OpenAiUsage | None: ...
    @property
    def choices(self) -> list[OpenAiChatChoice]: ...

class OpenAiChatChoice:
    @property
    def index(self) -> int: ...
    @property
    def finish_reason(self) -> str | None: ...
    @property
    def message(self) -> OpenAiChatMessage: ...
    @property
    def logprobs(self) -> OpenAiChatLogprobs | None: ...

class OpenAiChatMessage:
    @property
    def role(self) -> str: ...
    @property
    def content(self) -> OpenAiMessageContent | None: ...
    @property
    def name(self) -> str | None: ...
    @property
    def tool_calls(self) -> list[OpenAiToolCall] | None: ...
    @property
    def tool_call_id(self) -> str | None: ...
    @property
    def refusal(self) -> str | None: ...
    @property
    def annotations(self) -> list[OpenAiMessageAnnotation]: ...
    @property
    def audio(self) -> OpenAiMessageAudio | None: ...

class OpenAiMessageContent:
    @property
    def kind(self) -> str: ...
    def as_text(self) -> str: ...
    def as_parts(self) -> list[OpenAiContentPart]: ...

class OpenAiContentPart:
    @property
    def kind(self) -> str: ...
    def as_text(self) -> str: ...
    def as_image_url(self) -> OpenAiImageUrl: ...
    def as_input_audio(self) -> OpenAiInputAudio: ...
    def as_file(self) -> OpenAiFilePart: ...

class OpenAiImageUrl:
    @property
    def url(self) -> str: ...
    @property
    def detail(self) -> str | None: ...

class OpenAiInputAudio:
    @property
    def data(self) -> str: ...
    @property
    def format(self) -> str: ...

class OpenAiFilePart:
    @property
    def file_id(self) -> str | None: ...
    @property
    def file_data(self) -> str | None: ...
    @property
    def filename(self) -> str | None: ...

class OpenAiToolCall:
    @property
    def id(self) -> str: ...
    @property
    def kind(self) -> str: ...
    @property
    def function(self) -> OpenAiToolFunctionCall: ...

class OpenAiToolFunctionCall:
    @property
    def name(self) -> str: ...
    @property
    def arguments(self) -> str: ...

class OpenAiMessageAnnotation:
    @property
    def kind(self) -> str: ...
    @property
    def url_citation(self) -> OpenAiUrlCitation: ...

class OpenAiUrlCitation:
    @property
    def url(self) -> str: ...
    @property
    def title(self) -> str: ...
    @property
    def start_index(self) -> int: ...
    @property
    def end_index(self) -> int: ...

class OpenAiMessageAudio:
    @property
    def id(self) -> str: ...
    @property
    def expires_at(self) -> int: ...
    @property
    def data(self) -> str: ...
    @property
    def transcript(self) -> str: ...

class OpenAiUsage:
    @property
    def prompt_tokens(self) -> int: ...
    @property
    def completion_tokens(self) -> int: ...
    @property
    def total_tokens(self) -> int: ...
    @property
    def prompt_tokens_details(self) -> OpenAiPromptTokensDetails | None: ...
    @property
    def completion_tokens_details(self) -> OpenAiCompletionTokensDetails | None: ...

class OpenAiPromptTokensDetails:
    @property
    def audio_tokens(self) -> int: ...
    @property
    def cached_tokens(self) -> int: ...

class OpenAiCompletionTokensDetails:
    @property
    def accepted_prediction_tokens(self) -> int: ...
    @property
    def audio_tokens(self) -> int: ...
    @property
    def reasoning_tokens(self) -> int: ...
    @property
    def rejected_prediction_tokens(self) -> int: ...

class OpenAiChatLogprobs:
    @property
    def content(self) -> list[Any]: ...
    @property
    def refusal(self) -> list[Any]: ...

class AnthropicMessagesRequest:
    @property
    def model(self) -> str: ...

class AnthropicMessagesResponse:
    @property
    def id(self) -> str: ...
    @property
    def model(self) -> str: ...
    @property
    def role(self) -> str: ...
    @property
    def stop_reason(self) -> str | None: ...
    @property
    def stop_sequence(self) -> str | None: ...
    @property
    def usage(self) -> AnthropicUsage: ...

class AnthropicMessage:
    @property
    def role(self) -> str: ...
    @property
    def content(self) -> list[AnthropicContentBlock]: ...

class AnthropicContentBlock:
    @property
    def kind(self) -> str: ...
    def as_text(self) -> str: ...

class AnthropicUsage:
    @property
    def input_tokens(self) -> int: ...
    @property
    def output_tokens(self) -> int: ...
    @property
    def cache_creation_input_tokens(self) -> int: ...
    @property
    def cache_read_input_tokens(self) -> int: ...

class GeminiRequest:
    @property
    def contents(self) -> list[GoogleContent]: ...

class GeminiResponse:
    @property
    def candidates(self) -> list[GoogleCandidate]: ...
    @property
    def usage_metadata(self) -> GoogleUsageMetadata | None: ...

class GoogleContent:
    @property
    def role(self) -> str: ...
    @property
    def parts(self) -> list[GooglePart]: ...

class GooglePart:
    @property
    def kind(self) -> str: ...
    def as_text(self) -> str: ...

class GoogleCandidate:
    @property
    def finish_reason(self) -> str | None: ...
    @property
    def index(self) -> int | None: ...

class GoogleUsageMetadata:
    @property
    def prompt_token_count(self) -> int: ...
    @property
    def candidates_token_count(self) -> int: ...
    @property
    def total_token_count(self) -> int: ...

class VertexRequest:
    @property
    def contents(self) -> list[GoogleContent]: ...

class VertexResponse:
    @property
    def candidates(self) -> list[GoogleCandidate]: ...

class OpenAiResponsesRequest:
    @property
    def model(self) -> str: ...
    @property
    def instructions(self) -> str | None: ...

class OpenAiResponsesResponse:
    @property
    def id(self) -> str: ...
    @property
    def model(self) -> str: ...
    @property
    def status(self) -> str: ...
    @property
    def usage(self) -> OpenAiResponsesUsage | None: ...

class OpenAiResponsesUsage:
    @property
    def input_tokens(self) -> int: ...
    @property
    def output_tokens(self) -> int: ...
    @property
    def total_tokens(self) -> int: ...

class ResponseFormat:
    """Provider-independent response-format authoring helper."""

    @staticmethod
    def text() -> ResponseFormat:
        """Request plain-text output.

        Returns:
            ResponseFormat: Response-format helper for plain text.
        """
        ...

    @staticmethod
    def json_object() -> ResponseFormat:
        """Request provider-native JSON object output.

        Returns:
            ResponseFormat: Response-format helper for provider JSON object
                mode.
        """
        ...

    @staticmethod
    def json_schema(name: str, schema: JsonDict) -> ResponseFormat:
        """Request structured JSON output matching `schema`.

        Args:
            name (str): Provider-visible schema name.
            schema (JsonDict): JSON Schema object that the response must
                satisfy.

        Returns:
            ResponseFormat: Response-format helper for provider JSON Schema
                mode.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return the response format as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible response format configuration.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the response format.
        """
        ...

class OpenAISettings:
    """OpenAI Chat generation settings.

    These fields serialize at the top level of the native Chat Completions
    request. Unknown keyword arguments are preserved and emitted at the same
    top-level location.

    Args:
        temperature (float | None): Sampling temperature.
        top_p (float | None): Nucleus sampling probability.
        max_tokens (int | None): Legacy Chat output-token cap.
        max_completion_tokens (int | None): Chat completion-token cap.
        n (int | None): Number of completions to generate.
        stop (str | list[str] | None): Stop sequence or sequences.
        presence_penalty (float | None): Presence penalty.
        frequency_penalty (float | None): Frequency penalty.
        seed (int | None): Provider best-effort deterministic seed.
        logit_bias (Mapping[str, Any] | None): Token-bias map keyed by token id.
        user (str | None): Provider-visible end-user identifier.
        reasoning_effort (str | None): Reasoning effort label.
        modalities (list[str] | None): Requested output modalities.
        audio (JsonDict | None): Audio output settings.
        prediction (JsonDict | None): Prediction content hint.
        prompt_cache_key (str | None): OpenAI prompt cache key.
        service_tier (str | None): OpenAI service tier.
        safety_identifier (str | None): Safety identifier.
        store (bool | None): Whether the provider may store the response.
        metadata (Mapping[str, Any] | None): Provider metadata object.
        logprobs (bool | None): Whether to return token log probabilities.
        top_logprobs (int | None): Number of top token log probabilities.
        **extra (Any): Unmodeled OpenAI Chat fields.
    """

    def __init__(
        self,
        *,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_tokens: int | None = ...,
        max_completion_tokens: int | None = ...,
        n: int | None = ...,
        stop: str | list[str] | None = ...,
        presence_penalty: float | None = ...,
        frequency_penalty: float | None = ...,
        seed: int | None = ...,
        logit_bias: Mapping[str, Any] | None = ...,
        user: str | None = ...,
        reasoning_effort: str | None = ...,
        modalities: list[str] | None = ...,
        audio: JsonDict | None = ...,
        prediction: JsonDict | None = ...,
        prompt_cache_key: str | None = ...,
        service_tier: str | None = ...,
        safety_identifier: str | None = ...,
        store: bool | None = ...,
        metadata: Mapping[str, Any] | None = ...,
        logprobs: bool | None = ...,
        top_logprobs: int | None = ...,
        **extra: Any,
    ) -> None:
        """Create OpenAI Chat settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> OpenAISettings:
        """Create OpenAI Chat settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class OpenAIResponsesSettings:
    """OpenAI Responses generation settings.

    These fields serialize at the top level of the native Responses request.
    Unknown keyword arguments are preserved and emitted at the same top-level
    location.

    Args:
        temperature (float | None): Sampling temperature.
        top_p (float | None): Nucleus sampling probability.
        max_output_tokens (int | None): Responses output-token cap.
        reasoning (JsonDict | None): Responses reasoning configuration.
        store (bool | None): Whether the provider may store the response.
        include (list[str] | None): Additional response fields to include.
        metadata (Mapping[str, Any] | None): Provider metadata object.
        **extra (Any): Unmodeled OpenAI Responses fields.
    """

    def __init__(
        self,
        *,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_output_tokens: int | None = ...,
        reasoning: JsonDict | None = ...,
        store: bool | None = ...,
        include: list[str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
        **extra: Any,
    ) -> None:
        """Create OpenAI Responses settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> OpenAIResponsesSettings:
        """Create OpenAI Responses settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class AnthropicSettings:
    """Anthropic Messages generation settings.

    These fields serialize at the top level of the native Messages request.
    `max_tokens` defaults to `4096`. Unknown keyword arguments are preserved
    and emitted at the same top-level location.

    Args:
        max_tokens (int): Maximum output tokens.
        temperature (float | None): Sampling temperature.
        top_p (float | None): Nucleus sampling probability.
        top_k (int | None): Top-k sampling limit.
        stop_sequences (list[str] | None): Stop sequences.
        metadata (Mapping[str, Any] | None): Provider metadata object.
        thinking (JsonDict | None): Anthropic thinking configuration.
        **extra (Any): Unmodeled Anthropic Messages fields.
    """

    def __init__(
        self,
        *,
        max_tokens: int = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        top_k: int | None = ...,
        stop_sequences: list[str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
        thinking: JsonDict | None = ...,
        **extra: Any,
    ) -> None:
        """Create Anthropic settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> AnthropicSettings:
        """Create Anthropic settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class GeminiSettings:
    """Gemini and Vertex GenerateContent settings.

    These fields serialize at the top level of the native GenerateContent
    request. Generation knobs such as `temperature`, `top_p`, `top_k`,
    `max_output_tokens`, and `thinking_config` live inside
    `generation_config`. Unknown keyword arguments are preserved and emitted at
    the request top level.

    Args:
        generation_config (JsonDict | None): Native Google generationConfig object.
        safety_settings (list[JsonDict] | None): Native Google safetySettings list.
        cached_content (str | None): Cached content resource name.
        labels (Mapping[str, Any] | None): Provider labels object.
        **extra (Any): Unmodeled Gemini or Vertex request fields.
    """

    def __init__(
        self,
        *,
        generation_config: JsonDict | None = ...,
        safety_settings: list[JsonDict] | None = ...,
        cached_content: str | None = ...,
        labels: Mapping[str, Any] | None = ...,
        **extra: Any,
    ) -> None:
        """Create Gemini or Vertex settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> GeminiSettings:
        """Create Gemini or Vertex settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class Prompt:
    """Client-safe native prompt builder.

    Prompt text variables and media variables use separate placeholder
    namespaces. Text variables use `{{name}}` and are bound with `bind` or
    `bind_mut`. Media variables use `${media:name}` and are bound with
    `bind_media` or `bind_media_mut`.

    Media placeholders are split into isolated provider-native text parts
    during prompt construction. Binding media replaces those sentinel parts
    with provider-native content blocks while preserving the original provider
    request shape.

    Examples:
        >>> from wyrd import MediaRef, Prompt
        >>> prompt = Prompt("Hi {{name}}, see ${media:logo}", "gpt-4o", provider="openai")
        >>> assert prompt.variables == ["name"]
        >>> assert prompt.media_variables == ["logo"]
        >>> bound = prompt.bind("name", "Ada").bind_media(
        ...     "logo",
        ...     MediaRef.image_url("https://example.com/logo.png"),
        ... )
        >>> assert bound.variables == []
        >>> assert bound.media_variables == []
    """

    provider: str
    request: ProviderRequest
    messages: list[JsonDict]
    message: JsonDict | None
    system_messages: Any
    model: str
    version: str | None
    variables: list[str]
    media_variables: list[str]
    model_settings: (
        OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | None
    )

    def __init__(
        self,
        messages: Any,
        model: str,
        *,
        provider: str,
        system: str | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        output: dict[str, type] | type | JsonDict | ResponseFormat | None = ...,
        operation: str | None = ...,
        cache: str | Mapping[str, Any] | None = ...,
        model_settings: OpenAISettings
        | OpenAIResponsesSettings
        | AnthropicSettings
        | GeminiSettings
        | Mapping[str, Any]
        | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> None:
        """Build a provider-native prompt from dynamic authoring inputs.

        Args:
            messages (Any): Provider-shaped message input. Strings are treated
                as user text; sequences and mappings are coerced into the
                target provider's native request shape.
            model (str): Provider model identifier to store on the native
                prompt and request.
            provider (str): Provider name. Accepted values are `openai`,
                `anthropic`, `gemini`, `google`, `vertex`, or a custom provider
                accepted by the raw passthrough path.
            system (str | None): Optional system instruction when supported by
                the selected provider.
            response_format (ResponseFormat | JsonDict | None): Optional
                structured-output helper or schema dictionary.
            output (dict[str, type] | type | JsonDict | ResponseFormat | None): Optional structured-output declaration. Accepts dict[str, type], raw JSON Schema, ResponseFormat, or pydantic BaseModel subclass. When set, output wins over response_format.
            operation (str | None): Optional provider operation selector used
                by provider families with more than one request shape.
            cache (str | Mapping[str, Any] | None): Optional cache sugar for
                providers with a native cache key.
            model_settings (OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | Mapping[str, Any] | None): Native provider generation settings object or mapping. Explicit settings take precedence over cache sugar.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `${name}` and `{{name}}` placeholders from the request.
            version (str | None): Optional prompt version string stored on the
                native prompt.
        """
        ...

    @staticmethod
    def openai_chat(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        output: dict[str, type] | type | JsonDict | ResponseFormat | None = ...,
        cache: str | Mapping[str, Any] | None = ...,
        model_settings: OpenAISettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an OpenAI Chat prompt.

        Args:
            model (str): OpenAI model identifier.
            system (str | None): Optional system message prepended to the chat.
            messages (Any | None): Optional user-authored chat messages.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            output (dict[str, type] | type | JsonDict | ResponseFormat | None): Optional structured-output declaration. When set, output wins over response_format.
            cache (str | Mapping[str, Any] | None): Optional prompt cache key.
            model_settings (OpenAISettings | Mapping[str, Any] | None): OpenAI settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `${name}` and `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping an OpenAI Chat Completions request.
        """
        ...

    @staticmethod
    def openai_responses(
        model: str,
        *,
        instructions: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        output: dict[str, type] | type | JsonDict | ResponseFormat | None = ...,
        model_settings: OpenAIResponsesSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an OpenAI Responses prompt.

        Args:
            model (str): OpenAI model identifier.
            instructions (str | None): Optional Responses API instructions.
            messages (Any | None): Optional Responses API input items.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            output (dict[str, type] | type | JsonDict | ResponseFormat | None): Optional structured-output declaration. When set, output wins over response_format.
            model_settings (OpenAIResponsesSettings | Mapping[str, Any] | None): OpenAI Responses settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `${name}` and `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping an OpenAI Responses request.
        """
        ...

    @staticmethod
    def anthropic(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        output: dict[str, type] | type | JsonDict | ResponseFormat | None = ...,
        model_settings: AnthropicSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an Anthropic Messages prompt.

        Args:
            model (str): Anthropic model identifier.
            system (str | None): Optional system instruction.
            messages (Any | None): Optional Anthropic message list.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            output (dict[str, type] | type | JsonDict | ResponseFormat | None): Optional structured-output declaration. When set, output wins over response_format.
            model_settings (AnthropicSettings | Mapping[str, Any] | None): Anthropic settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `${name}` and `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping an Anthropic Messages request.
        """
        ...

    @staticmethod
    def gemini(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        output: dict[str, type] | type | JsonDict | ResponseFormat | None = ...,
        model_settings: GeminiSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a Gemini GenerateContent prompt.

        Args:
            model (str): Gemini model identifier.
            system (str | None): Optional system instruction.
            messages (Any | None): Optional Gemini content turns.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            output (dict[str, type] | type | JsonDict | ResponseFormat | None): Optional structured-output declaration. When set, output wins over response_format.
            model_settings (GeminiSettings | Mapping[str, Any] | None): Gemini settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `${name}` and `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping a Gemini GenerateContent request.
        """
        ...

    @staticmethod
    def vertex(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        output: dict[str, type] | type | JsonDict | ResponseFormat | None = ...,
        model_settings: GeminiSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a Vertex GenerateContent prompt.

        Args:
            model (str): Vertex model identifier.
            system (str | None): Optional system instruction.
            messages (Any | None): Optional Vertex content turns.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            output (dict[str, type] | type | JsonDict | ResponseFormat | None): Optional structured-output declaration. When set, output wins over response_format.
            model_settings (GeminiSettings | Mapping[str, Any] | None): Gemini settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `${name}` and `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping a Vertex GenerateContent request.
        """
        ...

    @staticmethod
    def raw(provider: str, model: str, body: bytes) -> Prompt:
        """Build a raw JSON passthrough prompt.

        Args:
            provider (str): Provider dispatch target for the raw body.
            model (str): Model identifier used for indexing and selection.
            body (bytes): Raw provider request JSON bytes.

        Returns:
            Prompt: Prompt wrapping a raw provider request.
        """
        ...

    def system(self, text: str) -> Prompt:
        """Return a copy with a system message applied.

        Args:
            text (str): System instruction text.

        Returns:
            Prompt: New prompt with the system instruction applied.
        """
        ...

    def user(self, content: Any) -> Prompt:
        """Return a copy with a user message appended.

        Args:
            content (Any): Provider-compatible user message content.

        Returns:
            Prompt: New prompt with the user message appended.
        """
        ...

    def assistant(self, content: Any) -> Prompt:
        """Return a copy with an assistant message appended.

        Args:
            content (Any): Provider-compatible assistant message content.

        Returns:
            Prompt: New prompt with the assistant message appended.
        """
        ...

    def tool_result(self, tool_use_id: str, content: str, is_error: bool = ...) -> Prompt:
        """Return a copy with a native tool-result message appended.

        Args:
            tool_use_id (str): Provider tool-call identifier being answered.
            content (str): Tool result text.
            is_error (bool): Whether the tool result represents an error.

        Returns:
            Prompt: New prompt with the tool-result message appended.
        """
        ...

    def render(self, **kwargs: str) -> ProviderRequest:
        """Render declared variables and return a provider request.

        Args:
            **kwargs (str): Text variable bindings keyed by variable name.

        Returns:
            ProviderRequest: Rendered provider-native request.
        """
        ...

    def bind(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> Prompt:
        """Return a copy with one or more variables bound.

        Args:
            name (str | None): Optional single text variable name.
            value (Any | None): Optional value for `name`.
            **kwargs (Any): Additional text variable bindings.

        Returns:
            Prompt: New prompt with bound text variables removed from
                `variables`.
        """
        ...

    def bind_mut(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> None:
        """Bind one or more variables in place.

        Args:
            name (str | None): Optional single text variable name.
            value (Any | None): Optional value for `name`.
            **kwargs (Any): Additional text variable bindings.
        """
        ...

    def bind_media(self, name: str, media: MediaRef) -> Prompt:
        """Return a copy with a media placeholder bound.

        The matching `${media:name}` text part is replaced with the typed
        provider-native content variant for this prompt's provider. The
        original prompt is unchanged.

        Args:
            name (str): Media parameter name, without the `${media:...}`
                wrapper.
            media (MediaRef): Media payload to insert.

        Returns:
            Prompt: New prompt with `name` removed from `media_variables`.

        Raises:
            WyrdError: If the placeholder is missing, not isolated, unsupported
                by the provider, or missing a required MIME type.
        """
        ...

    def bind_media_mut(self, name: str, media: MediaRef) -> None:
        """Bind a media placeholder in place.

        Args:
            name (str): Media parameter name, without the `${media:...}`
                wrapper.
            media (MediaRef): Media payload to insert.

        Raises:
            WyrdError: Same failures as `bind_media`.
        """
        ...

    def model_dump(self) -> JsonDict:
        """Return the native prompt as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible native prompt representation.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the native prompt as JSON.

        Returns:
            str: Serialized native prompt JSON.
        """
        ...

    @staticmethod
    def model_validate_json(data: str) -> Prompt:
        """Build a prompt from serialized native prompt JSON.

        Args:
            data (str): Serialized native prompt JSON.

        Returns:
            Prompt: Prompt rebuilt from the serialized native prompt.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> Prompt:
        """Load a bare native prompt from JSON or YAML.

        Args:
            path (PathLike): Source `.json`, `.yaml`, or `.yml` prompt file.

        Returns:
            Prompt: Prompt loaded from the bare prompt spec file.
        """
        ...

    def dump(self, path: PathLike) -> None:
        """Dump a bare native prompt to JSON or YAML.

        Args:
            path (PathLike): Target `.json`, `.yaml`, or `.yml` prompt file.
        """
        ...

    @staticmethod
    def openai_image_url(url: str, detail: str | None = ...) -> JsonDict:
        """Return an OpenAI image URL content part.

        Args:
            url (str): Image URL.
            detail (str | None): Optional OpenAI image detail hint.

        Returns:
            JsonDict: OpenAI image URL content part.
        """
        ...

    @staticmethod
    def anthropic_image_base64(media_type: str, data: str) -> JsonDict:
        """Return an Anthropic base64 image content block.

        Args:
            media_type (str): Image MIME type.
            data (str): Base64 image payload.

        Returns:
            JsonDict: Anthropic image content block.
        """
        ...

    @staticmethod
    def google_file_data(mime_type: str, file_uri: str) -> JsonDict:
        """Return a Google file-data content part.

        Args:
            mime_type (str): File MIME type.
            file_uri (str): Google file URI.

        Returns:
            JsonDict: Google file-data content part.
        """
        ...

    @staticmethod
    def image_url(url: str, *, detail: str | None = ..., provider: str = ...) -> JsonDict:
        """Return an image URL content helper.

        Args:
            url (str): Image URL.
            detail (str | None): Optional image detail hint.
            provider (str): Provider-specific helper target.

        Returns:
            JsonDict: Provider-specific image URL content value.
        """
        ...

    @staticmethod
    def image_base64(media_type: str, data: str) -> JsonDict:
        """Return a base64 image content helper.

        Args:
            media_type (str): Image MIME type.
            data (str): Base64 image payload.

        Returns:
            JsonDict: Provider-specific base64 image content value.
        """
        ...

    @staticmethod
    def file_uri(mime_type: str, file_uri: str) -> JsonDict:
        """Return a file URI content helper.

        Args:
            mime_type (str): File MIME type.
            file_uri (str): Provider file URI.

        Returns:
            JsonDict: Provider-specific file URI content value.
        """
        ...

    @staticmethod
    def file_id(file_id: str) -> JsonDict:
        """Return a file-id content helper.

        Args:
            file_id (str): Provider file identifier.

        Returns:
            JsonDict: Provider-specific file-id content value.
        """
        ...

    @staticmethod
    def document_text(media_type: str, data: str, title: str | None = ...) -> JsonDict:
        """Return a text document content helper.

        Args:
            media_type (str): Document MIME type.
            data (str): Document text payload.
            title (str | None): Optional document title.

        Returns:
            JsonDict: Provider-specific text document content value.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the native prompt.
        """
        ...

class PromptRef:
    """Python-facing Wyrd prompt reference.

    A prompt reference points at either a registered Prompt Card or an inline
    Prompt spec.
    """

    kind: str

    @staticmethod
    def card(
        name: str, version: str, *, space: str | None = ..., uid: str | None = ...
    ) -> PromptRef:
        """Create a reference to a registered Prompt Card."""
        ...

    @staticmethod
    def inline(prompt: Prompt) -> PromptRef:
        """Create an inline prompt reference from a Prompt."""
        ...

    def model_dump(self) -> JsonDict:
        """Return this prompt reference as a Python dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return this prompt reference as JSON."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> PromptRef:
        """Build a prompt reference from serialized JSON."""
        ...

class PromptCardMetadata:
    """Local holder metadata used when serializing a `PromptCard`.

    The metadata stores the native prompt body that becomes
    `PromptSpec.prompt` in the Wyrd card envelope.
    """

    prompt: Prompt

    def __init__(
        self,
        prompt: Prompt | None = ...,
        *,
        model_settings: OpenAISettings
        | OpenAIResponsesSettings
        | AnthropicSettings
        | GeminiSettings
        | Mapping[str, Any]
        | None = ...,
    ) -> None:
        """Create prompt card metadata from an optional `Prompt`.

        Args:
            prompt (Prompt | None): Native prompt builder to store in metadata.
                When omitted, Wyrd creates placeholder metadata that callers can
                replace before serialization.
            model_settings (OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | Mapping[str, Any] | None): Provider-native generation settings to apply to `prompt` before storing metadata.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this metadata as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible holder metadata.
        """
        ...

    def to_spec(self) -> JsonDict:
        """Return the validated PromptSpec body as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible `PromptSpec` body containing the native
                prompt.

        Raises:
            WyrdError: If the stored prompt violates PromptCard validation.
        """
        ...

class PromptCard:
    """Local PromptCard holder for filesystem materialization.

    A `PromptCard` wraps a live `Prompt`, local card identity, labels,
    annotations, and metadata. It can save and load local JSON or YAML card
    envelopes, but registration belongs to registry/client surfaces.
    """

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    metadata: PromptCardMetadata
    prompt: Prompt
    model_settings: (
        OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | None
    )
    content_hash: str
    parameters: list[str]
    is_fully_bound: bool
    is_card: bool

    def __init__(
        self,
        prompt: Prompt,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
        metadata: PromptCardMetadata | None = ...,
        model_settings: OpenAISettings
        | OpenAIResponsesSettings
        | AnthropicSettings
        | GeminiSettings
        | Mapping[str, Any]
        | None = ...,
    ) -> None:
        """Create a local `PromptCard` from a `Prompt`.

        Args:
            prompt (Prompt): Live prompt builder to store in the PromptCard
                spec body.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `prompt`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated UID.
            labels (Mapping[str, str] | None): Queryable user labels copied
                into card metadata.
            annotations (Mapping[str, str] | None): Free-form user annotations
                copied into card metadata.
            metadata (PromptCardMetadata | None): Existing holder metadata to
                seed before the live prompt is captured.
            model_settings (OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | Mapping[str, Any] | None): Provider-native generation settings to apply to the captured prompt. Typed settings must match the prompt provider; mappings decode into the active provider shape.

        Raises:
            WyrdError: If labels, annotations, identity fields, or prompt
                metadata violate the PromptCard contract.
        """
        ...

    def save(self, path: PathLike) -> None:
        """Save this PromptCard envelope to a local JSON or YAML file.

        Args:
            path (PathLike): Target `.json`, `.yaml`, or `.yml` file path.

        Raises:
            WyrdError: If the path extension is unsupported, the card cannot be
                serialized, or filesystem IO fails.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> PromptCard:
        """Load a PromptCard envelope from a local JSON or YAML file.

        Args:
            path (PathLike): Source `.json`, `.yaml`, or `.yml` file path.

        Returns:
            PromptCard: Local holder rebuilt from the serialized envelope.

        Raises:
            WyrdError: If the file cannot be read, parsed, or validated as a
                PromptCard envelope.
        """
        ...

    @staticmethod
    def from_path(path: PathLike) -> PromptCard:
        """Load a PromptCard envelope from a local JSON or YAML file.

        Accepts both the stored native format and the declarative authoring
        format (when the spec has a ``provider`` key instead of ``request``).

        Args:
            path (PathLike): Source `.json`, `.yaml`, or `.yml` file path.

        Returns:
            PromptCard: Local holder rebuilt from the serialized envelope.

        Raises:
            WyrdError: If the file cannot be read, parsed, or validated as a
                PromptCard envelope.
        """
        ...

    def model_dump_json(self) -> str:
        """Return this PromptCard as a JSON envelope string.

        Returns:
            str: Serialized Wyrd card envelope.

        Raises:
            WyrdError: If the prompt or holder identity cannot be converted
                into a valid PromptCard envelope.
        """
        ...

    def as_card_ref(self) -> CardRef:
        """Return a CardRef pointing at this PromptCard.

        Returns:
            CardRef: Reference with kind `Kind.Prompt`.

        Raises:
            WyrdError: If holder identity fields are invalid.
        """
        ...

    @staticmethod
    def model_validate_json(json_string: str) -> PromptCard:
        """Build a PromptCard from serialized card-envelope JSON.

        Args:
            json_string (str): Serialized Wyrd PromptCard envelope.

        Returns:
            PromptCard: Local holder rebuilt from the JSON envelope.

        Raises:
            WyrdError: If the JSON is invalid or does not contain a
                `apiVersion: wyrd/v1`, `kind: Prompt` envelope.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the PromptCard envelope.
        """
        ...

__all__ = [
    "AnthropicSettings",
    "GeminiSettings",
    "MediaRef",
    "OpenAIResponsesSettings",
    "OpenAISettings",
    "Prompt",
    "PromptCard",
    "PromptCardMetadata",
    "PromptRef",
    "ProviderRequest",
    "ProviderResponse",
    "ResponseFormat",
    "WyrdError",
]
