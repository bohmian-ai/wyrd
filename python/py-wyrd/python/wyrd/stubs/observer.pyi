#### begin imports ####
from __future__ import annotations

from wyrd.stubs.prompt import ProviderRequest, ProviderResponse

#### end of imports ####

class Observer:
    """Base class for Wyrd run observers.

    Subclass and override any methods you want to observe. All methods
    have default no-op implementations.

    Observer methods may be called concurrently from parallel workflow
    steps. Protect any shared mutable state with threading.Lock.

    duration_ms values are integer milliseconds.
    """

    def on_agent_start(
        self,
        run_id: str,
        parent_run_id: str | None,
        agent_id: str,
        input: str,
        session_id: str | None,
    ) -> None:
        """Agent run started.

        Use this hook to open an agent-level span or run record before any
        iteration, model call, or tool call is observed. When parent_run_id is
        present, use it to attach this agent run to the workflow run that
        scheduled it.

        Args:
            run_id (str): Unique identifier for this agent run.
            parent_run_id (str | None): Parent workflow run_id, or None.
            agent_id (str): Agent identifier.
            input (str): User input text.
            session_id (str | None): Session identifier, or None.
        """
        ...

    def on_iteration(self, run_id: str, agent_id: str, index: int) -> None:
        """Agent loop iteration started.

        Use this hook for per-iteration counters, budget tracking, and loop
        diagnostics. It fires before the model call for the iteration, so it is
        the earliest hook that can distinguish individual agent turns.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            index (int): Zero-based iteration index.
        """
        ...

    def on_model_call(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        provider: str,
        model: str,
        request: ProviderRequest,
    ) -> None:
        """Provider model call started.

        Use this hook to open a provider/model span or count outbound model
        requests. It fires immediately before the provider request is executed.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            iteration (int): Loop iteration index.
            provider (str): Provider name (e.g. "openai", "anthropic").
            model (str): Model name.
            request (ProviderRequest): Typed provider request wrapper.
        """
        ...

    def on_model_result(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        finish_reason: str,
        synthetic: bool,
        response: ProviderResponse,
    ) -> None:
        """Provider model call completed.

        Use this hook to close the model span opened by on_model_call or record
        the provider finish reason. When synthetic is True, the result came from
        runtime synthesis rather than a normal provider response.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            iteration (int): Loop iteration index.
            finish_reason (str): Provider finish reason string.
            synthetic (bool): True when the response was synthesized by a callback.
            response (ProviderResponse): Typed provider response wrapper.
        """
        ...

    def on_tool_call(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        call_id: str,
        tool_name: str,
    ) -> None:
        """Tool invocation started.

        Use this hook to open a tool span, count tool usage, or correlate a
        provider tool call with the later on_tool_result event using call_id.
        Tool arguments are intentionally not exposed through this hook.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            iteration (int): Loop iteration index.
            call_id (str): Provider-assigned tool call identifier.
            tool_name (str): Name of the tool being invoked.
        """
        ...

    def on_tool_result(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        call_id: str,
        ok: bool,
    ) -> None:
        """Tool invocation completed.

        Use this hook to close the tool span opened by on_tool_call and record
        whether the invocation succeeded. Tool result payloads are intentionally
        not exposed through this hook.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            iteration (int): Loop iteration index.
            call_id (str): Provider-assigned tool call identifier.
            ok (bool): True when the tool returned without error.
        """
        ...

    def on_agent_finish(
        self,
        run_id: str,
        agent_id: str,
        finish_reason: str,
        iterations: int,
        duration_ms: int,
    ) -> None:
        """Agent run finished successfully.

        Use this hook to close an agent-level span or finalize a successful run
        record. Failed runs use on_agent_error instead.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            finish_reason (str): Why the run terminated.
            iterations (int): Total iterations consumed.
            duration_ms (int): Wall-clock milliseconds for the run.
        """
        ...

    def on_agent_error(
        self,
        run_id: str,
        agent_id: str,
        code: str,
        message: str,
    ) -> None:
        """Agent run failed.

        Use this hook to close a failed agent span, increment error metrics, or
        record the stable Wyrd error code. The message is diagnostic text and
        should not be used as a durable classifier.

        Args:
            run_id (str): Agent run identifier.
            agent_id (str): Agent identifier.
            code (str): Stable Wyrd error code.
            message (str): Human-readable error message.
        """
        ...

    def on_workflow_start(
        self,
        run_id: str,
        workflow_id: str,
        step_count: int,
    ) -> None:
        """Workflow run started.

        Use this hook to open a workflow-level span or job record before DAG
        steps are scheduled. Child agent runs can reference this run_id through
        their parent_run_id when the runtime supplies one.

        Args:
            run_id (str): Unique identifier for this workflow run.
            workflow_id (str): Workflow identifier.
            step_count (int): Number of steps in the DAG.
        """
        ...

    def on_workflow_finish(
        self,
        run_id: str,
        workflow_id: str,
        duration_ms: int,
    ) -> None:
        """Workflow run finished successfully.

        Use this hook to close the workflow-level span or finalize a successful
        workflow record after all DAG steps complete.

        Args:
            run_id (str): Workflow run identifier.
            workflow_id (str): Workflow identifier.
            duration_ms (int): Wall-clock milliseconds for the run.
        """
        ...

class OtelObserver(Observer):
    """OTel observer that creates spans from Wyrd lifecycle events.

    Uses the globally configured OTel tracer provider. If none is configured,
    a warning is emitted and spans are silently discarded.

    Thread-safe for concurrent workflow steps.
    """

    def __init__(self, tracer: object | None = ...) -> None:
        """Create an OtelObserver.

        Args:
            tracer (object | None): Optional pre-configured OTel tracer.
                When omitted, uses trace.get_tracer("wyrd") with the global provider.
        """
        ...

__all__ = [
    "Observer",
    "OtelObserver",
]
