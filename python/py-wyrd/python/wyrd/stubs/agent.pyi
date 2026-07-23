#### begin imports ####
from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from contextlib import AbstractContextManager
from typing import Any, Protocol, runtime_checkable

from .error import WyrdError
from .header import JsonDict, PathLike
from .observer import Observer
from .prompt import Prompt, ProviderResponse

#### end of imports ####

class Role:
    """Session turn role.

    Values identify whether a turn came from the user, assistant, or tool.
    """

    User: Role
    Assistant: Role
    Tool: Role

class SessionTurn:
    """One session memory turn.

    Session turns are passed between Python session memory objects and Agent runs.
    """

    def __init__(
        self,
        role: Role,
        content: str,
        *,
        tool_call_id: str | None = ...,
    ) -> None:
        """Create a session turn.

        Args:
            role (Role): Role for the turn.
            content (str): Turn content.
            tool_call_id (str | None): Optional tool call id for tool turns.
        """
        ...

    @property
    def role(self) -> Role:
        """Return the turn role."""
        ...

    @property
    def content(self) -> str:
        """Return the turn content."""
        ...

    @property
    def tool_call_id(self) -> str | None:
        """Return the optional tool call id."""
        ...

    def to_dict(self) -> JsonDict:
        """Return a JSON-compatible session turn mapping."""
        ...

@runtime_checkable
class SessionMemory(Protocol):
    """Protocol for Python session memory objects.

    Implement this protocol to provide recent and append behavior to Agent runs.
    """

    def recent(self, session_id: str, limit: int) -> Sequence[SessionTurn | JsonDict]:
        """Return recent session turns.

        Args:
            session_id (str): Session id for the run.
            limit (int): Maximum recent turns requested.

        Returns:
            Sequence[SessionTurn | JsonDict]: Recent turns.
        """
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Append one session turn.

        Args:
            session_id (str): Session id for the run.
            turn (SessionTurn): Turn to append.
        """
        ...

class NoSession:
    """No-op session memory implementation.

    Use this when an Agent should not persist session turns.
    """

    def recent(self, session_id: str, limit: int) -> list[SessionTurn]:
        """Return an empty recent-turn list."""
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Ignore one session turn."""
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

        @property
        def structured_output(self) -> dict[str, Any] | None:
            """Return parsed JSON output when the prompt declared an output schema."""
            ...

        @property
        def parsed(self) -> Any:
            """Return the typed model instance when output_type was a class, or None."""
            ...

        @property
        def provider_response(self) -> ProviderResponse | None:
            """Return the final provider response as a typed wrapper, if the run reached one."""
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
        provider_base_url: str | None = ...,
        provider_api_key: str | None = ...,
        output_type: type | None = ...,
    ) -> None:
        """Create an Agent.

        Args:
            prompt (Prompt | Mapping[str, Any]): Resolved prompt or inlineable prompt-reference mapping.
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
            provider_base_url (str | None): Override the provider endpoint for this agent only.
                Useful for routing through an AI gateway such as LiteLLM. When omitted the
                process-global default registry is used.
            provider_api_key (str | None): API key for the overridden endpoint. When omitted
                the standard environment variable for the prompt's provider is used.
            output_type (type | None): Optional Python class for parsing AgentRun.parsed.
                Must be callable and accept keyword arguments matching the structured output
                fields (typically a pydantic.BaseModel subclass). Does NOT inject a
                response_format schema — use Prompt(output=...) for schema enforcement.
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

    def run(
        self,
        input: str | Mapping[str, Any],
        *,
        session_id: str | None = ...,
        output_type: type | None = ...,
    ) -> AgentRun:
        """Run the bounded tool loop.

        Args:
            input (str | Mapping[str, Any]): User input for the run.
            session_id (str | None): Optional session id.
            output_type (type | None): Per-call class override for AgentRun.parsed.
                Overrides Agent(output_type=...) and Prompt(output=...) for this call only.

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

class StepStatus:
    """Lifecycle status of one workflow step."""

    Pending: StepStatus
    Running: StepStatus
    Completed: StepStatus
    Failed: StepStatus

class StepOutcome:
    """Per-step final outcome captured after a workflow run."""

    @property
    def status(self) -> StepStatus:
        """Return the final step status."""
        ...

    @property
    def retries(self) -> int:
        """Return the number of retries consumed before reaching the final status."""
        ...

class StepEvent:
    """One observable step transition during a workflow run."""

    @property
    def step_id(self) -> str:
        """Return the step id this event refers to."""
        ...

    @property
    def status(self) -> StepStatus:
        """Return the step status recorded for this transition."""
        ...

    @property
    def started_at(self) -> int:
        """Return the Unix epoch milliseconds when the attempt started."""
        ...

    @property
    def ended_at(self) -> int:
        """Return the Unix epoch milliseconds when the attempt ended."""
        ...

    @property
    def attempt(self) -> int:
        """Return the attempt index, starting at 1."""
        ...

    @property
    def error(self) -> str | None:
        """Return the stable error code for failed attempts, or None for completed ones."""
        ...

