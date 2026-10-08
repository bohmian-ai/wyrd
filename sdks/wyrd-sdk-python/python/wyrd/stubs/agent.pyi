#### begin imports ####
from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from contextlib import AbstractContextManager
from typing import (
    Any,
    Literal,
    ParamSpec,
    Protocol,
    TypedDict,
    TypeVar,
    overload,
    runtime_checkable,
)

from .cards import AgentCard
from .client import WyrdClient
from .error import WyrdError
from .header import JsonDict, PathLike
from .prompt import Prompt, ProviderResponse

#### end of imports ####

_P = ParamSpec("_P")
_R = TypeVar("_R")

class Role:
    """Session turn role.

    Values identify whether a turn is a system instruction or came from the
    user, assistant, or a tool. ``str(role)`` is the lowercase name
    (``"system"``, ``"user"``, ``"assistant"``, or ``"tool"``).
    """

    System: Role
    User: Role
    Assistant: Role
    Tool: Role

class SessionTurn:
    """One session memory turn.

    Session turns are passed between Python session memory objects and Agent runs.
    """

    def __init__(
        self,
        *,
        role: Role | str,
        content: str,
        call_id: str | None = None,
    ) -> None:
        """Create a session turn; every argument is keyword-only.

        Args:
            role: a ``Role`` or its name, lowercase (``"system"``,
                ``"user"``, ``"assistant"``, ``"tool"``) or capitalized.
            content: the turn text.
            call_id: the provider tool call id a ``tool`` turn answers.
                Omitted, the turn answers no tool call.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` for an unknown
                role name.
        """
        ...

    @property
    def role(self) -> str:
        """Return the lowercase role name, such as ``"user"``."""
        ...

    @property
    def content(self) -> str:
        """Return the turn text."""
        ...

    @property
    def call_id(self) -> str | None:
        """Return the tool call id a ``tool`` turn answers, or ``None``."""
        ...

    def model_dump(self) -> JsonDict:
        """Return the turn as ``{"role", "content", "call_id"}``."""
        ...

    def model_dump_json(self) -> str:
        """Return the turn as JSON text in the ``model_dump()`` shape."""
        ...

    @staticmethod
    def model_validate(value: SessionTurn | Mapping[str, Any]) -> SessionTurn:
        """Build a turn from a ``SessionTurn`` or a ``model_dump()``-shaped mapping.

        Args:
            value: a ``SessionTurn`` or a ``{"role", "content", "call_id"}``
                mapping.

        Returns:
            The validated turn.

        Raises:
            WyrdError: when the mapping is not one valid turn.
        """
        ...

    @staticmethod
    def model_validate_json(data: str) -> SessionTurn:
        """Build a turn from ``model_dump_json()`` text.

        Args:
            data: JSON text in the ``model_dump()`` shape.

        Returns:
            The validated turn.

        Raises:
            WyrdError: when the text is not one valid turn.
        """
        ...

@runtime_checkable
class SessionMemory(Protocol):
    """Backend that stores and replays an Agent's conversation turns per session.

    Pass an object with ``recent`` and ``append`` methods as
    ``Agent(session=...)``. The Agent uses it only for runs given a
    ``session_id``. An exception raised by either method fails the run.
    """

    def recent(self, session_id: str, limit: int) -> Sequence[SessionTurn | JsonDict]:
        """Return a session's most recent turns, oldest first.

        Called once at the start of each run that has a ``session_id``; the
        turns are replayed in the order returned, before the new user input.

        Args:
            session_id: the ``session_id`` passed to ``Agent.run()``.
            limit: the most turns to return: ``RunConfig.session_recent_limit``,
                or 50 when that is ``None``.

        Returns:
            ``SessionTurn`` values or mappings with ``role`` (``"system"``,
            ``"user"``, ``"assistant"``, or ``"tool"``), ``content``, and an
            optional ``call_id``.

        Raises:
            Exception: any exception fails the run with
                ``WYRD_SESSION_500_RECENT``.
        """
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Store one new turn of a session.

        Called for the run's user input, each assistant response, and each
        successful tool result. Failed tool calls are not appended.

        Args:
            session_id: the ``session_id`` passed to ``Agent.run()``.
            turn: the turn to store.

        Raises:
            Exception: any exception fails the run with
                ``WYRD_SESSION_500_APPEND``.
        """
        ...

class NoSession:
    """Session memory that stores nothing; ``recent`` always returns ``[]``."""

    def recent(self, session_id: str, limit: int) -> list[SessionTurn]:
        """Return ``[]``; both arguments are ignored.

        Args:
            session_id: ignored.
            limit: ignored.

        Returns:
            An empty list.
        """
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Discard ``turn``; both arguments are ignored.

        Args:
            session_id: ignored.
            turn: ignored.
        """
        ...

