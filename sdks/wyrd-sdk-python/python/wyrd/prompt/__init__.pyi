# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from collections.abc import Mapping
from typing import Any

from .._wyrd import JsonDict, PathLike, WyrdError
from ..cards import CardRef

#### end of imports ####

class MediaRef:
    """Image or document content to bind into a ``${media:name}`` placeholder.

    Build one with a static constructor and pass it to ``Prompt.bind_media()``.
    Text placeholders (``${name}`` or ``{{name}}``) and media placeholders
    (``${media:name}``) are separate namespaces.

    Provider support is checked at bind time:

    - OpenAI Chat: image URL, base64, and file id; document base64 and file
      id. Document URLs are rejected.
    - OpenAI Responses: image URL and base64; image or document file id.
      Document URLs and document base64 are rejected.
    - Anthropic: every kind and source.
    - Gemini and Vertex: base64 data, and file URIs with a ``mime_type``. A URL
      must be ``gs://`` or a ``https://generativelanguage.googleapis.com/v1/``
      or ``/v1beta/`` file API URL, with a ``mime_type``.

    Attributes:
        kind: ``"image"`` or ``"document"``.
        source_type: ``"url"``, ``"base64"``, or ``"file"``.

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
        """Reference an image by a URL the provider can fetch.

        Args:
            url: the image URL, or for Gemini and Vertex a ``gs://`` or Gemini
                file API URI.
            mime_type: the image MIME type. Gemini and Vertex require it;
                other providers ignore it.
        """
        ...

    @staticmethod
    def image_bytes(mime_type: str, data: bytes) -> MediaRef:
        """Reference an image from raw bytes, base64-encoded immediately.

        Args:
            mime_type: the image MIME type, such as ``image/png``.
            data: the raw image bytes; they are not retained.
        """
        ...

    @staticmethod
    def image_base64(mime_type: str, data: str) -> MediaRef:
        """Reference an image from base64 text.

        Args:
            mime_type: the image MIME type, such as ``image/png``.
            data: the base64 payload, without a ``data:`` prefix.
        """
        ...

    @staticmethod
    def image_file(uri: str, *, mime_type: str | None = ...) -> MediaRef:
        """Reference an image already uploaded to the provider.

        Args:
            uri: an OpenAI or Anthropic file id, or a Gemini or Vertex file URI.
            mime_type: the image MIME type. Gemini and Vertex require it.
        """
        ...

    @staticmethod
    def image_path(path: PathLike) -> MediaRef:
        """Read a local image file now and reference it as base64 data.

        The MIME type comes from the extension: ``png``, ``jpg``, ``jpeg``,
        ``gif``, or ``webp``.

        Args:
            path: a regular file (not a directory or symlink) of at most
                20 MiB.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE``,
                ``WYRD_PROMPT_400_MEDIA_TOO_LARGE``,
                ``WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION``, or
                ``WYRD_PROMPT_500_MEDIA_IO`` when the file cannot be read.
        """
        ...

    @staticmethod
    def document_url(url: str, *, mime_type: str | None = ...) -> MediaRef:
        """Reference a document by a URL the provider can fetch.

        OpenAI Chat and Responses reject document URLs when bound.

        Args:
            url: the document URL, or for Gemini and Vertex a ``gs://`` or
                Gemini file API URI.
            mime_type: the document MIME type. Gemini and Vertex require it.
        """
        ...

    @staticmethod
    def document_bytes(mime_type: str, data: bytes) -> MediaRef:
        """Reference a document from raw bytes, base64-encoded immediately.

        Args:
            mime_type: the document MIME type, such as ``application/pdf``.
            data: the raw document bytes; they are not retained.
        """
        ...

    @staticmethod
    def document_base64(mime_type: str, data: str) -> MediaRef:
        """Reference a document from base64 text.

        Args:
            mime_type: the document MIME type, such as ``application/pdf``.
            data: the base64 payload, without a ``data:`` prefix.
        """
        ...

    @staticmethod
    def document_file(uri: str, *, mime_type: str | None = ...) -> MediaRef:
        """Reference a document already uploaded to the provider.

        Args:
            uri: an OpenAI or Anthropic file id, or a Gemini or Vertex file URI.
            mime_type: the document MIME type. Gemini and Vertex require it.
        """
        ...

    @staticmethod
    def document_path(path: PathLike) -> MediaRef:
        """Read a local document file now and reference it as base64 data.

        The MIME type comes from the extension: ``pdf``, ``txt``, ``md``,
        ``json``, ``csv``, ``html``, or ``htm``. Size and file rules and
        errors are as for ``MediaRef.image_path()``.
        """
        ...

    def __repr__(self) -> str:
        """Show ``kind`` and ``source_type`` only, never the payload or URL."""
        ...

class ProviderRequest:
    """A provider-native request, as returned by ``Prompt.render()``.

    Use the accessor matching the provider for typed field access, or
    ``model_dump()`` for any request, including raw passthrough requests.

    Attributes:
        provider: the request dialect's default provider, such as
            ``"google"`` for a Vertex body; ``Prompt.provider`` names the
            dispatch destination.
        messages: the native messages or content turns as dictionaries; for
            OpenAI Responses, the input items. Empty for raw requests.
        message: the last entry of ``messages``, or ``None``.
        system: the native system field: the system messages for OpenAI Chat,
            ``instructions`` for OpenAI Responses, ``system`` for Anthropic,
            ``system_instruction`` for Gemini and Vertex; ``None`` when unset.
    """

    provider: str
    messages: list[JsonDict]
    message: JsonDict | None
    system: Any

    def model_dump(self) -> JsonDict:
        """Return the native provider request as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return the native provider request as a JSON string."""
        ...

    def openai(self) -> OpenAiChatRequest:
        """Return the typed OpenAI Chat Completions view.

        Also used for custom OpenAI-compatible providers.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` when this is not
                an OpenAI Chat request. The other accessors raise the same way.
        """
        ...

    def openai_responses(self) -> OpenAiResponsesRequest:
        """Return the typed OpenAI Responses view."""
        ...

    def anthropic(self) -> AnthropicMessagesRequest:
        """Return the typed Anthropic Messages view."""
        ...

    def gemini(self) -> GeminiRequest:
        """Return the typed Gemini GenerateContent view."""
        ...

    def __str__(self) -> str:
        """Return the request as pretty-printed JSON."""
        ...

class ProviderResponse:
    """A provider-native response with typed per-provider views."""

    @staticmethod
    def text(text: str) -> ProviderResponse:
        """Build a finished OpenAI Chat response whose one assistant message is ``text``.

        Return it from an ``after_model_callback`` to replace the model's
        answer. It is the deterministic shape the ``mock`` provider returns:
        id ``mock_response``, model ``mock-model``, finish reason ``stop``,
        and zero token usage.
        """
        ...

    @property
    def provider(self) -> str:
        """The response dialect's default provider, such as ``"google"`` for a Vertex body.

        OpenAI Chat and OpenAI Responses both report ``"openai"``.
        ``Prompt.provider`` names the dispatch destination.
        """
        ...
    def openai(self) -> OpenAiChatResponse:
        """Return the typed OpenAI Chat Completions view.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` when the response
                is from another API. The other accessors raise the same way.
        """
        ...
    def openai_responses(self) -> OpenAiResponsesResponse:
        """Return the typed OpenAI Responses view."""
        ...
    def anthropic(self) -> AnthropicMessagesResponse:
        """Return the typed Anthropic Messages view."""
        ...
    def gemini(self) -> GeminiResponse:
        """Return the typed Gemini GenerateContent view."""
        ...
    def model_dump(self) -> JsonDict:
        """Return the native provider response as a dictionary."""
        ...
    def model_dump_json(self) -> str:
        """Return the native provider response as a JSON string."""
        ...

class OpenAiChatRequest:
    """Read-only view of an OpenAI Chat Completions request."""

    @property
    def model(self) -> str:
        """The ``model`` field."""
        ...
    @property
    def messages(self) -> list[OpenAiChatMessage]:
        """The ``messages`` list, in order."""
        ...
    @property
    def stream(self) -> bool | None:
        """The ``stream`` flag, or ``None`` when unset."""
        ...
    @property
    def parallel_tool_calls(self) -> bool | None:
        """The ``parallel_tool_calls`` flag, or ``None`` when unset."""
        ...

class OpenAiChatResponse:
    """Read-only view of an OpenAI Chat Completions response."""

    @property
    def id(self) -> str:
        """The completion ``id``."""
        ...
    @property
    def object(self) -> str:
        """The ``object`` type, ``"chat.completion"`` from OpenAI."""
        ...
    @property
    def created(self) -> int:
        """The ``created`` Unix timestamp in seconds."""
        ...
    @property
    def model(self) -> str:
        """The ``model`` that produced the completion."""
        ...
    @property
    def system_fingerprint(self) -> str | None:
        """The ``system_fingerprint`` backend configuration id."""
        ...
    @property
    def service_tier(self) -> str | None:
        """The ``service_tier`` that served the request."""
        ...
    @property
    def usage(self) -> OpenAiUsage | None:
        """The token ``usage``, or ``None`` when the provider omitted it."""
        ...
    @property
    def choices(self) -> list[OpenAiChatChoice]:
        """The ``choices`` list."""
        ...

class OpenAiChatChoice:
    """One entry of an OpenAI Chat response's ``choices``."""

    @property
    def index(self) -> int:
        """The choice ``index``."""
        ...
    @property
    def finish_reason(self) -> str | None:
        """The ``finish_reason``, such as ``"stop"``, ``"length"``, or ``"tool_calls"``."""
        ...
    @property
    def message(self) -> OpenAiChatMessage:
        """The generated ``message``."""
        ...
    @property
    def logprobs(self) -> OpenAiChatLogprobs | None:
        """The ``logprobs``, or ``None`` when not requested."""
        ...

