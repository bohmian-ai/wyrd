"""Wyrd observer base class."""

from __future__ import annotations


class Observer:
    """Base class for Wyrd run observers.

    Subclass this to receive lifecycle events from agent and workflow runs.
    All methods have default no-op implementations - override only what you need.

    Observer methods are called concurrently when a workflow runs agents in
    parallel. Each concurrent agent run has a unique run_id, so calls for
    different runs never share the same key. If your observer maintains any
    mutable shared state (span stores, counters, buffers), protect it with
    threading.Lock or a thread-safe data structure.

    Duration values passed to on_agent_finish and on_workflow_finish are
    integer milliseconds (converted from Rust Duration before crossing the
    PyO3 boundary).
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
        """

    def on_iteration(self, run_id: str, agent_id: str, index: int) -> None:
        """Agent loop iteration started.

        Use this hook for per-iteration counters, budget tracking, and loop
        diagnostics. It fires before the model call for the iteration, so it is
        the earliest hook that can distinguish individual agent turns.
        """

    def on_model_call(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        provider: str,
        model: str,
        request: object,
    ) -> None:
        """Provider model call started.

        Use this hook to open a provider/model span or count outbound model
        requests. It receives a typed ProviderRequest wrapper.
        """

    def on_model_result(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        finish_reason: str,
        synthetic: bool,
        response: object,
    ) -> None:
        """Provider model call completed.

        Use this hook to close the model span opened by on_model_call or record
        the provider finish reason. It receives a typed ProviderResponse wrapper.
        """

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
        """

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
        """

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
        """

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
        """

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
        """

    def on_workflow_finish(
        self,
        run_id: str,
        workflow_id: str,
        duration_ms: int,
    ) -> None:
        """Workflow run finished successfully.

        Use this hook to close the workflow-level span or finalize a successful
        workflow record after all DAG steps complete.
        """
