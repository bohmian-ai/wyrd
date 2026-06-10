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