if True:
    class RunConfig:
        """Limits for one Agent's bounded model/tool loop.

        The read-only attributes mirror the constructor arguments.
        """

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
                max_iterations: the most model calls one run may make; defaults
                    to 10. A run that still has tool calls to answer after the
                    last iteration raises ``WYRD_AGENT_500_MAX_ITERATIONS``, as
                    does every run when this is 0.
                tool_concurrency_cap: the most tool calls of one model response
                    run concurrently; defaults to 8. ``None`` also means 8 and
                    0 means 1.
                session_recent_limit: the ``limit`` passed to
                    ``SessionMemory.recent()``. ``None`` (the default) means 50.
                timeout_ms: the wall-clock budget for a whole run in
                    milliseconds. ``None`` (the default) means no timeout; an
                    expired run raises ``WYRD_AGENT_504_TIMEOUT``.
            """
            ...

    class FinishReason:
        """Reason an Agent run terminated.

        A returned ``AgentRun`` reports ``ModelStopped`` (the model answered
        without tool calls) or ``CallbackAborted`` (a ``before_agent``,
        ``before_model``, ``after_model``, or ``after_agent`` callback raised
        or returned a value of the wrong type; see ``AgentRun.error``). Iteration
        exhaustion, provider failure, and timeout raise ``WyrdError`` from
        ``Agent.run()`` instead of returning ``MaxIterations``,
        ``ProviderError``, or ``Timeout``, and failed tools are reported back
        to the model rather than ending the run with ``ToolError``.
        """

        ModelStopped: FinishReason
        MaxIterations: FinishReason
        CallbackAborted: FinishReason
        ProviderError: FinishReason
        ToolError: FinishReason
        Timeout: FinishReason

    class AgentRun:
        """Result of one completed ``Agent.run()``."""

        @property
        def finish_reason(self) -> FinishReason:
            """Return why the run terminated."""
            ...

        @property
        def output(self) -> str:
            """Return the final assistant text; ``""`` when a callback aborted the run."""
            ...

        @property
        def iterations(self) -> int:
            """Return how many loop iterations ran; 0 when ``before_agent`` aborted."""
            ...

        @property
        def tokens_in(self) -> int:
            """Return the input tokens the final provider response reports.

            Earlier iterations are not summed. 0 when the run reached no
            response or the provider reported no usage.
            """
            ...

        @property
        def tokens_out(self) -> int:
            """Return the output tokens the final provider response reports, as ``tokens_in``."""
            ...

        @property
        def conversation(self) -> JsonDict:
            """Return the run's accumulated conversation as a JSON-compatible mapping."""
            ...

        @property
        def error(self) -> WyrdError | None:
            """Return the callback's error when the run ended ``CallbackAborted``, else ``None``."""
            ...

        @property
        def structured_output(self) -> dict[str, Any] | None:
            """Return the final output parsed as a JSON object.

            Present only when the Prompt declared an output schema; ``None``
            for text prompts.
            """
            ...

        @property
        def parsed(self) -> Any:
            """Return the output as an ``output_type`` instance, or ``None``.

            Set only when the run produced ``structured_output`` and an output
            class came from ``Agent.run(output_type=)``, ``Agent(output_type=)``,
            or ``Prompt(output=)``, in that order of precedence.
            """
            ...

        @property
        def provider_response(self) -> ProviderResponse | None:
            """Return the final provider response, or ``None`` if the run reached none."""
            ...

class ConversationTurn:
    """One read-only turn of an Agent conversation."""

    @property
    def role(self) -> Role:
        """Role that produced the turn."""
        ...

    @property
    def content(self) -> str | None:
        """Text of a system or user turn; `None` for assistant and tool turns."""
        ...

    @property
    def call_id(self) -> str | None:
        """Provider tool call id of a tool result turn, otherwise `None`."""
        ...

    def to_dict(self) -> JsonDict:
        """Return the turn as its JSON-compatible wire mapping."""
        ...