class OpenAiChatMessage:
    """One OpenAI Chat message, from a request's ``messages`` or a response choice."""

    @property
    def role(self) -> str:
        """The ``role``: ``"system"``, ``"user"``, ``"assistant"``, ``"tool"``, or another."""
        ...
    @property
    def content(self) -> OpenAiMessageContent | None:
        """The ``content``, or ``None`` when absent."""
        ...
    @property
    def name(self) -> str | None:
        """The participant ``name``."""
        ...
    @property
    def tool_calls(self) -> list[OpenAiToolCall] | None:
        """The assistant ``tool_calls``, or ``None`` when absent."""
        ...
    @property
    def tool_call_id(self) -> str | None:
        """The ``tool_call_id`` a ``tool`` message answers."""
        ...
    @property
    def refusal(self) -> str | None:
        """The assistant ``refusal`` text."""
        ...
    @property
    def annotations(self) -> list[OpenAiMessageAnnotation]:
        """The ``annotations`` list, empty when absent."""
        ...
    @property
    def audio(self) -> OpenAiMessageAudio | None:
        """The assistant ``audio`` output, or ``None`` when absent."""
        ...

class OpenAiMessageContent:
    """OpenAI Chat message content: either a string or a list of parts."""

    @property
    def kind(self) -> str:
        """``"text"`` or ``"parts"``."""
        ...
    def as_text(self) -> str:
        """Return the string content.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` when ``kind`` is
                not ``"text"``. Every ``as_*`` accessor in this module raises
                the same way for the wrong variant.
        """
        ...
    def as_parts(self) -> list[OpenAiContentPart]:
        """Return the content parts when ``kind`` is ``"parts"``."""
        ...

class OpenAiContentPart:
    """One part of OpenAI Chat message content."""

    @property
    def kind(self) -> str:
        """``"text"``, ``"image_url"``, ``"input_audio"``, or ``"file"``."""
        ...
    def as_text(self) -> str:
        """Return the ``text`` of a ``"text"`` part."""
        ...
    def as_image_url(self) -> OpenAiImageUrl:
        """Return the ``image_url`` of an ``"image_url"`` part."""
        ...
    def as_input_audio(self) -> OpenAiInputAudio:
        """Return the ``input_audio`` of an ``"input_audio"`` part."""
        ...
    def as_file(self) -> OpenAiFilePart:
        """Return the ``file`` of a ``"file"`` part."""
        ...

class OpenAiImageUrl:
    """The ``image_url`` object of an OpenAI Chat image part."""

    @property
    def url(self) -> str:
        """The image ``url``, possibly a ``data:`` URL."""
        ...
    @property
    def detail(self) -> str | None:
        """The ``detail`` level, such as ``"low"``, ``"high"``, or ``"auto"``."""
        ...

