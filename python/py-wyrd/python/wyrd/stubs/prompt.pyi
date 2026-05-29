#### begin imports ####

from typing import Any

from .error import WyrdError
from .header import JsonDict, PathLike

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
        """Return a concise representation without payload bytes or URLs."""
        ...

class ProviderRequest:
    """Opaque provider-native request returned by prompt rendering."""

    provider: str
    messages: list[JsonDict]
    message: JsonDict | None
    system: Any

    def model_dump(self) -> JsonDict:
        """Return the native provider request as a Python dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return the native provider request as JSON."""
        ...

    def to_json(self) -> str:
        """Return the native provider request as JSON."""
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection."""
        ...

class ResponseFormat:
    """Provider-independent response-format authoring helper."""

    @staticmethod
    def text() -> ResponseFormat:
        """Request plain-text output."""
        ...

    @staticmethod
    def json_object() -> ResponseFormat:
        """Request provider-native JSON object output."""
        ...

    @staticmethod
    def json_schema(name: str, schema: JsonDict) -> ResponseFormat:
        """Request structured JSON output matching `schema`."""
        ...

    def to_dict(self) -> JsonDict:
        """Return the response format as a Python dictionary."""
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection."""
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

    def __new__(
        cls,
        messages: Any,
        model: str,
        *,
        provider: str,
        system: str | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        operation: str | None = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_tokens: int | None = ...,
        max_output_tokens: int | None = ...,
        top_k: int | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a provider-native prompt from dynamic authoring inputs."""
        ...

    @staticmethod
    def openai_chat(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_tokens: int | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an OpenAI Chat prompt."""
        ...

    @staticmethod
    def openai_responses(
        model: str,
        *,
        instructions: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_output_tokens: int | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an OpenAI Responses prompt."""
        ...

    @staticmethod
    def anthropic(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        max_tokens: int = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        top_k: int | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an Anthropic Messages prompt."""
        ...

    @staticmethod
    def gemini(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        top_k: int | None = ...,
        max_output_tokens: int | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a Gemini GenerateContent prompt."""
        ...

    @staticmethod
    def vertex(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        top_k: int | None = ...,
        max_output_tokens: int | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a Vertex GenerateContent prompt."""
        ...

    @staticmethod
    def raw(provider: str, model: str, body: bytes) -> Prompt:
        """Build a raw JSON passthrough prompt."""
        ...

    def system(self, text: str) -> Prompt:
        """Return a copy with a system message applied."""
        ...

    def user(self, content: Any) -> Prompt:
        """Return a copy with a user message appended."""
        ...

    def assistant(self, content: Any) -> Prompt:
        """Return a copy with an assistant message appended."""
        ...

    def tool_result(self, tool_use_id: str, content: str, is_error: bool = ...) -> Prompt:
        """Return a copy with a native tool-result message appended."""
        ...

    def render(self, **kwargs: str) -> ProviderRequest:
        """Render declared variables and return a provider request."""
        ...

    def bind(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> Prompt:
        """Return a copy with one or more variables bound."""
        ...

    def bind_mut(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> None:
        """Bind one or more variables in place."""
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
        """Return the native prompt as a Python dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return the native prompt as JSON."""
        ...

    def to_json(self) -> str:
        """Return the native prompt as JSON."""
        ...

    @staticmethod
    def from_json(data: str) -> Prompt:
        """Build a prompt from serialized native prompt JSON."""
        ...

    @staticmethod
    def load(path: PathLike) -> Prompt:
        """Load a bare native prompt from JSON or YAML."""
        ...

    def dump(self, path: PathLike) -> None:
        """Dump a bare native prompt to JSON or YAML."""
        ...

    @staticmethod
    def openai_image_url(url: str, detail: str | None = ...) -> JsonDict:
        """Return an OpenAI image URL content part."""
        ...

    @staticmethod
    def anthropic_image_base64(media_type: str, data: str) -> JsonDict:
        """Return an Anthropic base64 image content block."""
        ...

    @staticmethod
    def google_file_data(mime_type: str, file_uri: str) -> JsonDict:
        """Return a Google file-data content part."""
        ...

    @staticmethod
    def image_url(url: str, *, detail: str | None = ..., provider: str = ...) -> JsonDict:
        """Return an image URL content helper."""
        ...

    @staticmethod
    def image_base64(media_type: str, data: str) -> JsonDict:
        """Return a base64 image content helper."""
        ...

    @staticmethod
    def file_uri(mime_type: str, file_uri: str) -> JsonDict:
        """Return a file URI content helper."""
        ...

    @staticmethod
    def file_id(file_id: str) -> JsonDict:
        """Return a file-id content helper."""
        ...

    @staticmethod
    def document_text(media_type: str, data: str, title: str | None = ...) -> JsonDict:
        """Return a text document content helper."""
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection."""
        ...

__all__ = [
    "MediaRef",
    "Prompt",
    "ProviderRequest",
    "ResponseFormat",
    "WyrdError",
]
