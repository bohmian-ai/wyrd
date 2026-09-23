# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from typing import Literal

#### end of imports ####

MediaKind = Literal["image", "document"]

class MediaRef:
    """One media item an Eval observation names for its judge Prompt.

    ``id`` names an existing ``${media:id}`` variable in the resolved judge
    Prompt; ``kind`` selects a supported media kind rather than inferring it
    from the filename. ``uri`` is a durable object-storage locator, never
    provider-facing prompt text.
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
    ) -> None: ...

__all__ = ["MediaKind", "MediaRef"]
