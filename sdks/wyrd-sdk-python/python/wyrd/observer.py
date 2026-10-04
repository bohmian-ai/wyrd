"""Wyrd observer base class."""

from __future__ import annotations


class Observer:
    """Base class for Wyrd run observers.

    Subclass this and pass instances as ``Workflow(observers=[...])`` to
    receive lifecycle events from that workflow's runs and the Agent runs of
    its steps. Every hook is a no-op by default, so override only what you
    need.

    Hooks run on a worker thread, and the run waits for each hook to return,
    so keep them fast. An exception raised by a hook is ignored. Hooks for
    parallel steps run concurrently; each Agent run has its own ``run_id``, but
    protect any state shared across runs with ``threading.Lock``.

    ``run_id`` values are UUIDv7 strings; ``agent_id`` is the ``Agent.id``;
    ``iteration`` is the zero-based loop iteration; ``duration_ms`` is integer
    wall-clock milliseconds.
    """

    def on_agent_start(
        self,
        run_id: str,
        parent_run_id: str | None,
        agent_id: str,
        input: str,
        session_id: str | None,
    ) -> None:
        """Called when an Agent run starts, before its first iteration.

        Each step attempt of a workflow is a separate Agent run.

        Args:
            run_id: this Agent run's id.
            parent_run_id: the ``run_id`` of the workflow run that scheduled
                this Agent run, or ``None`` when there is none.
            agent_id: the Agent's runtime id.
            input: the user input text; ``""`` for workflow steps, which run
                their rendered Prompt instead.
            session_id: the run's session id; ``None`` for workflow steps.
        """

    def on_iteration(self, run_id: str, agent_id: str, index: int) -> None:
        """Called at the start of each loop iteration, before its model call.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            index: the zero-based iteration index.
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
        """Called immediately before a model call.

        When a ``before_model`` callback aborts the run, this hook and
        ``on_model_result`` still fire although no request is sent.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            iteration: the zero-based iteration index.
            provider: ``"openai"``, ``"anthropic"``, ``"google"``,
                ``"vertex"``, or a custom provider's name.
            model: the requested model name.
            request: the outbound request, after ``before_model`` callbacks.
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
        """Called when a model call completes or fails.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            iteration: the zero-based iteration index.
            finish_reason: the provider's finish reason (``"other"`` when it
                gave none); ``"callback_aborted"`` when ``before_model``
                aborted; ``"provider_error:<message>"`` when the call failed.
            synthetic: ``True`` when no provider response exists, that is for
                the callback-aborted and provider-error cases.
            response: the provider response; an empty placeholder when
                ``synthetic`` is ``True``.
        """

    def on_tool_call(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        call_id: str,
        tool_name: str,
    ) -> None:
        """Called before a tool runs; the tool's arguments are not exposed.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            iteration: the zero-based iteration index.
            call_id: the provider-assigned tool call id, matching the later
                ``on_tool_result``.
            tool_name: the name of the tool being called.
        """

    def on_tool_result(
        self,
        run_id: str,
        agent_id: str,
        iteration: int,
        call_id: str,
        ok: bool,
    ) -> None:
        """Called after a tool call finishes; the result payload is not exposed.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            iteration: the zero-based iteration index.
            call_id: the provider-assigned tool call id from ``on_tool_call``.
            ok: ``False`` when the tool call failed, for example by raising,
                and no ``after_tool`` callback replaced the result. A call
                skipped by a raising ``before_tool`` callback reports ``True``.
        """

    def on_agent_finish(
        self,
        run_id: str,
        agent_id: str,
        finish_reason: str,
        iterations: int,
        duration_ms: int,
    ) -> None:
        """Called when an Agent run returns a result; failed runs call ``on_agent_error``.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            finish_reason: ``"modelstopped"`` or ``"callbackaborted"``.
            iterations: the number of iterations the run used.
            duration_ms: the run's wall-clock duration.
        """

    def on_agent_error(
        self,
        run_id: str,
        agent_id: str,
        code: str,
        message: str,
    ) -> None:
        """Called when an Agent run fails, for example by timeout or provider error.

        Args:
            run_id: the Agent run's id.
            agent_id: the Agent's runtime id.
            code: the stable Skald agent error code, such as
                ``"SKALD_AGENT_504_TIMEOUT"``; use it, not ``message``, to
                classify failures.
            message: human-readable diagnostic text.
        """

    def on_workflow_start(
        self,
        run_id: str,
        workflow_id: str,
        step_count: int,
    ) -> None:
        """Called when a workflow run starts, before any step is scheduled.

        Args:
            run_id: this workflow run's id; its step runs receive it as
                ``parent_run_id``.
            workflow_id: the workflow's name.
            step_count: the number of steps in the DAG.
        """

    def on_workflow_finish(
        self,
        run_id: str,
        workflow_id: str,
        duration_ms: int,
    ) -> None:
        """Called when every step of a workflow run has completed.

        A failed workflow run raises from ``Workflow.run()`` without calling
        this hook.

        Args:
            run_id: the workflow run's id.
            workflow_id: the workflow's name.
            duration_ms: the workflow run's wall-clock duration.
        """
