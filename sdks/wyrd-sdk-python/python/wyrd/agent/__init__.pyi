# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from contextlib import AbstractContextManager
from typing import Any, Protocol, runtime_checkable

from .._wyrd import JsonDict, PathLike, WyrdError
from ..observer import Observer
from ..prompt import Prompt, ProviderResponse

#### end of imports ####

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

        Raises:
            WyrdError: when the mapping is not one valid turn.
        """
        ...

    @staticmethod
    def model_validate_json(data: str) -> SessionTurn:
        """Build a turn from ``model_dump_json()`` text.

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
        """Return ``[]``; both arguments are ignored."""
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Discard ``turn``; both arguments are ignored."""
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
    ) -> None:
        """Create an Agent.

        Every callback receives a ``ctx`` mapping with ``agent_id``,
        ``session_id``, ``iteration``, and ``conversation`` as its first
        argument. Returning ``None`` keeps the value unchanged. A callback
        that returns a value of the wrong type is treated as raising, with
        ``WYRD_AGENT_422_CALLBACK_RETURN_TYPE`` as the error. A raised
        ``WyrdError``, or any exception whose ``args`` start with a catalog
        code and a message, keeps that code; any other exception becomes
        ``WYRD_AGENT_422_VALIDATION``.

        Args:
            prompt: the Prompt that fixes provider, model, and messages, or a
                Prompt-reference mapping (or object with
                ``model_dump_json()``) resolved now.
            name: the Agent Card name; required by ``save()``, ``to_card()``,
                and the other Card serializers. Also the step id inside a
                ``Workflow``.
            version: the Agent Card version; required like ``name``.
            space: the Agent Card space. Omitted, Cards use ``"default"``.
            id: the runtime id used for diagnostics, observer events, and
                ``as_tool()``. Omitted, a UUIDv7 is generated.
            tools: callables decorated with ``tool()`` (or returned by
                ``as_tool()``) the model may call.
            run_config: loop limits. Omitted, ``RunConfig()`` defaults apply.
            before_agent_callback: ``(ctx, input) -> str | None`` before the
                first model call; a string replaces the user input. Raising
                ends the run ``CallbackAborted`` with zero iterations.
            after_agent_callback: ``(ctx, run) -> mapping | None`` after the
                model stops; a mapping in the serialized ``AgentRun`` shape
                replaces the result. Raising ends the run ``CallbackAborted``
                with empty ``output`` and the run's iteration count.
            before_model_callback: ``(ctx, request) -> ProviderRequest | None``
                before each model call; a request replaces the outbound one.
                Raising ends the run ``CallbackAborted``.
            after_model_callback: ``(ctx, response) -> ProviderResponse | None``
                after each successful model call; a response replaces it.
                Raising discards the response and ends the run
                ``CallbackAborted``.
            before_tool_callback: ``(ctx, tool_name, args) -> args | None``
                before each tool call; a value replaces the arguments.
                Raising skips that tool call, reports an error result to the
                model, and continues the run.
            after_tool_callback: ``(ctx, tool_name, result) -> result | None``
                after each tool call; ``result`` is the tool's JSON output, or
                ``{"error", "code"}`` when it failed. A value replaces it as a
                successful result. Raising reports that tool call to the model
                as failed with the raised error and continues the run.
            session: a ``SessionMemory`` used for runs given a ``session_id``.
                Omitted, nothing is remembered between runs.
            labels: Agent Card labels.
            annotations: Agent Card annotations.
            provider_base_url: an endpoint for this Agent's provider calls,
                such as an AI gateway. Omitted, the process-wide provider
                registry and its standard endpoints are used.
            provider_api_key: the API key for ``provider_base_url``; ignored
                without it. Omitted, the provider's standard environment
                variable supplies the key.
            output_type: a callable class used to build ``AgentRun.parsed``,
                typically a ``pydantic.BaseModel`` subclass (built with
                ``model_validate_json``) or any class accepting the output
                fields as keyword arguments. It does not add a response
                schema; use ``Prompt(output=...)`` for that.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` for invalid labels,
                annotations, a session without callable ``recent``/``append``,
                or a non-callable ``output_type``;
                ``WYRD_TOOL_400_INVALID_SCHEMA`` for an undecorated tool;
                ``WYRD_AGENT_404_PROMPT_CARD`` for an unresolvable Prompt
                reference.
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

        Raises:
            WyrdError: ``WYRD_AGENT_404_RUNTIME_LOCAL_TOOL_NOT_FOUND`` for an
                unregistered tool, ``WYRD_AGENT_404_PROMPT_CARD`` for an
                unresolvable Prompt reference, or the IO or parse error.
        """
        ...

    def to_yaml_string(self) -> str:
        """Return this Agent Card as YAML text; raises as ``save()``."""
        ...

    def to_card(self) -> JsonDict:
        """Return this Agent Card as a JSON-compatible mapping; raises as ``save()``."""
        ...

    def model_dump_json(self) -> str:
        """Return this Agent Card as JSON text; raises as ``save()``."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> Agent:
        """Build an Agent from Agent Card JSON text.

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
        """Replace every runtime-local tool in place; raises as ``add_tool()``."""
        ...

    def with_prompt(self, prompt: Prompt) -> None:
        """Replace the Prompt in place.

        Unlike ``Agent(prompt=)``, only a ``Prompt`` instance is accepted.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` when ``prompt`` is
                not a ``Prompt``.
        """
        ...

    def with_session(self, session: SessionMemory) -> None:
        """Replace the session memory in place.

        Raises:
            WyrdError: ``WYRD_AGENT_422_INVALID_ARGUMENT`` when ``session``
                lacks callable ``recent`` and ``append`` methods.
        """
        ...

    def with_run_config(self, run_config: RunConfig) -> None:
        """Replace the loop limits in place."""
        ...

    def add_before_agent(self, callback: Callable[..., object]) -> None:
        """Append a callback after any registered ``before_agent_callback``.

        Callbacks of one hook run in registration order, each seeing the
        previous one's replacement; the first that raises stops the chain with
        that hook's raise behavior from ``Agent()``.
        """
        ...

    def add_after_agent(self, callback: Callable[..., object]) -> None:
        """Append an ``after_agent_callback``, chained as ``add_before_agent()``."""
        ...

    def add_before_model(self, callback: Callable[..., object]) -> None:
        """Append a ``before_model_callback``, chained as ``add_before_agent()``."""
        ...

    def add_after_model(self, callback: Callable[..., object]) -> None:
        """Append an ``after_model_callback``, chained as ``add_before_agent()``."""
        ...

    def add_before_tool(self, callback: Callable[..., object]) -> None:
        """Append a ``before_tool_callback``, chained as ``add_before_agent()``."""
        ...

    def add_after_tool(self, callback: Callable[..., object]) -> None:
        """Append an ``after_tool_callback``, chained as ``add_before_agent()``."""
        ...

    def as_tool(self, *, description: str | None = ...) -> object:
        """Return this Agent as a tool another Agent can call.

        The tool is named after ``Agent.id``, takes one ``input`` string,
        runs this Agent on it, and returns its output text. Delegation nests at
        most three levels deep. The tool is not added to any registry.

        Args:
            description: the description the calling model sees. Omitted, it
                is ``"Delegate to agent <id>"``.
        """
        ...

def tool(
    fn: Callable[..., object] | None = ...,
    *,
    name: str | None = ...,
    description: str | None = ...,
) -> object:
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
    """Authoring and run surface for a DAG of Agent steps.

    Each step is identified by its Agent's ``name``, or its ``id`` when the
    Agent is unnamed.
    """

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
            name: the Workflow Card name.
            version: the Workflow Card version. Omitted, it stays unset until
                ``set_version()``.
            space: the Workflow Card space. Omitted, it stays unset until
                ``set_space()``.
            labels: queryable Workflow Card labels.
            annotations: free-form Workflow Card annotations.
            observers: ``Observer`` instances that receive this workflow's
                lifecycle events, called in list order. Omitted, no events are
                delivered.

        Raises:
            WyrdError: when a label, annotation, or observer is invalid; every
                observer must be an ``Observer`` subclass instance.
        """
        ...

    @staticmethod
    def sequential(
        name: str,
        *agents: Agent,
        observers: Sequence[Observer] | None = ...,
    ) -> Workflow:
        """Build a workflow whose steps run in the given order.

        Args:
            name: the Workflow Card name.
            *agents: the Agents to chain; each depends on the previous one.
            observers: as for ``Workflow()``.

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
        """Build a workflow whose steps are independent roots that run in parallel.

        Args:
            name: the Workflow Card name.
            *agents: the Agents to run.
            observers: as for ``Workflow()``.

        Raises:
            WyrdError: When the resulting DAG is invalid.
        """
        ...

    def add(self, agent: Agent) -> Workflow:
        """Append ``agent`` as a new step with no dependencies and return this workflow.

        Raises:
            WyrdError: When the resulting DAG is invalid.
        """
        ...

    def add_after(self, agent: Agent, after: Agent | str | Sequence[Agent | str]) -> Workflow:
        """Append ``agent`` as a step depending on ``after`` and return this workflow.

        Args:
            agent: the Agent to append.
            after: the predecessor steps, as step ids or Agents.

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

    def to_yaml(self) -> str:
        """Serialize this workflow to a canonical envelope YAML string.

        Raises:
            WyrdError: When identity or codec fails.
        """
        ...

    def save(self, path: PathLike) -> None:
        """Save this workflow to ``path`` as canonical envelope YAML.

        Raises:
            WyrdError: When identity, IO, or codec fails.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> Workflow:
        """Load a workflow from a YAML file, resolving inline Agents eagerly.

        Raises:
            WyrdError: When IO, codec, or resolution fails.
        """
        ...

    @staticmethod
    def from_yaml(yaml: str) -> Workflow:
        """Parse a workflow from envelope YAML text, resolving inline Agents eagerly.

        Raises:
            WyrdError: When parse or resolution fails.
        """
        ...

    def run(self, input: str | Mapping[str, Any]) -> WorkflowRun:
        """Run this workflow against the process-local provider registry.

        Args:
            input: the workflow input. A string binds the ``input`` template
                variable; a mapping binds each key as a template variable.

        Returns:
            WorkflowRun: per-step outcomes, events, and the cross-step
            parameter map.

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
