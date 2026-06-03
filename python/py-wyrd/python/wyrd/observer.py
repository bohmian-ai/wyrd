"""Observer initialization surface."""

from __future__ import annotations

from typing import Any, Protocol, runtime_checkable

from ._wyrd import _init


@runtime_checkable
class Observer(Protocol):
    def on_event(self, event: Any) -> None: ...


def init() -> None:
    _init()


__all__ = ["Observer", "init"]
