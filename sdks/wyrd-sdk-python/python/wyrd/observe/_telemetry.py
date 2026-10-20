"""Fire-and-forget Vala telemetry, re-exported by ``wyrd.observe``."""

from __future__ import annotations

import json
from collections.abc import Mapping
from typing import TYPE_CHECKING, Any

# The native ``_wyrd.observe`` submodule is private and has no stub.
from .._wyrd.observe import record as _record  # ty: ignore[unresolved-import]

if TYPE_CHECKING:
    from ..bifrost import Bifrost, Correlation


def record(
    bifrost: Bifrost,
    table: str,
    schema: Mapping[str, Any] | type[Any],
    row: Mapping[str, Any] | Any,
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
        schema: the table's JSON Schema mapping, or a Pydantic model class
            whose ``model_json_schema()`` is used.
        row: the row as a mapping or a Pydantic model instance.
        correlation: optional ``card_uid`` (a declared Card UID) and
            ``run_id`` stamped on the row. Omitted, the row is uncorrelated.

    Returns:
        ``None``; a refused row is counted on ``bifrost.dropped``.

    Raises:
        WyrdError: ``WYRD_VALA_400_SCHEMA_PARSE`` for a schema with no
            mappable columns, as ``TableConfig.from_json_schema`` raises;
            ``WYRD_SPEC_400_VALIDATION`` for an invalid ``card_uid``.
    """
    columns = schema if isinstance(schema, Mapping) else schema.model_json_schema()
    dump = getattr(row, "model_dump_json", None)
    correlation = correlation or {}
    _record(
        bifrost._native,
        table,
        json.dumps(dict(columns)),
        str(dump()) if callable(dump) else json.dumps(dict(row)),
        correlation.get("card_uid"),
        correlation.get("run_id"),
    )
