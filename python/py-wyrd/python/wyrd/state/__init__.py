"""Offline, fully hydrated Wyrd Card graph and typed holder access.

Load a complete local Service bundle with no registry or network access.
WyrdState creates one persistent typed holder for each exact Card and shares it
across every friendly alias. Typed accessors expose those holders by alias;
artifact descriptors expose confined local paths and integrity metadata, but
never read payload bytes until the caller explicitly opens a path.
"""

from .._wyrd.state import CardEnvelope, HydratedArtifact, WyrdState

__all__ = ["CardEnvelope", "HydratedArtifact", "WyrdState"]