class OpenAiInputAudio:
    """The ``input_audio`` object of an OpenAI Chat audio part."""

    @property
    def data(self) -> str:
        """The base64 audio ``data``."""
        ...
    @property
    def format(self) -> str:
        """The audio ``format``, such as ``"wav"`` or ``"mp3"``."""
        ...

class OpenAiFilePart:
    """The ``file`` object of an OpenAI Chat file part."""

    @property
    def file_id(self) -> str | None:
        """The uploaded ``file_id``."""
        ...
    @property
    def file_data(self) -> str | None:
        """The inline ``file_data``, a base64 ``data:`` URL."""
        ...
    @property
    def filename(self) -> str | None:
        """The ``filename``."""
        ...

class OpenAiToolCall:
    """One entry of an OpenAI Chat assistant message's ``tool_calls``."""

    @property
    def id(self) -> str:
        """The tool call ``id``; a ``tool`` message answers it by this id."""
        ...
    @property
    def kind(self) -> str:
        """The call ``type``, such as ``"function"``."""
        ...
    @property
    def function(self) -> OpenAiToolFunctionCall:
        """The called ``function``."""
        ...

class OpenAiToolFunctionCall:
    """The ``function`` object of an OpenAI Chat tool call."""

    @property
    def name(self) -> str:
        """The function ``name``."""
        ...
    @property
    def arguments(self) -> str:
        """The ``arguments`` as the model's JSON string, not parsed."""
        ...

class OpenAiMessageAnnotation:
    """One entry of an OpenAI Chat message's ``annotations``."""

    @property
    def kind(self) -> str:
        """The annotation ``type``, such as ``"url_citation"``."""
        ...
    @property
    def url_citation(self) -> OpenAiUrlCitation:
        """The ``url_citation`` object."""
        ...

class OpenAiUrlCitation:
    """A web citation inside an OpenAI Chat message."""

    @property
    def url(self) -> str:
        """The cited ``url``."""
        ...
    @property
    def title(self) -> str:
        """The cited page ``title``."""
        ...
    @property
    def start_index(self) -> int:
        """The ``start_index`` of the cited span in the message content."""
        ...
    @property
    def end_index(self) -> int:
        """The ``end_index`` of the cited span in the message content."""
        ...

class OpenAiMessageAudio:
    """The ``audio`` output of an OpenAI Chat assistant message."""

    @property
    def id(self) -> str:
        """The audio response ``id``."""
        ...
    @property
    def expires_at(self) -> int:
        """The ``expires_at`` Unix timestamp after which the audio is unavailable."""
        ...
    @property
    def data(self) -> str:
        """The base64 audio ``data``."""
        ...
    @property
    def transcript(self) -> str:
        """The audio ``transcript``."""
        ...

class OpenAiUsage:
    """The ``usage`` object of an OpenAI Chat response."""

    @property
    def prompt_tokens(self) -> int:
        """The ``prompt_tokens`` count."""
        ...
    @property
    def completion_tokens(self) -> int:
        """The ``completion_tokens`` count."""
        ...
    @property
    def total_tokens(self) -> int:
        """The ``total_tokens`` count."""
        ...
    @property
    def prompt_tokens_details(self) -> OpenAiPromptTokensDetails | None:
        """The ``prompt_tokens_details`` breakdown, when present."""
        ...
    @property
    def completion_tokens_details(self) -> OpenAiCompletionTokensDetails | None:
        """The ``completion_tokens_details`` breakdown, when present."""
        ...

class OpenAiPromptTokensDetails:
    """The ``prompt_tokens_details`` of an OpenAI Chat usage object."""

    @property
    def audio_tokens(self) -> int:
        """The ``audio_tokens`` count."""
        ...
    @property
    def cached_tokens(self) -> int:
        """The ``cached_tokens`` count served from the prompt cache."""
        ...

class OpenAiCompletionTokensDetails:
    """The ``completion_tokens_details`` of an OpenAI Chat usage object."""

    @property
    def accepted_prediction_tokens(self) -> int:
        """The ``accepted_prediction_tokens`` count."""
        ...
    @property
    def audio_tokens(self) -> int:
        """The ``audio_tokens`` count."""
        ...
    @property
    def reasoning_tokens(self) -> int:
        """The ``reasoning_tokens`` count."""
        ...
    @property
    def rejected_prediction_tokens(self) -> int:
        """The ``rejected_prediction_tokens`` count."""
        ...

class OpenAiChatLogprobs:
    """The ``logprobs`` of an OpenAI Chat choice."""

    @property
    def content(self) -> list[Any]:
        """The per-token ``content`` entries as dictionaries; empty when absent."""
        ...
    @property
    def refusal(self) -> list[Any]:
        """The per-token ``refusal`` entries as dictionaries; empty when absent."""
        ...

class AnthropicMessagesRequest:
    """Read-only view of an Anthropic Messages request."""

    @property
    def model(self) -> str:
        """The ``model`` field."""
        ...

class AnthropicMessagesResponse:
    """Read-only view of an Anthropic Messages response."""

    @property
    def id(self) -> str:
        """The message ``id``."""
        ...
    @property
    def model(self) -> str:
        """The ``model`` that produced the message."""
        ...
    @property
    def role(self) -> str:
        """The message ``role``, ``"assistant"`` from Anthropic."""
        ...
    @property
    def stop_reason(self) -> str | None:
        """The ``stop_reason``.

        One of ``"end_turn"``, ``"max_tokens"``, ``"stop_sequence"``,
        ``"tool_use"``, ``"pause_turn"``, or ``"refusal"``.
        """
        ...
    @property
    def stop_sequence(self) -> str | None:
        """The ``stop_sequence`` that ended generation, if any."""
        ...
    @property
    def usage(self) -> AnthropicUsage:
        """The token ``usage``."""
        ...

class AnthropicMessage:
    """One entry of an Anthropic Messages request's ``messages``."""

    @property
    def role(self) -> str:
        """The ``role``, ``"user"`` or ``"assistant"``."""
        ...
    @property
    def content(self) -> list[AnthropicContentBlock]:
        """The ``content`` blocks."""
        ...

