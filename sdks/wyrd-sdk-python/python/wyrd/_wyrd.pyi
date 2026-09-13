# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
### header.pyi ###
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value, missing-final-newline
# ruff: noqa: F401

from __future__ import annotations

import datetime
import os
import pathlib
from collections.abc import Callable, Mapping, Sequence
from typing import Any, Protocol, TypeAlias, overload

from wyrd.observer import Observer
from wyrd.otel import OtelObserver

PathLike: TypeAlias = str | os.PathLike[str] | pathlib.Path
JsonDict: TypeAlias = dict[str, Any]
StringMap: TypeAlias = Mapping[str, str]

class CardRefLike(Protocol):
    """Object that can be represented as a Wyrd card reference.

    Implement this protocol when a Python object can provide a JSON-compatible
    CardRef mapping to a Wyrd boundary.
    """

    def to_dict(self) -> JsonDict:
        """Return a JSON-compatible card reference dictionary.

        Returns:
            JsonDict: Serialized card reference.
        """
        ...

### error.pyi ###
class WyrdError(Exception):
    """Python-facing Wyrd error with stable metadata.

    Wyrd raises this exception for validation and boundary failures that have a
    durable Wyrd error code. Every attribute is projected from one RFC 9457
    problem document, so every direct attribute agrees with that projection:
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

def build_wyrd_error(
    code: str,
    message: str,
    details: dict[str, Any] | None = None,
) -> WyrdError:
    """Build a fully populated Wyrd error from a stable catalog code.

    Pure Python helpers use this instead of constructing an exception and
    assigning a subset of its attributes, so every raised error carries the
    catalog's status, title, type, and remediation.

    Args:
        code (str): Stable Wyrd error code.
        message (str): Human-readable failure message.
        details (dict[str, Any] | None): Optional structured context.

    Returns:
        WyrdError: Exception instance carrying complete Wyrd metadata.
    """
    ...

def _init() -> None:
    """Initialize the native Wyrd extension."""
    ...

### GLOBAL EXPORTS ###
__all__ = [
    "AgentError",
    "SessionError",
    "ToolError",
    "WyrdError",
    "_init",
    "build_wyrd_error",
]
