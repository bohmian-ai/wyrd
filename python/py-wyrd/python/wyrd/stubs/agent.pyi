#### begin imports ####
from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from contextlib import AbstractContextManager
from typing import Any, Protocol

from .error import WyrdError
from .header import JsonDict, PathLike
from .prompt import Prompt

#### end of imports ####

class SessionMemory(Protocol):
    """Session memory object consumed by `Agent` runs."""

    def recent(self, session_id: str, limit: int) -> Sequence[object]:
        """Return recent session turns.

        Args:
            session_id (str): Session id for the run.
            limit (int): Maximum turns requested.

        Returns:
            Sequence[object]: Session turns as `SessionTurn` or mappings.
        """
        ...

    def append(self, session_id: str, turn: object) -> None:
        """Append one session turn.

        Args:
            session_id (str): Session id for the run.
            turn (object): Session turn to store.
        """
        ...

if True:
    class RunConfig:
        """Run configuration for the bounded Agent loop."""

        max_iterations: int
        tool_concurrency_cap: int | None
        session_recent_limit: int | None
        timeout_ms: int | None

        def __init__(
            self,
            *,
            max_iterations: int = ...,
            tool_concurrency_cap: int | None = ...,
            session_recent_limit: int | None = ...,
            timeout_ms: int | None = ...,
        ) -> None:
            """Create run configuration.

            Args:
                max_iterations (int): Maximum model/tool loop iterations.
                tool_concurrency_cap (int | None): Maximum concurrent tool calls.
                session_recent_limit (int | None): Maximum recent session turns.
                timeout_ms (int | None): Overall run timeout in milliseconds.
            """
            ...

    class FinishReason:
        """Reason an Agent run terminated."""

        ModelStopped: FinishReason
        MaxIterations: FinishReason
        CallbackAborted: FinishReason
        ProviderError: FinishReason
        ToolError: FinishReason
        Timeout: FinishReason

    class AgentRun:
        """Output of one Agent run."""

        @property
        def finish_reason(self) -> FinishReason:
            """Return why the run terminated."""
            ...

        @property
        def output(self) -> str:
            """Return final assistant output text."""
            ...

        @property
        def iterations(self) -> int:
            """Return loop iteration count."""
            ...

        @property
        def tokens_in(self) -> int:
            """Return provider input token count."""
            ...

        @property
        def tokens_out(self) -> int:
            """Return provider output token count."""
            ...

        @property
        def conversation(self) -> JsonDict:
            """Return the run conversation."""
            ...

        @property
        def error(self) -> WyrdError | None:
            """Return the structured terminal error when the run aborted."""
            ...

class Agent:
    """Declarative and runnable Wyrd Agent."""

    def __init__(
        self,
        *,
        prompt: Prompt | Mapping[str, Any],
        name: str | None = ...,
        version: str | None = ...,
        space: str | None = ...,
        id: str | None = ...,
        tools: Sequence[object] | None = ...,
        run_config: RunConfig | None = ...,
        before_agent_callback: Callable[..., object] | None = ...,
        after_agent_callback: Callable[..., object] | None = ...,
        before_model_callback: Callable[..., object] | None = ...,
        after_model_callback: Callable[..., object] | None = ...,
        before_tool_callback: Callable[..., object] | None = ...,
        after_tool_callback: Callable[..., object] | None = ...,
        session: SessionMemory | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
    ) -> None:
        """Create an Agent.

        Args:
            prompt (Prompt | Mapping[str, Any]): Resolved prompt or PromptRef-like mapping.
            name (str | None): Optional envelope name.
            version (str | None): Optional envelope version.
            space (str | None): Optional envelope space.
            id (str | None): Optional stable runtime id.
            tools (Sequence[object] | None): Optional runtime-local decorated tools.
            run_config (RunConfig | None): Optional run configuration.
            before_agent_callback (Callable[..., object] | None): Optional callback before the run starts.
            after_agent_callback (Callable[..., object] | None): Optional callback after the run completes.
            before_model_callback (Callable[..., object] | None): Optional callback before model invocation.
            after_model_callback (Callable[..., object] | None): Optional callback after model invocation.
            before_tool_callback (Callable[..., object] | None): Optional callback before tool invocation.
            after_tool_callback (Callable[..., object] | None): Optional callback after tool invocation.
            session (SessionMemory | None): Optional session memory object.
            labels (Mapping[str, str] | None): Optional envelope labels.
            annotations (Mapping[str, str] | None): Optional envelope annotations.
        """
        ...

    @property
    def name(self) -> str | None:
        """Return the optional envelope name."""
        ...

    @property
    def version(self) -> str | None:
        """Return the optional envelope version."""
        ...

    @property
    def space(self) -> str | None:
        """Return the optional envelope space."""
        ...

    @property
    def id(self) -> str:
        """Return the stable runtime id."""
        ...

    @property
    def prompt(self) -> Prompt:
        """Return the resolved prompt."""
        ...

    @property
    def provider(self) -> str:
        """Return the provider name from the prompt."""
        ...

    @property
    def model(self) -> str:
        """Return the model name from the prompt."""
        ...

    @property
    def tool_names(self) -> list[str]:
        """Return runtime-local tool names."""
        ...

    def save(self, path: PathLike) -> None:
        """Save this Agent as local YAML.

        Args:
            path (PathLike): Destination YAML path.
        """
        ...

    @staticmethod
    def from_yaml(path: PathLike) -> Agent:
        """Load an Agent from local YAML.

        Args:
            path (PathLike): Source YAML path.

        Returns:
            Agent: Loaded Agent.
        """
        ...

    def to_yaml_string(self) -> str:
        """Return this Agent Card as YAML text."""
        ...

    def to_card(self) -> JsonDict:
        """Return this Agent Card as a JSON-compatible mapping."""
        ...

    def model_dump_json(self) -> str:
        """Return this Agent Card as JSON text."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> Agent:
        """Validate JSON into an Agent.

        Args:
            data (str): Agent Card JSON text.

        Returns:
            Agent: Validated Agent.
        """
        ...

    def validate_registrable(self) -> None:
        """Validate whether this local Agent can be durably registered."""
        ...

    def run(self, input: str | Mapping[str, Any], *, session_id: str | None = ...) -> AgentRun:
        """Run the bounded tool loop.

        Args:
            input (str | Mapping[str, Any]): User input for the run.
            session_id (str | None): Optional session id.

        Returns:
            AgentRun: Completed run result.
        """
        ...

    def as_tool(self, *, description: str | None = ...) -> object:
        """Return this Agent as a runtime-local delegate tool.

        Args:
            description (str | None): Optional tool description override.

        Returns:
            object: Decorated tool-compatible callable.
        """
        ...

def tool(
    fn: Callable[..., object] | None = ...,
    *,
    name: str | None = ...,
    description: str | None = ...,
) -> object:
    """Decorate and register a runtime-local tool.

    Args:
        fn (Callable[..., object] | None): Optional callable to decorate.
        name (str | None): Optional tool name override.
        description (str | None): Optional tool description override.

    Returns:
        object: Decorated callable, or a decorator when `fn` is omitted.
    """
    ...

def local_registry() -> AbstractContextManager[None]:
    """Return a scoped runtime-local tool registry context manager.

    Returns:
        AbstractContextManager[None]: Context manager for isolated tool registration.
    """
    ...

__all__ = [
    "Agent",
    "AgentRun",
    "FinishReason",
    "RunConfig",
    "SessionMemory",
    "local_registry",
    "tool",
]
