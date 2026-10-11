"""Register a user table on its own time column and read a time range back.

A table declares ``time_column``; the column partitions the table and bounds
its files, so a time-range read returns exactly the rows inside the range.
``describe_table`` reports the declared column, a table declared without one
reports ``wyrd_ingested_at``, and a declaration naming a column that cannot be
a time column is refused naming the field.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING

import pyarrow
import pytest
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

NAMESPACE = "vala.datasets"
READINGS = "py_time_column_readings"
DEFAULTS = "py_time_column_defaults"
REFUSED = "py_time_column_refused"
FIRST_HOUR = datetime(2026, 10, 6, tzinfo=UTC)
SCHEMA = pyarrow.schema(
    [
        pyarrow.field("event_time", pyarrow.timestamp("us", tz="UTC"), nullable=False),
        pyarrow.field("value", pyarrow.int64(), nullable=False),
    ]
)


def _readings() -> pyarrow.RecordBatch:
    """One reading in each of four consecutive hours; the value is the hour index."""

    return pyarrow.record_batch(
        [
            pyarrow.array(
                [FIRST_HOUR + timedelta(hours=hour, microseconds=1) for hour in range(4)],
                pyarrow.timestamp("us", tz="UTC"),
            ),
            pyarrow.array(range(4), pyarrow.int64()),
        ],
        schema=SCHEMA,
    )


def _table(name: str, time_column: str | None) -> Bifrost:
    """A writer for ``name`` on hourly partitions of ``time_column``."""

    return Bifrost(
        TableConfig.from_arrow(
            SCHEMA, f"{NAMESPACE}.{name}", partition_granularity="hour", time_column=time_column
        )
    )


@pytest.mark.integration
def test_user_table_time_column_prunes_a_time_range(wyrd_server: WyrdTestServer) -> None:
    readings = _table(READINGS, "event_time")
    readings.register()
    readings.write_batch(f"{NAMESPACE}.{READINGS}", _readings())
    readings.flush()
    wyrd_server.flush_bifrost()
    defaults = _table(DEFAULTS, None)
    defaults.register()

    lower = FIRST_HOUR + timedelta(hours=1)
    upper = FIRST_HOUR + timedelta(hours=3)
    in_range = (
        readings.sql(
            f"SELECT value FROM {NAMESPACE}.{READINGS} "
            f"WHERE event_time >= TIMESTAMP '{lower.isoformat()}' "
            f"AND event_time < TIMESTAMP '{upper.isoformat()}' ORDER BY value"
        )
        .to_arrow()
        .column("value")
        .to_pylist()
    )

    assert in_range == [1, 2]
    assert readings.describe_table(NAMESPACE, READINGS)["physical_layout"]["time_column"] == (
        "event_time"
    )
    assert defaults.describe_table(NAMESPACE, DEFAULTS)["physical_layout"]["time_column"] == (
        "wyrd_ingested_at"
    )


@pytest.mark.integration
def test_an_invalid_time_column_is_refused_naming_the_field(
    wyrd_server: WyrdTestServer,
) -> None:
    with pytest.raises(WyrdError) as refused:
        _table(REFUSED, "value").register()

    assert refused.value.code == "WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT"
    assert "TimeColumn" in str(refused.value)
