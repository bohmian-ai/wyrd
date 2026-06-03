"""Public Wyrd Python exceptions."""

from ._wyrd.cards.data import WyrdError


class AgentError(WyrdError):
    """Agent boundary error."""


class ToolError(WyrdError):
    """Tool boundary error."""


class SessionError(WyrdError):
    """Session boundary error."""


__all__ = ["AgentError", "SessionError", "ToolError", "WyrdError"]
