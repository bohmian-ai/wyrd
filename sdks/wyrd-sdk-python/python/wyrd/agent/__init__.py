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
    """Backend that stores and replays an agent's recent conversation turns."""

    def recent(self, session_id: str, limit: int) -> list[SessionTurn | dict[str, Any]]: ...
    def append(self, session_id: str, turn: SessionTurn) -> None: ...


class NoSession:
    """Default no-op session memory backend used when no session is configured."""

    def recent(self, session_id: str, limit: int) -> list[SessionTurn]:
        return []

    def append(self, session_id: str, turn: SessionTurn) -> None:
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
