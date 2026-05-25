#### begin imports ####

from typing import Any

#### end of imports ####

class WyrdError(Exception):
    """Python-facing Wyrd error with stable metadata.

    Wyrd raises this exception for validation and boundary failures that have a
    durable Wyrd error code. The attributes are intended for both humans and
    agents: `code` is stable, `message` explains the failure, `details` carries
    structured context, and `remediation` tells the caller what to change next.
    """

    code: str
    message: str
    detail: str
    details: dict[str, Any] | None
    remediation: str

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

__all__ = [
    "WyrdError",
]