class AnthropicContentBlock:
    """One content block of an Anthropic message."""

    @property
    def kind(self) -> str:
        """The block ``type``.

        One of ``"text"``, ``"image"``, ``"document"``, ``"thinking"``,
        ``"redacted_thinking"``, ``"tool_use"``, or ``"tool_result"``.
        """
        ...
    def as_text(self) -> str:
        """Return the ``text`` of a ``"text"`` block.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` for another kind.
        """
        ...

class AnthropicUsage:
    """The ``usage`` object of an Anthropic Messages response."""

    @property
    def input_tokens(self) -> int:
        """The ``input_tokens`` count."""
        ...
    @property
    def output_tokens(self) -> int:
        """The ``output_tokens`` count."""
        ...
    @property
    def cache_creation_input_tokens(self) -> int:
        """The ``cache_creation_input_tokens`` count written to the prompt cache."""
        ...
    @property
    def cache_read_input_tokens(self) -> int:
        """The ``cache_read_input_tokens`` count read from the prompt cache."""
        ...

class GeminiRequest:
    """Read-only view of a Gemini GenerateContent request."""

    @property
    def contents(self) -> list[GoogleContent]:
        """The ``contents`` turns, in order."""
        ...

class GeminiResponse:
    """Read-only view of a Gemini GenerateContent response."""

    @property
    def candidates(self) -> list[GoogleCandidate]:
        """The ``candidates`` list."""
        ...
    @property
    def usage_metadata(self) -> GoogleUsageMetadata | None:
        """The ``usageMetadata`` token counts, or ``None`` when omitted."""
        ...

class GoogleContent:
    """One Gemini or Vertex content turn."""

    @property
    def role(self) -> str:
        """The turn ``role``, such as ``"user"``, ``"model"``, or ``"function"``."""
        ...
    @property
    def parts(self) -> list[GooglePart]:
        """The turn ``parts``."""
        ...

class GooglePart:
    """One part of a Gemini or Vertex content turn."""

    @property
    def kind(self) -> str:
        """The part variant.

        One of ``"text"``, ``"inline_data"``, ``"file_data"``,
        ``"function_call"``, ``"function_response"``, ``"thought"``,
        ``"executable_code"``, or ``"code_execution_result"``.
        """
        ...
    def as_text(self) -> str:
        """Return the ``text`` of a ``"text"`` part.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` for another kind.
        """
        ...

class GoogleCandidate:
    """One Gemini or Vertex response candidate."""

    @property
    def finish_reason(self) -> str | None:
        """The ``finishReason``, lowercased.

        One of ``"stop"``, ``"max_tokens"``, ``"safety"``, ``"recitation"``,
        ``"language"``, ``"other"``, ``"blocklist"``, ``"prohibited_content"``,
        ``"spii"``, ``"malformed_function_call"``, or ``"unspecified"``.
        """
        ...
    @property
    def index(self) -> int | None:
        """The candidate ``index``."""
        ...

class GoogleUsageMetadata:
    """The ``usageMetadata`` of a Gemini or Vertex response."""

    @property
    def prompt_token_count(self) -> int:
        """The ``promptTokenCount``."""
        ...
    @property
    def candidates_token_count(self) -> int:
        """The ``candidatesTokenCount`` across all candidates."""
        ...
    @property
    def total_token_count(self) -> int:
        """The ``totalTokenCount``."""
        ...

class OpenAiResponsesRequest:
    """Read-only view of an OpenAI Responses request."""

    @property
    def model(self) -> str:
        """The ``model`` field."""
        ...
    @property
    def instructions(self) -> str | None:
        """The ``instructions`` system text, or ``None`` when unset."""
        ...

class OpenAiResponsesResponse:
    """Read-only view of an OpenAI Responses response."""

    @property
    def id(self) -> str:
        """The response ``id``."""
        ...
    @property
    def model(self) -> str:
        """The ``model`` that produced the response."""
        ...
    @property
    def status(self) -> str:
        """The response ``status``, such as ``"completed"`` or ``"incomplete"``."""
        ...
    @property
    def usage(self) -> OpenAiResponsesUsage | None:
        """The token ``usage``, or ``None`` when omitted."""
        ...

class OpenAiResponsesUsage:
    """The ``usage`` object of an OpenAI Responses response."""

    @property
    def input_tokens(self) -> int:
        """The ``input_tokens`` count."""
        ...
    @property
    def output_tokens(self) -> int:
        """The ``output_tokens`` count."""
        ...
    @property
    def total_tokens(self) -> int:
        """The ``total_tokens`` count."""
        ...

class ResponseFormat:
    """A provider-independent output format, emitted into each provider's native field.

    OpenAI Chat writes ``response_format``, OpenAI Responses writes
    ``text.format``, Anthropic writes ``output_config`` (JSON Schema only),
    and Gemini and Vertex write ``generation_config.response_mime_type`` and
    ``response_schema``.
    """

    @staticmethod
    def text() -> ResponseFormat:
        """Request plain-text output."""
        ...

    @staticmethod
    def json_object() -> ResponseFormat:
        """Request a JSON object without a schema.

        Anthropic has no such mode and sends no output constraint.
        """
        ...

    @staticmethod
    def json_schema(name: str, schema: JsonDict) -> ResponseFormat:
        """Request JSON output that matches a JSON Schema.

        Args:
            name: the schema name, sent to providers that require one.
            schema: a JSON Schema object, or a Pydantic model class whose
                ``model_json_schema()`` is used.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA`` when the
                schema is not a JSON object.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return ``{"type": ...}`` plus ``name`` and ``schema`` for JSON Schema."""
        ...

    def __str__(self) -> str:
        """Return ``to_dict()`` as pretty-printed JSON."""
        ...

