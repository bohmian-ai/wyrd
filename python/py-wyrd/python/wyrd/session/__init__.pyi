# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from __future__ import annotations

from collections.abc import Sequence
from typing import Protocol, runtime_checkable

from .._wyrd import JsonDict

#### end of imports ####

class Role:
    """Session turn role.

    Values identify whether a turn came from the user, assistant, or tool.
    """

    User: Role
    Assistant: Role
    Tool: Role

class SessionTurn:
    """One session memory turn.

    Session turns are passed between Python session memory objects and Agent runs.
    """

    def __init__(
        self,
        role: Role,
        content: str,
        *,
        tool_call_id: str | None = ...,
    ) -> None:
        """Create a session turn.

        Args:
            role (Role): Role for the turn.
            content (str): Turn content.
            tool_call_id (str | None): Optional tool call id for tool turns.
        """
        ...

    @property
    def role(self) -> Role:
        """Return the turn role.

        Returns:
            Role: Turn role.
        """
        ...

    @property
    def content(self) -> str:
        """Return the turn content.

        Returns:
            str: Turn content.
        """
        ...

    @property
    def tool_call_id(self) -> str | None:
        """Return the optional tool call id.

        Returns:
            str | None: Tool call id for tool turns.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return a JSON-compatible session turn mapping.

        Returns:
            JsonDict: Serialized session turn.
        """
        ...

@runtime_checkable
class SessionMemory(Protocol):
    """Protocol for Python session memory objects.

    Implement this protocol to provide recent and append behavior to Agent runs.
    """

    def recent(self, session_id: str, limit: int) -> Sequence[SessionTurn | JsonDict]:
        """Return recent session turns.

        Args:
            session_id (str): Session id for the run.
            limit (int): Maximum recent turns requested.

        Returns:
            Sequence[SessionTurn | JsonDict]: Recent turns.
        """
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Append one session turn.

        Args:
            session_id (str): Session id for the run.
            turn (SessionTurn): Turn to append.
        """
        ...

class NoSession:
    """No-op session memory implementation.

    Use this when an Agent should not persist session turns.
    """

    def recent(self, session_id: str, limit: int) -> list[SessionTurn]:
        """Return an empty recent-turn list.

        Args:
            session_id (str): Session id for the run.
            limit (int): Maximum recent turns requested.

        Returns:
            list[SessionTurn]: Empty list.
        """
        ...

    def append(self, session_id: str, turn: SessionTurn) -> None:
        """Ignore one session turn.

        Args:
            session_id (str): Session id for the run.
            turn (SessionTurn): Turn to ignore.
        """
        ...

__all__ = ["NoSession", "Role", "SessionMemory", "SessionTurn"]
