#### begin imports ####

from typing import Any

from .error import WyrdError
from .header import JsonDict, PathLike

#### end of imports ####

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
    """Client-safe native prompt builder."""

    provider: str
    request: ProviderRequest
    messages: list[JsonDict]
    message: JsonDict | None
    system_messages: Any
    model: str
    version: str | None
    variables: list[str]

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

    def bind_media(self, name: str, media: Any) -> Prompt:
        """Return a copy with a media placeholder bound."""
        ...

    def bind_media_mut(self, name: str, media: Any) -> None:
        """Bind a media placeholder in place."""
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
    "Prompt",
    "ProviderRequest",
    "ResponseFormat",
    "WyrdError",
]
