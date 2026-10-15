"""Scoped verification and observation, and fire-and-forget Vala telemetry.

``Run`` and ``Observe`` are reached through ``WyrdState.run()``; they are not
constructed directly. ``File`` and ``ArtifactRef`` name evidence for
``Observe.verify``/``Observe.submit``; ``VerificationExecution`` is a queued
execution. ``record`` below is the separate telemetry door that swallows
queue-full instead of raising.
"""

from .._wyrd.observe import (
    ArtifactRef,
    File,
    Observe,
    Recorded,
    Run,
    VerificationExecution,
    VerificationResult,
)
from ._telemetry import record

__all__ = [
    "ArtifactRef",
    "File",
    "Observe",
    "Recorded",
    "Run",
    "VerificationExecution",
    "VerificationResult",
    "record",
]
