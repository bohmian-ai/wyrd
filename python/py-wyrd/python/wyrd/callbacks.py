"""Agent callback helpers."""

from __future__ import annotations

import enum
from collections.abc import Callable
from typing import Any


class CallbackOutcome(enum.Enum):
    Continue = "continue"
    Skip = "skip"

    @staticmethod
    def replace_with(value: Any) -> _ReplaceWith:
        return _ReplaceWith(value)


class _ReplaceWith:
    __slots__ = ("value",)

    def __init__(self, value: Any) -> None:
        self.value = value


BeforeAgentFn = Callable[[Any, str], CallbackOutcome | None]
AfterAgentFn = Callable[[Any, Any], CallbackOutcome | None]
BeforeModelFn = Callable[[Any, Any], CallbackOutcome | None]
AfterModelFn = Callable[[Any, Any], CallbackOutcome | None]
BeforeToolFn = Callable[[Any, str, dict], CallbackOutcome | None]
AfterToolFn = Callable[[Any, str, Any], CallbackOutcome | None]


__all__ = [
    "AfterAgentFn",
    "AfterModelFn",
    "AfterToolFn",
    "BeforeAgentFn",
    "BeforeModelFn",
    "BeforeToolFn",
    "CallbackOutcome",
]