class WorkflowRun:
    """Final envelope returned by a successful `Workflow.run` call."""

    @property
    def final_step_id(self) -> str | None:
        """Return the terminal step's id when the workflow produced one."""
        ...

    @property
    def outcomes(self) -> Mapping[str, StepOutcome]:
        """Return per-step outcomes keyed by step id."""
        ...

    @property
    def events(self) -> Sequence[StepEvent]:
        """Return the ordered per-step events captured during the run."""
        ...

    @property
    def parameters(self) -> dict[str, Any]:
        """Return the accumulated parameter map from structured outputs."""
        ...

    @property
    def final_output(self) -> str | None:
        """Return the terminal step's assistant text, when present."""
        ...

class Workflow:
    """Authoring + run surface for a DAG of agents."""

    def __init__(
        self,
        *,
        name: str,
        version: str | None = ...,
        space: str | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
        observers: Sequence[Observer] | None = ...,
    ) -> None:
        """Build an empty Workflow with the given name and optional metadata.

        Args:
            name (str): Workflow name.
            version (str | None): Optional semantic version.
            space (str | None): Optional logical space.
            labels (Mapping[str, str] | None): Optional queryable labels.
            annotations (Mapping[str, str] | None): Optional free-form annotations.
            observers (Sequence[Observer] | None): Runtime observers for workflow runs.
        """
        ...

    @staticmethod
    def sequential(
        name: str,
        *agents: Agent,
        observers: Sequence[Observer] | None = ...,
    ) -> Workflow:
        """Build a workflow whose steps run sequentially.

        Args:
            name (str): Workflow name.
            *agents (Agent): One or more Agent values to chain.
            observers (Sequence[Observer] | None): Runtime observers for workflow runs.

        Returns:
            Workflow: Workflow with each agent depending on the previous one.

        Raises:
            WyrdError: When the resulting DAG is invalid.
        """
        ...

    @staticmethod
    def parallel(
        name: str,
        *agents: Agent,
        observers: Sequence[Observer] | None = ...,
    ) -> Workflow:
        """Build a workflow whose steps run in parallel with no dependencies.

        Args:
            name (str): Workflow name.
            *agents (Agent): One or more Agent values to run in parallel.
            observers (Sequence[Observer] | None): Runtime observers for workflow runs.

        Returns:
            Workflow: Workflow with each agent as an independent root step.

        Raises:
            WyrdError: When the resulting DAG is invalid.
        """
        ...

    def add(self, agent: Agent) -> Workflow:
        """Append `agent` as a new step with no dependencies.

        Args:
            agent (Agent): Agent to append.

        Returns:
            Workflow: This workflow (for chaining).

        Raises:
            WyrdError: When the resulting DAG is invalid.
        """
        ...

    def add_after(self, agent: Agent, after: Agent | str | Sequence[Agent | str]) -> Workflow:
        """Append `agent` as a new step depending on the supplied predecessors.

        Args:
            agent (Agent): Agent to append.
            after (Agent | str | Sequence[Agent | str]): Predecessor step ids
                or Agent values (their names are used as ids).

        Returns:
            Workflow: This workflow (for chaining).

        Raises:
            WyrdError: When the resulting DAG is invalid.
        """
        ...

    def set_version(self, version: str) -> None:
        """Set the workflow's semantic version in place."""
        ...

    def set_space(self, space: str) -> None:
        """Set the workflow's space in place."""
        ...

    @property
    def name(self) -> str | None:
        """Return the workflow name."""
        ...

    @property
    def version(self) -> str | None:
        """Return the workflow version."""
        ...

    @property
    def space(self) -> str | None:
        """Return the workflow space."""
        ...

    @property
    def steps(self) -> Sequence[str]:
        """Return the ordered step ids."""
        ...

    def to_yaml(self) -> str:
        """Serialize this workflow to a canonical envelope YAML string.

        Returns:
            str: Canonical envelope YAML body.

        Raises:
            WyrdError: When identity or codec fails.
        """
        ...

    def save(self, path: PathLike) -> None:
        """Save this workflow to disk as canonical envelope YAML.

        Args:
            path (PathLike): Filesystem path.

        Raises:
            WyrdError: When identity, IO, or codec fails.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> Workflow:
        """Load a workflow from disk.

        Args:
            path (PathLike): Filesystem path.

        Returns:
            Workflow: Reconstructed workflow with eager inline agent resolution.

        Raises:
            WyrdError: When IO, codec, or resolution fails.
        """
        ...

    @staticmethod
    def from_yaml(yaml: str) -> Workflow:
        """Parse a workflow from a canonical envelope YAML string.

        Args:
            yaml (str): Envelope YAML body.

        Returns:
            Workflow: Reconstructed workflow with eager inline agent resolution.

        Raises:
            WyrdError: When parse or resolution fails.
        """
        ...

    def run(self, input: str | Mapping[str, Any]) -> WorkflowRun:
        """Run this workflow against the process-local provider registry.

        Args:
            input (str | Mapping[str, Any]): Workflow input. A string lands as
                the `input` template variable; a mapping exposes every key as a
                discrete template variable.

        Returns:
            WorkflowRun: Final run envelope with per-step outcomes, events, and
            cross-step parameter map.

        Raises:
            WyrdError: When a provider call fails, retries exhaust, or any
                step references an undefined variable.
        """
        ...

__all__ = [
    "Agent",
    "AgentRun",
    "FinishReason",
    "NoSession",
    "Role",
    "RunConfig",
    "SessionMemory",
    "SessionTurn",
    "StepEvent",
    "StepOutcome",
    "StepStatus",
    "Workflow",
    "WorkflowRun",
    "local_registry",
    "tool",
]