class OpenAISettings:
    """OpenAI Chat Completions generation settings.

    Every argument is optional and maps to the same-named top-level request
    field; an omitted argument is left out of the request so the provider
    default applies. Unknown keyword arguments are kept and sent as extra
    top-level fields.

    Args:
        temperature: sampling temperature.
        top_p: nucleus sampling probability.
        max_tokens: legacy output-token cap.
        max_completion_tokens: output-token cap, including reasoning tokens.
        n: number of choices to generate.
        stop: one stop sequence or a list of them.
        presence_penalty: presence penalty.
        frequency_penalty: frequency penalty.
        seed: best-effort deterministic sampling seed.
        logit_bias: token id to bias value.
        user: end-user identifier.
        reasoning_effort: ``"none"``, ``"minimal"``, ``"low"``, ``"medium"``,
            ``"high"``, or ``"xhigh"``.
        modalities: output modalities, each ``"text"`` or ``"audio"``.
        audio: audio output settings, ``{"voice": ..., "format": ...}``.
        prediction: predicted output content.
        prompt_cache_key: prompt cache key. Takes precedence over the
            ``cache`` argument of ``Prompt()``.
        service_tier: service tier.
        safety_identifier: end-user safety identifier.
        store: whether OpenAI may store the completion.
        metadata: request metadata object.
        logprobs: whether to return token log probabilities.
        top_logprobs: number of most likely tokens to return per position.
        **extra: other Chat Completions fields, sent unchanged.

    Raises:
        WyrdError: ``WYRD_PROMPT_400_SETTINGS_DECODE`` when a value does not
            fit its field.
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
        """Create OpenAI Chat settings; see the class docstring for arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> OpenAISettings:
        """Create settings from a mapping.

        Args:
            value: the same keys the constructor accepts; unknown keys are
                kept as extra fields.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_SETTINGS_DECODE`` when a value does
                not fit its field.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return the set fields and extra fields as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return ``to_dict()`` as a JSON string."""
        ...

    def __repr__(self) -> str:
        """Return the class name with the settings JSON."""
        ...

class OpenAIResponsesSettings:
    """OpenAI Responses generation settings.

    Arguments, omission, and unknown keyword arguments behave as for
    ``OpenAISettings``.

    Args:
        temperature: sampling temperature.
        top_p: nucleus sampling probability.
        max_output_tokens: output-token cap, including reasoning tokens.
        reasoning: reasoning configuration, such as ``{"effort": "low"}``.
        store: whether OpenAI may store the response.
        include: additional output data to include in the response.
        metadata: request metadata object.
        **extra: other Responses fields, sent unchanged.

    Raises:
        WyrdError: ``WYRD_PROMPT_400_SETTINGS_DECODE`` when a value does not
            fit its field.
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
        """Create OpenAI Responses settings; see the class docstring for arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> OpenAIResponsesSettings:
        """As ``OpenAISettings.from_dict()``."""
        ...

    def to_dict(self) -> JsonDict:
        """Return the set fields and extra fields as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return ``to_dict()`` as a JSON string."""
        ...

    def __repr__(self) -> str:
        """Return the class name with the settings JSON."""
        ...

class AnthropicSettings:
    """Anthropic Messages generation settings.

    Arguments map to the same-named top-level request fields. Unknown keyword
    arguments are kept and sent as extra top-level fields. A prompt built
    without ``model_settings`` sends ``max_tokens`` 4096.

    Args:
        max_tokens: maximum output tokens.
        temperature: sampling temperature.
        top_p: nucleus sampling probability.
        top_k: top-k sampling limit.
        stop_sequences: custom stop sequences.
        metadata: request metadata object.
        thinking: extended thinking configuration, such as
            ``{"type": "enabled", "budget_tokens": 2048}``.
        **extra: other Messages fields, sent unchanged.

    Raises:
        WyrdError: ``WYRD_PROMPT_400_SETTINGS_DECODE`` when a value does not
            fit its field.
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
        """Create Anthropic settings; see the class docstring for arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> AnthropicSettings:
        """As ``OpenAISettings.from_dict()``."""
        ...

    def to_dict(self) -> JsonDict:
        """Return the set fields and extra fields as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return ``to_dict()`` as a JSON string."""
        ...

    def __repr__(self) -> str:
        """Return the class name with the settings JSON."""
        ...

class GeminiSettings:
    """Gemini and Vertex GenerateContent settings.

    Sampling knobs such as ``temperature``, ``top_p``, ``top_k``,
    ``max_output_tokens``, and ``thinking_config`` go inside
    ``generation_config``. Omitted arguments are left out of the request.
    Unknown keyword arguments are kept and sent as extra top-level fields.

    Args:
        generation_config: the native generation config object.
        safety_settings: the native safety settings, each with a
            ``category`` and ``threshold``.
        cached_content: the cached content resource name to reuse.
        labels: request labels object.
        **extra: other GenerateContent fields, sent unchanged.

    Raises:
        WyrdError: ``WYRD_PROMPT_400_SETTINGS_DECODE`` when a value does not
            fit its field.
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
        """Create Gemini or Vertex settings; see the class docstring for arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> GeminiSettings:
        """As ``OpenAISettings.from_dict()``."""
        ...

    def to_dict(self) -> JsonDict:
        """Return the set fields and extra fields as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return ``to_dict()`` as a JSON string."""
        ...

    def __repr__(self) -> str:
        """Return the class name with the settings JSON."""
        ...

