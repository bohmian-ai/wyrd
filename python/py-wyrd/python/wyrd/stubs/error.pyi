#### begin imports ####

from typing import Any

#### end of imports ####

class WyrdError(Exception):
    """Python-facing Wyrd error with stable metadata.

    Wyrd raises this exception for validation and boundary failures that have a
    durable Wyrd error code. Every attribute is projected from one RFC 9457
    problem document, so `problem` and the direct attributes always agree:
    `code` is stable, `message` and `detail` carry the same human-readable
    failure text, `details` carries structured context, `status`, `title`, and
    `type` mirror the problem document, and `remediation` tells the caller what
    to change next.
    """

    code: str
    message: str
    detail: str
    details: dict[str, Any] | None
    remediation: str
    status: int
    title: str
    type: str
    problem: dict[str, Any]

    def __init__(
        self,
        code: str,
        message: str,
        *,
        details: dict[str, Any] | None = None,
        remediation: str = ...,
    ) -> None:
        """Create a Wyrd error.

        Users normally receive this from Wyrd rather than constructing it
        directly. `code` is the machine-stable identifier; `message` is the
        short human-readable failure; `details` is JSON-compatible context; and
        `remediation` is the actionable recovery hint.

        Args:
            code (str): Stable Wyrd error code.
            message (str): Human-readable failure message.
            details (dict[str, Any] | None): Optional structured context for
                the failure.
            remediation (str): Actionable recovery guidance.
        """
        ...

class AgentError(WyrdError):
    """Agent-specific Wyrd error.

    Raised for structured errors produced by Agent runtime behavior.
    """

    def __init__(
        self,
        code: str,
        message: str,
        *,
        details: dict[str, Any] | None = None,
        remediation: str = ...,
    ) -> None:
        """Create an Agent error.

        Args:
            code (str): Stable Wyrd error code.
            message (str): Human-readable failure message.
            details (dict[str, Any] | None): Optional structured context.
            remediation (str): Actionable recovery guidance.
        """
        ...

class ToolError(WyrdError):
    """Tool-specific Wyrd error.

    Raised for structured errors produced by tool registration or invocation.
    """

    def __init__(
        self,
        code: str,
        message: str,
        *,
        details: dict[str, Any] | None = None,
        remediation: str = ...,
    ) -> None:
        """Create a Tool error.

        Args:
            code (str): Stable Wyrd error code.
            message (str): Human-readable failure message.
            details (dict[str, Any] | None): Optional structured context.
            remediation (str): Actionable recovery guidance.
        """
        ...

class SessionError(WyrdError):
    """Session-specific Wyrd error.

    Raised for structured errors produced by session memory behavior.
    """

    def __init__(
        self,
        code: str,
        message: str,
        *,
        details: dict[str, Any] | None = None,
        remediation: str = ...,
    ) -> None:
        """Create a Session error.

        Args:
            code (str): Stable Wyrd error code.
            message (str): Human-readable failure message.
            details (dict[str, Any] | None): Optional structured context.
            remediation (str): Actionable recovery guidance.
        """
        ...

__all__ = [
    "AgentError",
    "SessionError",
    "ToolError",
    "WyrdError",
]
