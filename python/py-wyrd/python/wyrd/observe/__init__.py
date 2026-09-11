"""Fire-and-forget Vala telemetry."""

from __future__ import annotations

from typing import TYPE_CHECKING

from .._wyrd.observe import record as _record

if TYPE_CHECKING:
    from ..bifrost import Bifrost, Correlation


def record(
    bifrost: Bifrost,
    table: str,
    schema: str,
    row: str,
    correlation: Correlation | None = None,
) -> None:
    """Record one telemetry observation, fire-and-forget.

    Telemetry names its own ``table`` and ``schema`` per call rather than using
    the client's active binding, so an instrumented process writes its signals
    alongside whatever the application is writing. Queue-full is swallowed and
    counted on ``bifrost.dropped``, never raised.
    """

    correlation = correlation or {}
    _record(
        bifrost._native,
        table,
        schema,
        row,
        correlation.get("card_ref"),
        correlation.get("run_id"),
    )


__all__ = ["record"]
