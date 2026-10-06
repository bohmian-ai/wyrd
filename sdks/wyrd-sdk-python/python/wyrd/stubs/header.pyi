# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value, missing-final-newline
# ruff: noqa: F401

from __future__ import annotations

import datetime
import os
import pathlib
from collections.abc import Callable, Mapping, Sequence
from types import TracebackType
from typing import Any, Literal, Protocol, TypeAlias, overload

PathLike: TypeAlias = str | os.PathLike[str] | pathlib.Path
JsonDict: TypeAlias = dict[str, Any]
StringMap: TypeAlias = Mapping[str, str]

def run_wyrd_cli() -> int:
    """Run the ``wyrd`` command line with ``sys.argv`` and return its exit status.

    Uses the same parser and commands as the ``wyrd`` binary; the installed
    ``wyrd`` console script calls it.
    """
    ...

class CardRefLike(Protocol):
    """Object that can be represented as a Wyrd card reference.

    Implement this protocol when a Python object can provide a JSON-compatible
    CardRef mapping to a Wyrd boundary.
    """

    def to_dict(self) -> JsonDict:
        """Return the reference as a JSON-compatible ``CardRef`` mapping."""
        ...