class Conversation:
    """Read-only conversation snapshot."""

    @property
    def turns(self) -> list[ConversationTurn]:
        """Turns in insertion order."""
        ...

    def __len__(self) -> int: ...

class CallbackContext:
    """Read-only context passed as `ctx` to every Agent callback."""

    @property
    def agent_id(self) -> str:
        """Stable runtime id of the Agent whose callback is firing."""
        ...

    @property
    def session_id(self) -> str | None:
        """Session id of the current run, or `None` without a session."""
        ...

    @property
    def iteration(self) -> int:
        """Zero-based model/tool loop iteration."""
        ...

    @property
    def conversation(self) -> Conversation:
        """Conversation snapshot at this callback fire point."""
        ...

class MockProvider:
    """Offline, deterministic `mock` provider with caller-set canned responses.

    Pass it to `Agent(mock_provider=...)` for prompts declaring
    `provider="mock"`. Each model call returns the next canned response in
    order; once the queue is empty the provider echoes the last user message.
    No network or credentials are used. One provider shared by several Agents
    is consumed across all of their runs.

    ```python
    mock = MockProvider(["first answer", "second answer"])
    agent = Agent(prompt=Prompt(messages="hi", model="m", provider="mock"), mock_provider=mock)
    assert agent.run("hi").output == "first answer"
    ```
    """

    def __init__(self, responses: Sequence[str] | None = ...) -> None:
        """Create a mock provider returning `responses` in order, then echoing.

        Args:
            responses: canned assistant responses, consumed in order. Omitted,
                the queue starts empty.
        """
        ...

    def push(self, text: str) -> None:
        """Queue one more canned assistant response.

        Args:
            text: the response text.
        """
        ...

    @property
    def remaining(self) -> int:
        """Number of canned responses still queued."""
        ...

