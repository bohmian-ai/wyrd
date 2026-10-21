"""Fully hydrated Wyrd Card graph and typed holder access.

Declare an authored Card file with ``WyrdState.declare``, or load a complete
local Service bundle with no registry or network access with
``WyrdState.from_path``. WyrdState creates one persistent typed holder for each
exact Card and shares it across every friendly alias.
"""

from .._wyrd.state import CardEnvelope, WyrdState
from ._hydrated import HydratedCard

__all__ = ["CardEnvelope", "HydratedCard", "WyrdState"]
