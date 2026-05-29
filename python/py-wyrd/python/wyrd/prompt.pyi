from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ._native import (
        MediaRef,
        Prompt,
        ProviderRequest,
        ResponseFormat,
        WyrdError,
    )
else:
    from ._native.prompt import (
        MediaRef,
        Prompt,
        ProviderRequest,
        ResponseFormat,
        WyrdError,
    )

__all__ = [
    "MediaRef",
    "Prompt",
    "ProviderRequest",
    "ResponseFormat",
    "WyrdError",
]
