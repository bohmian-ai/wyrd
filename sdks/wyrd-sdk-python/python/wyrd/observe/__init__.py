"""Scoped verification and observation, and fire-and-forget Vala telemetry.

``Run`` and ``Observe`` are reached through ``WyrdState.run()``; they are not
constructed directly. ``Observe.verify`` and ``Observe.submit`` take and return
the ``wyrd.verify`` types. ``record`` below is the separate telemetry door that
swallows queue-full instead of raising.
"""

from .._wyrd.observe import Observe, Run
from ._telemetry import record

__all__ = [
    "Observe",
    "Run",
    "record",
]
