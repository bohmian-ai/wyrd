from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ._wyrd import (
        Agent,
    )
else:
    from ._wyrd.agent import (
        Agent,
    )

__all__ = [
    "Agent",
]
