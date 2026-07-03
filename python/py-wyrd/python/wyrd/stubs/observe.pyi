#### begin imports ####
from .bifrost import Bifrost

#### end of imports ####

def record(
    bifrost: Bifrost,
    table: str,
    schema: str,
    row: str,
    card_ref: str,
    run_id: str | None = None,
) -> None:
    """Record one telemetry observation, fire-and-forget.

    ``schema`` is JSON-Schema text; ``card_ref`` is ``space/Kind/name@version``.
    Queue-full is swallowed and counted on the handle, never raised.
    """
    ...

__all__ = ["record"]
