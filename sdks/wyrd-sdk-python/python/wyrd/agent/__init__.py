"""Public Agent package — Agent runtime + Workflow composition + session memory."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable

from .._wyrd.agent import (
    Agent,
    AgentRun,
    FinishReason,
    Role,
    RunConfig,
    SessionTurn,
    StepEvent,
    StepOutcome,
    StepStatus,
    Workflow,
    WorkflowRun,
)
from .tool import local_registry, tool


@runtime_checkable
class SessionMemory(Protocol):
    """Backend that stores and replays an Agent's conversation turns per session.

    Pass an object with ``recent`` and ``append`` methods as
    ``Agent(session=...)``. The Agent uses it only for runs given a
    ``session_id``. An exception raised by either method fails the run.
    """

    def recent(self, session_id: str, limit: int) -> list[SessionTurn | dict[str, Any]]:
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
        return []

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Discard ``turn``; both arguments are ignored."""
        return None


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
