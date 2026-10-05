#### begin imports ####

from typing import Any

#### end of imports ####

class WyrdError(Exception):
    """Wyrd failure carrying a stable, catalog-backed error code.

    Every attribute is projected from one RFC 9457 problem document, so they
    always agree. Branch on ``code``, not on the message text. Wyrd raises the
    ``AgentError``, ``ToolError``, or ``SessionError`` subclass when the code
    starts with ``WYRD_AGENT_``, ``WYRD_TOOL_``, or ``WYRD_SESSION_`` (or the
    matching ``SKALD_`` prefix).

    Attributes:
        code: stable machine identifier, for example
            ``WYRD_CLIENT_401_NO_CREDENTIALS``.
        message: human-readable failure text.
        detail: the same text as ``message``, under its problem-document name.
        details: structured JSON context, or ``None`` when there is none.
        remediation: what the caller should change before retrying.
        status: the HTTP status the code maps to.
        title: short catalog title for the code.
        type: problem-type URI, ``https://wyrd.dev/problems/<code>``.
    """

    code: str
    message: str
    detail: str
    details: dict[str, Any] | None
    remediation: str
    status: int
    title: str
    type: str

    def __init__(self, *args: object) -> None:
        """Create a bare Wyrd error, exactly as ``Exception(*args)``.

        Wyrd raises fully populated instances itself. Direct construction
        accepts no keyword arguments, keeps the positional arguments in
        ``args`` only, and sets none of the attributes above; call
        ``build_wyrd_error`` for a populated instance. When an Agent callback
        raises an error whose ``args`` start with a catalog ``code`` and a
        ``message``, the Agent run keeps that code.
        """
        ...

class AgentError(WyrdError):
    """``WyrdError`` for a ``WYRD_AGENT_*`` code raised by the Agent runtime."""

    def __init__(self, *args: object) -> None:
        """As ``WyrdError()``."""
        ...

class ToolError(WyrdError):
    """``WyrdError`` for a ``WYRD_TOOL_*`` code raised by tool registration or calls."""

    def __init__(self, *args: object) -> None:
        """As ``WyrdError()``."""
        ...

class SessionError(WyrdError):
    """``WyrdError`` for a ``WYRD_SESSION_*`` code raised by session memory."""

    def __init__(self, *args: object) -> None:
        """As ``WyrdError()``."""
        ...

def build_wyrd_error(
    code: str,
    message: str,
    details: dict[str, Any] | None = None,
) -> WyrdError:
    """Build a fully populated Wyrd error from a stable catalog code.

    The catalog supplies ``status``, ``title``, ``type``, and ``remediation``,
    and the code prefix selects the subclass, as when Wyrd raises the error.
    The error is returned, not raised.

    Args:
        code: a catalog code. An unknown code yields an ``AgentError`` with
            code ``WYRD_AGENT_422_VALIDATION`` whose ``details`` is
            ``{"python_error_code": code}``; the given ``details`` is dropped.
        message: human-readable failure text.
        details: JSON-compatible structured context. Omitted, ``details`` is
            ``None``.

    """
    ...

__all__ = [
    "AgentError",
    "build_wyrd_error",
    "SessionError",
    "ToolError",
    "WyrdError",
]
