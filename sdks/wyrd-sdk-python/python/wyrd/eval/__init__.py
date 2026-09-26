"""Eval observation authoring types.

``MediaRef`` is the Eval media contract under an Eval-specific name so it is not
mistaken for the Prompt ``MediaRef`` in :mod:`wyrd.prompt`. It is a durable
object-storage locator, not provider-facing prompt text: the client queues the
small descriptor and the server resolves and reads the bytes at judge time.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

MediaKind = Literal["image", "document"]


@dataclass(frozen=True, slots=True)
class MediaRef:
    """One media item an Eval observation names for its judge Prompt.

    ``id`` names an existing ``${media:id}`` variable in the resolved judge
    Prompt; ``kind`` selects a supported media kind rather than inferring it
    from the filename.
    """

    id: str
    kind: MediaKind
    uri: str
    media_type: str | None = None


__all__ = ["MediaKind", "MediaRef"]
