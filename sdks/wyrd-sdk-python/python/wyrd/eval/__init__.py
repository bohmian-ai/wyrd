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

    Pass a list of these as ``Observe.eval(media=...)``. Only this small
    descriptor is queued; the server reads the object at judge time. The
    fields are checked when the observation is emitted, not on construction.

    Attributes:
        id: the name of an existing ``${media:id}`` slot in the resolved judge
            Prompt.
        kind: ``"image"`` or ``"document"``, stated explicitly rather than
            inferred from the filename.
        uri: the durable object-storage URI of the media; never sent to the
            provider as prompt text.
        media_type: the IANA media type, such as ``"image/png"``. ``None``
            (the default) leaves it unstated.
    """

    id: str
    kind: MediaKind
    uri: str
    media_type: str | None = None


__all__ = ["MediaKind", "MediaRef"]
