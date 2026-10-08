#### begin imports ####
from typing import Literal

#### end of imports ####

MediaKind = Literal["image", "document"]

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
    media_type: str | None

    def __init__(
        self,
        id: str,
        kind: MediaKind,
        uri: str,
        media_type: str | None = None,
    ) -> None:
        """Create a media descriptor; the arguments are the attributes above.

        Args:
            id: the ``${media:id}`` slot name in the judge Prompt.
            kind: ``"image"`` or ``"document"``.
            uri: the durable object-storage URI of the media.
            media_type: the IANA media type, or ``None`` to leave it unstated.
        """
        ...

__all__ = ["MediaKind", "MediaRef"]
