#### begin imports ####
from .bifrost import Bifrost, Correlation

#### end of imports ####

def record(
    bifrost: Bifrost,
    table: str,
    schema: str,
    row: str,
    correlation: Correlation | None = None,
) -> None:
    """Record one telemetry observation, fire-and-forget.

    Telemetry names its own ``table`` and ``schema`` per call rather than using
    the client's active binding. ``schema`` is JSON-Schema text; ``correlation``
    optionally carries ``card_ref`` (``space/Kind/name@version``) and ``run_id``.
    Queue-full is swallowed and counted on ``bifrost.dropped``, never raised.
    """
    ...

__all__ = ["record"]
