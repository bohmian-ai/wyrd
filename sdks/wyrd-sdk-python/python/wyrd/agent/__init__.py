"""Public Agent package — Agent runtime + Workflow composition + session memory."""

from __future__ import annotations

from typing import Any, Literal, Protocol, TypedDict, runtime_checkable

from .._wyrd.agent import (
    Agent,
    AgentRun,
    FinishReason,
    Role,
    RunConfig,
    SessionTurn,
    Workflow,
    WorkflowRun,
)
from .tool import local_registry, tool


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
    "Workflow",
    "WorkflowRun",
    "WorkflowRunDict",
    "WorkflowRunError",
    "WorkflowStepResult",
    "local_registry",
    "tool",
]