class Prompt:
    """A prompt held as one provider-native request plus its variables.

    Text variables are ``${name}`` or ``{{name}}`` placeholders, bound with
    ``bind()`` or at ``render()``. Media variables are ``${media:name}``
    placeholders, bound with ``bind_media()``. Builder methods such as
    ``user()`` and ``bind()`` return a new prompt; ``*_mut`` methods change
    this one.

    Attributes:
        provider: ``"openai"``, ``"anthropic"``, ``"google"``, ``"vertex"``,
            or a custom provider name. Gemini prompts report ``"google"``.
        request: the native request. Assigning a ``ProviderRequest`` or an
            equivalent dictionary replaces it and adopts its model.
        messages: as ``ProviderRequest.messages``.
        message: as ``ProviderRequest.message``.
        system_messages: as ``ProviderRequest.system``.
        model: the model name. Assigning also updates the request's model and
            raises ``WYRD_PROMPT_400_EMPTY_MODEL`` for a blank name.
        version: an optional free-form prompt version.
        variables: the text variables still unbound.
        media_variables: the media placeholders still unbound.
        model_settings: a copy of the typed generation settings, or ``None``
            for a raw prompt.

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

    @property
    def response_schema(self) -> JsonDict | None:
        """The declared structured-output JSON Schema, whatever the provider body shape.

        `None` when the prompt declares no JSON-schema response format.
        """
        ...

    @property
    def response_schema_name(self) -> str | None:
        """The declared structured-output schema name, when one was given."""
        ...

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
        """Build a prompt for any provider from one call.

        Args:
            messages: the user turns. A string is one user message; a list or
                tuple adds one user message per item; any other value is one
                user message. A non-string item is a provider-native content
                part or list of parts, such as ``Prompt.image_url()`` returns.
                For a custom provider every item must be a string.
            model: the provider model name; must not be blank.
            provider: ``"openai"``, ``"anthropic"``, ``"gemini"`` or
                ``"google"``, ``"vertex"``, case-insensitive (``"open_ai"``
                and ``"vertex_ai"`` also work). Any other name builds an
                OpenAI-compatible Chat request for that custom provider.
            system: the system instruction. OpenAI Responses stores it as
                ``instructions``. Omitted, no system instruction is sent.
            response_format: a ``ResponseFormat``, or a JSON Schema object or
                Pydantic model class used as a schema named ``"response"``.
                Omitted, the provider's default text output applies.
            output: a structured-output declaration that replaces
                ``response_format`` when both are set. A Pydantic model class
                becomes a schema named after the class and is kept for decoding
                results; a dictionary with a ``type``, ``properties``, or
                ``$schema`` key is a raw JSON Schema; any other dictionary maps
                required field names to Python types. Object schemas default to
                ``additionalProperties: false``.
            operation: with OpenAI, ``"responses"`` builds an OpenAI Responses
                request instead of Chat. Ignored otherwise.
            cache: OpenAI Chat only: the ``prompt_cache_key``, given as a
                string or as a mapping with a ``prompt_cache_key`` or ``key``
                item. A ``prompt_cache_key`` in ``model_settings`` wins.
                Ignored for other providers.
            model_settings: generation settings for this provider, as the
                matching settings class or a mapping of its fields. Omitted,
                provider defaults apply (Anthropic sends ``max_tokens`` 4096).
            variables: the text variable names to require at render time.
                Omitted, every ``${name}`` and ``{{name}}`` placeholder found
                in the request is used.
            version: an optional free-form version label stored on the prompt.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_EMPTY_MODEL`` for a blank model;
                ``WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA`` for a schema that
                is not a JSON object; ``WYRD_PROMPT_400_DRAFT_INVALID`` for an
                unsupported ``output`` value;
                ``WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH`` for settings of
                another provider; ``WYRD_PROMPT_400_SETTINGS_DECODE`` for a
                settings mapping that does not decode;
                ``WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE`` for a media
                placeholder in the system instruction.
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
        """Build an OpenAI Chat Completions prompt.

        Arguments and errors are as for ``Prompt()``, except ``messages``: a
        string or an iterable of strings, each one user message. ``system``
        becomes the first message.
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

        As ``Prompt.openai_chat()``; ``instructions`` is the Responses system
        text, and each message becomes a user input item.
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

        As ``Prompt.openai_chat()``; ``system`` becomes the request's
        ``system`` field.
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

        As ``Prompt.openai_chat()``; ``system`` becomes
        ``system_instruction``.
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
        """Build a Vertex GenerateContent prompt. As ``Prompt.gemini()``."""
        ...

    @staticmethod
    def raw(provider: str, model: str, body: bytes) -> Prompt:
        """Wrap a provider request body Wyrd does not model.

        A raw prompt has no variables, no typed settings, and no typed request
        view; builder methods such as ``user()`` raise
        ``WYRD_PROMPT_400_PROVIDER_MISMATCH``.

        Args:
            provider: the provider name, spelled as for ``Prompt()``.
            model: the model name; must not be blank.
            body: the request body as UTF-8 JSON bytes, stored unchanged.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_EMPTY_MODEL`` for a blank model, or
                ``WYRD_PROMPT_500_SERIALIZE_REQUEST`` when ``body`` is not
                valid UTF-8 JSON.
        """
        ...

    def system(self, text: str) -> Prompt:
        """Return a copy with a system instruction.

        OpenAI Chat inserts a new first system message; the other providers
        replace their system field (``instructions`` for OpenAI Responses).

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` for a raw prompt,
                or ``WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE`` when ``text``
                holds a media placeholder.
        """
        ...

    def user(self, content: Any) -> Prompt:
        """Return a copy with one user message appended.

        Args:
            content: a string, or a provider-native content part or list of
                parts, such as ``Prompt.image_url()`` returns.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` for a raw prompt.
        """
        ...

    def assistant(self, content: Any) -> Prompt:
        """As ``Prompt.user()``, appending an assistant (Gemini: ``model``) message."""
        ...

    def tool_result(self, tool_use_id: str, content: str, is_error: bool = ...) -> Prompt:
        """Return a copy with a tool result appended in the provider's native form.

        Args:
            tool_use_id: the tool call being answered: the call id for OpenAI
                and Anthropic, the function name for Gemini and Vertex.
            content: the tool output text.
            is_error: mark the result as an error. Only Anthropic sends it.
                Defaults to ``False``.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` for a raw prompt.
        """
        ...

    def render(self, **kwargs: str) -> ProviderRequest:
        """Substitute text variables and return the provider request.

        The prompt itself is unchanged. Values are inserted as JSON string
        content, so they cannot add fields to the request.

        Args:
            **kwargs: a value for every name in ``variables``; non-string
                values are converted with ``str()``.

        Raises:
            WyrdError: ``WYRD_PROMPT_422_MISSING_VARIABLE`` when a declared
                variable has no value, or
                ``WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE`` when a media
                placeholder is still unbound.
        """
        ...

    def bind(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> Prompt:
        """Return a copy with some text variables substituted.

        Unlike ``render()``, binding a subset is allowed; bound names leave
        ``variables``.

        Args:
            name: one variable name, used together with ``value``.
            value: the value for ``name``, converted with ``str()``.
            **kwargs: more variable values, converted with ``str()``.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_DRAFT_INVALID`` when no binding is
                given.
        """
        ...

    def bind_mut(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> None:
        """As ``Prompt.bind()``, changing this prompt in place."""
        ...

    def bind_media(self, name: str, media: MediaRef) -> Prompt:
        """Return a copy with a media placeholder replaced by native media content.

        Args:
            name: the placeholder name, without the ``${media:...}`` wrapper.
            media: the media to insert.

        Raises:
            WyrdError: ``WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER`` when
                no ``${media:name}`` part exists;
                ``WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED`` when the
                placeholder shares its text part;
                ``WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER`` for a
                combination the provider rejects (see ``MediaRef``);
                ``WYRD_PROMPT_400_INVALID_MEDIA_TYPE`` for a missing or empty
                MIME type the provider needs.
        """
        ...

    def bind_media_mut(self, name: str, media: MediaRef) -> None:
        """As ``Prompt.bind_media()``, changing this prompt in place."""
        ...

    def model_dump(self) -> JsonDict:
        """Return the native prompt as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return the native prompt as a JSON string."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> Prompt:
        """Rebuild a prompt from ``model_dump_json()`` output.

        Raises:
            WyrdError: when ``data`` is not a valid native prompt.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> Prompt:
        """Load a prompt from a Prompt Card ``spec`` body file.

        Both the stored native form and the declarative authoring form (a
        ``provider`` key instead of ``request``) are accepted.

        Args:
            path: a ``.json``, ``.yaml``, or ``.yml`` file.

        Raises:
            WyrdError: ``WYRD_PROMPT_500_LOADER_IO`` when the file cannot be
                read, ``WYRD_PROMPT_400_LOADER_BAD_EXTENSION`` for another
                extension, or a validation error for invalid content.
        """
        ...

    def dump(self, path: PathLike) -> None:
        """Write this prompt as a Prompt Card ``spec`` body file.

        Args:
            path: a ``.json``, ``.yaml``, or ``.yml`` file, written in that
                format.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_LOADER_BAD_EXTENSION``,
                ``WYRD_PROMPT_500_LOADER_IO``, or a validation error when the
                prompt is not a valid Prompt Card body.
        """
        ...

    @staticmethod
    def openai_image_url(url: str, detail: str | None = ...) -> JsonDict:
        """Return an OpenAI Chat ``image_url`` content part.

        Args:
            url: the image URL or ``data:`` URL.
            detail: ``"low"``, ``"high"``, or ``"auto"``. Omitted, OpenAI
                chooses.
        """
        ...

    @staticmethod
    def openai_audio(data: str, format: str) -> JsonDict:
        """Return an OpenAI Chat ``input_audio`` content part.

        Args:
            data: the base64 audio payload.
            format: the audio format, such as ``"wav"`` or ``"mp3"``.
        """
        ...

    @staticmethod
    def openai_file_data(file_data: str, filename: str | None = None) -> JsonDict:
        """Return an OpenAI Chat inline ``file`` content part.

        Args:
            file_data: the base64 file payload or ``data:`` URL.
            filename: the file name. Omitted, none is sent.
        """
        ...

    @staticmethod
    def openai_file_id(file_id: str) -> JsonDict:
        """Return an OpenAI Chat ``file`` content part for an uploaded file id."""
        ...

    @staticmethod
    def anthropic_image_url(url: str) -> JsonDict:
        """Return an Anthropic URL-sourced ``image`` content block."""
        ...

    @staticmethod
    def anthropic_image_file_id(file_id: str) -> JsonDict:
        """Return an Anthropic ``image`` content block for an uploaded file id."""
        ...

    @staticmethod
    def anthropic_document_text(media_type: str, data: str, title: str | None = None) -> JsonDict:
        """Return an Anthropic plain-text ``document`` block. As ``Prompt.document_text()``."""
        ...

    @staticmethod
    def anthropic_document_file_id(file_id: str, title: str | None = None) -> JsonDict:
        """Return an Anthropic ``document`` block for an uploaded file id.

        Args:
            file_id: the uploaded file id.
            title: the document title. Omitted, none is sent.
        """
        ...

    @staticmethod
    def google_inline_data(mime_type: str, data: str) -> JsonDict:
        """Return a Gemini or Vertex ``inline_data`` part.

        Args:
            mime_type: the payload MIME type.
            data: the base64 payload.
        """
        ...

    @staticmethod
    def anthropic_image_base64(media_type: str, data: str) -> JsonDict:
        """Return an Anthropic base64 ``image`` content block.

        Args:
            media_type: the image MIME type, such as ``image/png``.
            data: the base64 image payload.
        """
        ...

    @staticmethod
    def google_file_data(mime_type: str, file_uri: str) -> JsonDict:
        """Return a Gemini or Vertex ``file_data`` part.

        Args:
            mime_type: the file MIME type.
            file_uri: a ``gs://`` or Gemini file API URI.
        """
        ...

    @staticmethod
    def image_url(url: str, *, detail: str | None = ..., provider: str = ...) -> JsonDict:
        """Return an image-URL content value for OpenAI Chat or Anthropic.

        Args:
            url: the image URL.
            detail: the OpenAI detail level; Anthropic ignores it.
            provider: ``"openai"`` (the default) or ``"anthropic"``.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_PROVIDER_MISMATCH`` for any other
                provider.
        """
        ...

    @staticmethod
    def image_base64(media_type: str, data: str) -> JsonDict:
        """Return an Anthropic base64 ``image`` block. As ``Prompt.anthropic_image_base64()``."""
        ...

    @staticmethod
    def file_uri(mime_type: str, file_uri: str) -> JsonDict:
        """Return a Gemini or Vertex ``file_data`` part. As ``Prompt.google_file_data()``."""
        ...

    @staticmethod
    def file_id(file_id: str) -> JsonDict:
        """Return an OpenAI Chat ``file`` content part for an uploaded file id."""
        ...

    @staticmethod
    def document_text(media_type: str, data: str, title: str | None = ...) -> JsonDict:
        """Return an Anthropic plain-text ``document`` block.

        Args:
            media_type: the document MIME type, such as ``text/plain``.
            data: the document text.
            title: the document title. Omitted, none is sent.
        """
        ...

    def __str__(self) -> str:
        """Return the native prompt as pretty-printed JSON."""
        ...

class PromptReference:
    """An Agent's prompt slot: a registered Prompt Card or an inline ``Prompt``.

    Attributes:
        kind: ``"card"`` or ``"inline"`` for references built here;
            ``"sibling"`` or ``"path"`` for ones loaded from JSON.
        card_ref: the Prompt Card reference when ``kind`` is ``"card"``,
            otherwise ``None``.
        prompt: a copy of the inline prompt when ``kind`` is ``"inline"``,
            otherwise ``None``.
    """

    kind: str
    card_ref: CardRef | None
    prompt: Prompt | None

    @staticmethod
    def card(name: str, version: str, *, space: str, uid: str | None = ...) -> PromptReference:
        """Reference a registered Prompt Card.

        Args:
            name: the card name.
            version: the card version or version requirement, such as
                ``"1.2.3"``.
            space: the card space; must not be empty.
            uid: the exact card UID. Omitted, the reference is resolved by
                space, name, and version.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when a field is invalid.
        """
        ...

    @staticmethod
    def inline(prompt: Prompt) -> PromptReference:
        """Embed a copy of ``prompt``.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` when ``prompt`` is not a
                ``Prompt``.
        """
        ...

    def model_dump(self) -> JsonDict:
        """Return this reference as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return this reference as a JSON string."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> PromptReference:
        """Rebuild a reference from ``model_dump_json()`` output.

        Args:
            data: the JSON text.

        Raises:
            WyrdError: when ``data`` is not a valid prompt reference.
        """
        ...

