"""Scoped observation emits, and fire-and-forget Vala telemetry.

``Run`` and ``Observe`` are reached through ``WyrdState.run()``; they are not
constructed directly. ``record`` below is the separate telemetry door that
swallows queue-full instead of raising.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from .._wyrd.observe import Observe, Run
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
    alongside whatever the application writes. If the producer refuses the row,
    for example because its queue is full, the row is dropped and counted on
    ``bifrost.dropped`` instead of raising.

    Args:
        bifrost: the ``Bifrost`` client whose producers carry the row.
        table: the destination table name.
        schema: the table's JSON Schema text, mapped to the Arrow schema of
            the row.
        row: the row as JSON object text.
        correlation: optional ``card_ref`` (``space/Kind/name@version``) and
            ``run_id`` stamped on the row. Omitted, the row is uncorrelated.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for malformed or unsupported
            ``schema`` text or an invalid ``card_ref``.
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


__all__ = ["Observe", "Run", "record"]
