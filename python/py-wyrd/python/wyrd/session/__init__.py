"""Agent session typing package."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable

from .._wyrd.agent import Role, SessionTurn


@runtime_checkable
class SessionMemory(Protocol):
    def recent(self, session_id: str, limit: int) -> list[SessionTurn | dict[str, Any]]: ...
    def append(self, session_id: str, turn: SessionTurn) -> None: ...


class NoSession:
    def recent(self, session_id: str, limit: int) -> list[SessionTurn]:
        return []

    def append(self, session_id: str, turn: SessionTurn) -> None:
        return None


__all__ = ["NoSession", "Role", "SessionMemory", "SessionTurn"]