class Agent:
    """A Prompt-backed agent that runs a bounded model/tool loop and saves as an Agent Card."""

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
        mock_provider: MockProvider | None = ...,
    ) -> None:
        """Create an Agent.

        Every callback receives a read-only ``CallbackContext`` with
        ``agent_id``, ``session_id``, ``iteration``, and ``conversation``
        attributes as its first argument. Returning ``None`` keeps the value unchanged. A callback
        that returns a value of the wrong type is treated as raising, with
        ``WYRD_AGENT_422_CALLBACK_RETURN_TYPE`` as the error. A raised
        ``WyrdError``, or any exception whose ``args`` start with a catalog
        code and a message, keeps that code; any other exception becomes
        ``WYRD_AGENT_422_VALIDATION``.

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
            mock_provider (MockProvider | None): Run `provider="mock"` prompts offline
                against this provider instead of the process default registry. Cannot be
                combined with `provider_base_url`.
        """
        ...

    @property
    def name(self) -> str | None:
        """Return the Agent Card name, or ``None`` when unset."""
        ...

    @property
    def version(self) -> str | None:
        """Return the Agent Card version, or ``None`` when unset."""
        ...

    @property
    def space(self) -> str | None:
        """Return the Agent Card space, or ``None`` when unset."""
        ...

    @property
    def id(self) -> str:
        """Return the runtime id: the ``id`` argument or a generated UUIDv7."""
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
        """Return the names of the attached runtime-local tools."""
        ...

    def save(self, path: PathLike) -> None:
        """Write this Agent Card to ``path`` as YAML.

        Args:
            path: the destination file.

        Raises:
            WyrdError: ``WYRD_AGENT_422_MISSING_NAME`` or
                ``WYRD_AGENT_422_MISSING_VERSION`` when identity is incomplete,
                or the IO or serialization error.
        """
        ...

    @staticmethod
    def from_yaml(path: PathLike) -> Agent:
        """Load an Agent from an Agent Card YAML file.

        Tool names in the Card resolve against the process-wide tool registry,
        so decorate the tools before loading.

        Args:
            path: the Agent Card YAML file.

        Returns:
            The loaded Agent.

        Raises:
            WyrdError: ``WYRD_AGENT_404_RUNTIME_LOCAL_TOOL_NOT_FOUND`` for an
                unregistered tool, ``WYRD_AGENT_404_PROMPT_CARD`` for an
                unresolvable Prompt reference, or the IO or parse error.
        """
        ...

    def to_yaml_string(self) -> str:
        """Return this Agent Card as YAML text; raises as ``save()``."""
        ...

    def to_card(self) -> AgentCard:
        """Return this Agent as a typed, unregistered `AgentCard` envelope."""
        ...

    def model_dump_json(self) -> str:
        """Return this Agent Card as JSON text; raises as ``save()``."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> Agent:
        """Build an Agent from Agent Card JSON text.

        Args:
            data: Agent Card JSON text.

        Returns:
            The loaded Agent.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` for invalid JSON,
                otherwise as ``from_yaml()``.
        """
        ...

    def validate_registrable(self) -> None:
        """Check that this Agent can be registered as a durable Card.

        Raises:
            WyrdError: ``WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE``
                when runtime-local tools are attached.
        """
        ...

    def run(
        self,
        input: str | Mapping[str, Any],
        *,
        session_id: str | None = ...,
        output_type: type | None = ...,
    ) -> AgentRun:
        """Run the bounded model/tool loop to completion, blocking until it ends.

        Each iteration calls the model once and then runs any requested tools,
        feeding their results (including tool errors) back to the model. The
        run ends when the model answers without tool calls.

        Args:
            input: the user message. A string is sent as-is; a mapping is sent
                as its JSON text.
            session_id: the session whose recent turns seed the conversation
                and receive this run's turns. Omitted, session memory is not
                used.
            output_type: as ``Agent(output_type=)``, for this call only; it
                takes precedence over the Agent's and the Prompt's class.

        Returns:
            The completed run, with its output and turns.

        Raises:
            WyrdError: ``WYRD_AGENT_500_MAX_ITERATIONS``,
                ``WYRD_AGENT_502_PROVIDER``, ``WYRD_AGENT_504_TIMEOUT``,
                ``WYRD_AGENT_404_TOOL_NOT_IN_AGENT`` when the model calls an
                unattached tool, ``WYRD_AGENT_422_STRUCTURED_DECODE`` when the
                declared JSON output or ``output_type`` cannot be parsed,
                ``WYRD_SESSION_500_RECENT`` or ``WYRD_SESSION_500_APPEND``.
        """
        ...

    def add_tool(self, tool: object) -> None:
        """Attach one more runtime-local tool in place.

        Args:
            tool: a callable decorated with ``tool()`` or returned by
                ``as_tool()``.

        Raises:
            WyrdError: ``WYRD_TOOL_400_INVALID_SCHEMA`` for an undecorated
                callable.
        """
        ...

    def set_tools(self, tools: Sequence[object]) -> None:
        """Replace every runtime-local tool in place.

        Args:
            tools: the decorated tool callables that replace the current set.

        Raises:
            WyrdError: as ``add_tool()``.
        """
        ...

    def with_prompt(self, prompt: Prompt) -> None:
        """Replace the Prompt in place.

        Unlike ``Agent(prompt=)``, only a ``Prompt`` instance is accepted.

        Args:
            prompt: the replacement Prompt.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` when ``prompt`` is
                not a ``Prompt``.
        """
        ...

    def with_session(self, session: SessionMemory) -> None:
        """Replace the session memory in place.

        Args:
            session: an object with ``recent`` and ``append`` methods.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` when ``session``
                lacks callable ``recent`` and ``append`` methods.
        """
        ...

    def with_run_config(self, run_config: RunConfig) -> None:
        """Replace the loop limits in place.

        Args:
            run_config: the replacement limits.
        """
        ...

    def add_before_agent(self, callback: Callable[..., object]) -> None:
        """Append a callback after any registered ``before_agent_callback``.

        Callbacks of one hook run in registration order, each seeing the
        previous one's replacement; the first that raises stops the chain with
        that hook's raise behavior from ``Agent()``.

        Args:
            callback: the callback to append.
        """
        ...

    def add_after_agent(self, callback: Callable[..., object]) -> None:
        """Append an ``after_agent_callback``, chained as ``add_before_agent()``.

        Args:
            callback: the callback to append.
        """
        ...

    def add_before_model(self, callback: Callable[..., object]) -> None:
        """Append a ``before_model_callback``, chained as ``add_before_agent()``.

        Args:
            callback: the callback to append.
        """
        ...

    def add_after_model(self, callback: Callable[..., object]) -> None:
        """Append an ``after_model_callback``, chained as ``add_before_agent()``.

        Args:
            callback: the callback to append.
        """
        ...

    def add_before_tool(self, callback: Callable[..., object]) -> None:
        """Append a ``before_tool_callback``, chained as ``add_before_agent()``.

        Args:
            callback: the callback to append.
        """
        ...

    def add_after_tool(self, callback: Callable[..., object]) -> None:
        """Append an ``after_tool_callback``, chained as ``add_before_agent()``.

        Args:
            callback: the callback to append.
        """
        ...

    def as_tool(self, *, description: str | None = ...) -> Callable[[Mapping[str, Any]], Any]:
        """Return this Agent as a tool another Agent can call.

        The tool is named after ``Agent.id``, takes one ``input`` string,
        runs this Agent on it, and returns its output text. Delegation nests at
        most three levels deep. The tool is not added to any registry.

        Args:
            description: the description the calling model sees. Omitted, it
                is ``"Delegate to agent <id>"``.

        Returns:
            A callable that takes the tool arguments as one mapping, such as
            ``{"input": "..."}``, and returns the delegate Agent's output.
        """
        ...

@overload
def tool(
    fn: None = ...,
    *,
    name: str | None = ...,
    description: str | None = ...,
) -> Callable[[Callable[_P, _R]], Callable[_P, _R]]:
    """Return a decorator that registers a function as a tool; see the other overload."""
    ...

@overload
def tool(
    fn: Callable[_P, _R],
    *,
    name: str | None = ...,
    description: str | None = ...,
) -> Callable[_P, _R]:
    """Decorate a function as a runtime-local tool and register it.

    The input schema comes from the parameters' annotations (parameters without
    defaults are required) and the output schema from the return annotation.
    The model's arguments are passed as keyword arguments, and the return value
    must be JSON-compatible. The returned callable still calls ``fn`` directly.
    Registration goes to the innermost ``local_registry()`` on this thread, or
    the process-wide registry outside one.

    Args:
        fn: the function to decorate. Omitted, returns a decorator, so both
            ``@tool`` and ``@tool(name=...)`` work.
        name: the tool name the model sees. Omitted, ``fn.__name__``.
        description: the description the model sees. Omitted, ``fn``'s
            docstring, or ``""`` without one.

    Raises:
        WyrdError: ``WYRD_TOOL_409_NAME_TAKEN`` when the active registry
            already holds a tool with that name.
    """
    ...

def local_registry() -> AbstractContextManager[None]:
    """Scope tool registration to a temporary, thread-local registry.

    Tools decorated inside the ``with`` block register into a fresh registry
    that is discarded on exit, so names may repeat across blocks (useful in
    tests). Nested blocks stack. ``Agent.from_yaml()`` and
    ``Agent.model_validate_json()`` still resolve tool names against the
    process-wide registry.
    """
    ...

class WorkflowRunError(TypedDict):
    """Bounded primary error of a failed Workflow run or step."""

    code: str
    message: str
    details: Any
    remediation: str

class WorkflowStepResult(TypedDict):
    """One step's result in `WorkflowRun.steps`, keyed by step id."""

    status: Literal["pending", "running", "succeeded", "failed", "cancelled", "unstarted"]
    text: str | None
    structured_output: Any
    attempts: int
    started_at: str | None
    ended_at: str | None
    error: WorkflowRunError | None

class WorkflowRunDict(TypedDict):
    """Complete wire-shaped snapshot returned by `WorkflowRun.to_dict`."""

    run_id: str
    workflow: dict[str, Any] | None
    status: Literal["succeeded", "failed", "cancelled", "timed_out"]
    outputs: dict[str, Any]
    steps: dict[str, WorkflowStepResult]
    created_at: str
    started_at: str | None
    ended_at: str | None
    error: WorkflowRunError | None

class WorkflowRun:
    """Terminal Workflow run snapshot returned by `Workflow.run`.

    Values are the portable wire projection: named `outputs`, step results
    keyed by step id, and the bounded primary `error`.
    """

    @property
    def run_id(self) -> str:
        """Return the run identifier."""
        ...

    @property
    def status(self) -> Literal["succeeded", "failed", "cancelled", "timed_out"]:
        """Return the terminal run status."""
        ...

    @property
    def outputs(self) -> dict[str, Any]:
        """Return the named Workflow outputs; empty unless the run succeeded."""
        ...

    @property
    def steps(self) -> dict[str, WorkflowStepResult]:
        """Return step results keyed by step id."""
        ...

    @property
    def error(self) -> WorkflowRunError | None:
        """Return the primary run error, or None."""
        ...

    def to_dict(self) -> WorkflowRunDict:
        """Return the complete snapshot as its wire-shaped dictionary."""
        ...

class Workflow:
    """A YAML-authored DAG of Agent steps, loaded with ``from_path`` or ``from_yaml``.

    Each step is identified by its Agent's ``name``, or its ``id`` when the
    Agent is unnamed.
    """

    @property
    def name(self) -> str | None:
        """Return the workflow name."""
        ...

    @property
    def version(self) -> str | None:
        """Return the workflow version, or ``None`` when unset."""
        ...

    @property
    def space(self) -> str | None:
        """Return the workflow space, or ``None`` when unset."""
        ...

    @property
    def steps(self) -> Sequence[str]:
        """Return the ordered step ids."""
        ...

    @staticmethod
    def from_path(path: PathLike, client: WyrdClient | None = None) -> Workflow:
        """Load an authored Workflow file and the Cards it references.

        Relative paths and sibling Agents and Prompts in the same bundle load
        locally; a wholly local file needs no server or credentials. Registry
        Card refs are read exactly as ``client``; omitted, they are read
        through the ambient Wyrd client configuration (`WYRD_SERVER_URL` and
        `WYRD_API_KEY` or `WYRD_ACCESS_TOKEN`). A given ``client`` also makes
        the loaded Workflow's gateway calls, so a program serving several
        principals passes each one's client.

        Loading only reads files and Cards; it registers and runs nothing. A
        failure after some reads returns no partial Workflow.

        Args:
            path (PathLike): Workflow entry file.
            client (WyrdClient | None): Principal that reads registry refs and
                calls the gateway; the ambient configuration when omitted.

        Returns:
            Workflow: Fully hydrated and validated workflow.

        Raises:
            WyrdError: When the file fails to load or validate;
                `WYRD_CLIENT_401_NO_CREDENTIALS` when the file references a
                registered Card and no credential is configured;
                `WYRD_PERMISSION_403_DENIED_RBAC` when the credential cannot
                read Cards; `WYRD_REGISTRY_404_CARD_NOT_FOUND` when a
                referenced Card does not exist or was deleted.

        Example:
            ```python
            from wyrd.agent import Workflow

            workflow = Workflow.from_path("workflows/code-review/workflow.yaml")
            print(workflow.steps)
            ```
        """
        ...

    @staticmethod
    def from_yaml(yaml: str) -> Workflow:
        """Parse a workflow from envelope YAML text, resolving inline Agents eagerly.

        Args:
            yaml: the workflow envelope YAML text.

        Returns:
            The parsed workflow.

        Raises:
            WyrdError: When parse or resolution fails.
        """
        ...

    def run(self, input: str | Mapping[str, Any] | None = None) -> WorkflowRun:
        """Run this workflow, preparing only what its step routes select.

        Native steps use the process-local provider registry. Steps routed to
        the Wyrd gateway call the server and credential this Workflow was
        loaded through, or the ambient client configuration when it was built
        locally. Steps routed to an external gateway use the bindings named in
        the shared client configuration; only the selected bindings' secrets
        are read, at run start. Dependencies order steps only; data reaches a step solely through its
        declared bindings.

        Args:
            input (str | Mapping[str, Any] | None): Workflow input. A string is
                shorthand for the declared string input named `input`; a
                mapping supplies declared inputs by name; `None` uses defaults.

        Returns:
            WorkflowRun: Terminal run snapshot. Step failures are recorded in
            it rather than raised.

        Raises:
            WyrdError: When validation, input, or route checks fail before any
                step is dispatched.
        """
        ...

__all__ = [
    "Agent",
    "AgentRun",
    "CallbackContext",
    "Conversation",
    "ConversationTurn",
    "FinishReason",
    "MockProvider",
    "NoSession",
    "Role",
    "RunConfig",
    "SessionMemory",
    "SessionTurn",
    "Workflow",
    "WorkflowRun",
    "WorkflowRunDict",
    "WorkflowRunError",
    "WorkflowStepResult",
    "local_registry",
    "tool",
]
