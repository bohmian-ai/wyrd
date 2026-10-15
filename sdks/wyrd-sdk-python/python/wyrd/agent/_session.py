"""Pure-Python Agent session memory and Workflow run snapshot types, re-exported by ``wyrd.agent``."""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any, Literal, Protocol, TypedDict, runtime_checkable

if TYPE_CHECKING:
    from .._wyrd import JsonDict
    from . import SessionTurn


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
        return []

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Discard ``turn``; both arguments are ignored.

        Args:
            session_id: ignored.
            turn: ignored.
        """
        return None
