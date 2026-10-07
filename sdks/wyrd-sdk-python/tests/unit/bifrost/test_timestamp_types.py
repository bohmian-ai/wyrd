"""Wyrd timestamp types, and every Pydantic time type converted to one."""

from __future__ import annotations

import subprocess
import sys
from datetime import datetime, timedelta, timezone

import pyarrow
import pytest
from pydantic import (
    AwareDatetime,
    BaseModel,
    FutureDatetime,
    NaiveDatetime,
    PastDatetime,
    ValidationError,
)
from wyrd.bifrost import TableConfig
from wyrd.types import TimestampLTZ, TimestampNTZ, TimestampTZ

NTZ = pyarrow.timestamp("us")
LTZ = pyarrow.timestamp("us", tz="+00:00")
TZ = pyarrow.struct(
    [pyarrow.field("utc", LTZ, nullable=False), pyarrow.field("local", NTZ, nullable=False)]
)
CHICAGO = timezone(timedelta(hours=-5))


class Order(BaseModel):
    """One field of every Wyrd and Pydantic time type."""

    store_opens: TimestampNTZ
    received_at: TimestampLTZ
    submitted_at: TimestampTZ
    plain: datetime
    aware: AwareDatetime
    past: PastDatetime
    future: FutureDatetime
    naive: NaiveDatetime


def test_wyrd_timestamp_types_need_no_third_party_library() -> None:
    probe = (
        "import sys; from datetime import datetime, timezone;"
        "from wyrd.types import TimestampLTZ, TimestampNTZ, TimestampTZ;"
        "TimestampNTZ(2026, 10, 6, 12); TimestampLTZ(2026, 10, 6, 17, tzinfo=timezone.utc);"
        "assert 'pydantic' not in sys.modules and 'pydantic_core' not in sys.modules"
    )

    subprocess.run([sys.executable, "-c", probe], check=True)


def test_each_wyrd_timestamp_type_holds_its_zone_rule() -> None:
    assert TimestampNTZ.of(datetime(2026, 10, 6, 12)) == datetime(2026, 10, 6, 12)
    assert TimestampTZ.of(datetime(2026, 10, 6, 12, tzinfo=CHICAGO)).utcoffset() == timedelta(
        hours=-5
    )
    with pytest.raises(ValueError, match="TIMESTAMP_NTZ refuses a time zone"):
        TimestampNTZ(2026, 10, 6, 12, tzinfo=CHICAGO)
    for zoned in (TimestampLTZ, TimestampTZ):
        with pytest.raises(ValueError, match="requires a time zone"):
            zoned(2026, 10, 6, 12)


def test_every_time_type_declares_its_wyrd_timestamp_column() -> None:
    schema = TableConfig(Order, "vala.datasets.orders").arrow_schema

    assert {field.name: field.type for field in schema} == {
        "store_opens": NTZ,
        "received_at": LTZ,
        "submitted_at": TZ,
        "plain": LTZ,
        "aware": LTZ,
        "past": LTZ,
        "future": LTZ,
        "naive": NTZ,
    }


def test_wyrd_fields_validate_to_wyrd_values_and_write_their_offset() -> None:
    class Submission(BaseModel):
        submitted_at: TimestampTZ
        store_opens: TimestampNTZ

    row = Submission(
        submitted_at=datetime(2026, 10, 6, 12, tzinfo=CHICAGO), store_opens=datetime(2026, 10, 6, 9)
    )

    assert type(row.submitted_at) is TimestampTZ
    assert type(row.store_opens) is TimestampNTZ
    assert (
        row.model_dump_json()
        == '{"submitted_at":"2026-10-06T12:00:00-05:00","store_opens":"2026-10-06T09:00:00"}'
    )
    with pytest.raises(ValidationError):
        Submission(submitted_at=datetime(2026, 10, 6, 12), store_opens=datetime(2026, 10, 6, 9))
