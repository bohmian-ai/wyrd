"""Scoped observation emits, and fire-and-forget Vala telemetry.

``Run``, ``Observe``, and ``Judgment`` are reached through ``WyrdState.run()``; they are not
constructed directly. ``record`` below is the separate telemetry door that
swallows queue-full instead of raising.
"""

from .._wyrd.observe import Judgment, Observe, Run
from ._telemetry import record

__all__ = ["Judgment", "Observe", "Run", "record"]