class PromptCardMetadata:
    """The prompt a ``PromptCard`` stores as its ``spec`` body.

    The stored prompt is read back through ``to_dict()`` or ``to_spec()``; the
    object exposes no ``prompt`` attribute."""

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
        """Wrap a prompt as card metadata.

        Args:
            prompt: the prompt to store. Omitted, a raw placeholder prompt is
                stored for the caller to replace.
            model_settings: settings applied to ``prompt`` first, as for
                ``PromptCard()``. Omitted, the prompt's settings are kept.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` when ``prompt`` is not a
                ``Prompt``; ``WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH`` or
                ``WYRD_PROMPT_400_SETTINGS_DECODE`` for unusable settings.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this metadata as a dictionary."""
        ...

    def to_spec(self) -> JsonDict:
        """Return the validated Prompt Card ``spec`` body as a dictionary.

        Raises:
            WyrdError: when the stored prompt fails Prompt Card validation.
        """
        ...

class PromptCard:
    """A local Prompt Card: a ``Prompt`` plus card identity, labels, and annotations.

    It saves and loads card envelopes as local files; registration belongs to
    the client. Identity fields are validated when the card is serialized or
    turned into a ``CardRef``, not when they are assigned.

    Attributes:
        space, name, version, uid: the card identity; assignable.
        labels: queryable user labels; assignable.
        annotations: free-form user annotations; assignable.
        metadata: the stored ``PromptCardMetadata``; assigning it replaces the
            prompt.
        prompt: the card's prompt; assigning it replaces the stored prompt.
        model_settings: a copy of the prompt's typed settings, or ``None``
            for a raw prompt.
        content_hash: the hash of the validated ``spec`` body.
        parameters: the text variables the prompt declares.
        is_fully_bound: ``True`` when the prompt declares no text variables.
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
        """Create a local Prompt Card around ``prompt``.

        Args:
            prompt: the prompt to store.
            space: the card space. Omitted, the repository's Wyrd config
                default applies, else ``"default"``.
            name: the card name. Omitted, ``"prompt"``.
            version: the card version. Omitted, ``"0.1.0"``.
            uid: the card UID. Omitted, a new UUIDv7.
            labels: queryable user labels, merged with any repository config
                defaults.
            annotations: free-form user annotations, merged with any
                repository config defaults.
            metadata: existing card metadata. Its prompt is always replaced
                by ``prompt``.
            model_settings: settings applied to ``prompt`` before storing,
                as the settings class for the prompt's provider or a mapping
                decoded for it. Omitted, the prompt's settings are kept.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` when ``prompt`` is not a
                ``Prompt`` or a label or annotation is invalid;
                ``WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH`` or
                ``WYRD_PROMPT_400_SETTINGS_DECODE`` for unusable settings.
        """
        ...

    def save(self, path: PathLike) -> None:
        """Write this card's ``wyrd/v1`` envelope to a local file.

        Args:
            path: a ``.json``, ``.yaml``, or ``.yml`` file, written in that
                format.

        Raises:
            WyrdError: ``WYRD_PROMPT_400_LOADER_BAD_EXTENSION`` for another
                extension, a validation error for invalid identity or prompt,
                or ``WYRD_PROMPT_500_LOADER_IO`` when the file or its parent
                directories cannot be written.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> PromptCard:
        """Load a card envelope from a local file. As ``PromptCard.from_path()``."""
        ...

    @staticmethod
    def from_path(path: PathLike) -> PromptCard:
        """Load a card envelope from a local ``.json``, ``.yaml``, or ``.yml`` file.

        Both the stored native ``spec`` and the declarative authoring ``spec``
        (a ``provider`` key instead of ``request``) are accepted.

        Raises:
            WyrdError: when the file cannot be read or parsed, or is not a
                ``wyrd/v1`` Prompt envelope with a space, uid, and version.
        """
        ...

    def model_dump(self) -> JsonDict:
        """Return this card's ``wyrd/v1`` envelope as a dictionary.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for invalid identity, or a
                prompt validation error.
        """
        ...

    def model_dump_json(self) -> str:
        """Return this card's ``wyrd/v1`` envelope as a JSON string.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for invalid identity, or a
                prompt validation error.
        """
        ...

    def _to_card_envelope_json(self) -> str:
        """Return the registry adapter's single envelope conversion."""
        ...

    def as_card_ref(self) -> CardRef:
        """Return a ``CardRef`` of kind ``Prompt`` for this card's identity.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when an identity field is
                invalid.
        """
        ...

    @staticmethod
    def model_validate_json(json_string: str) -> PromptCard:
        """Rebuild a card from ``model_dump_json()`` output.

        Args:
            json_string: the JSON envelope text.

        Raises:
            WyrdError: when the JSON is invalid or is not a ``wyrd/v1``
                Prompt envelope with a space, uid, and version.
        """
        ...

    def __str__(self) -> str:
        """Return the envelope as pretty-printed JSON, or a short error string if invalid."""
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
    "PromptReference",
    "ProviderRequest",
    "ProviderResponse",
    "ResponseFormat",
    "WyrdError",
]
