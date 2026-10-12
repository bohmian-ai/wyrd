"""Record orders with the three Wyrd timestamp types and read them back.

An order has a store opening time (``TIMESTAMP_NTZ``, a wall-clock reading),
the moment the server received it (``TIMESTAMP_LTZ``, one instant), and the
moment the customer submitted it in their own zone (``TIMESTAMP_TZ``, the
instant plus the customer's wall clock). A table declared with Wyrd types and
one declared with Pydantic's own time types both go through the Wyrd types:
each reads back as Wyrd values, and the customer's local hour is queryable.
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta, timezone
from typing import TYPE_CHECKING

import pyarrow
import pytest
from pydantic import BaseModel, NaiveDatetime
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.types import TimestampLTZ, TimestampNTZ, TimestampTZ

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

WYRD_ORDERS = "vala.datasets.wyrd_orders"
PYDANTIC_ORDERS = "vala.datasets.pydantic_orders"
ARROW_ORDERS = "vala.datasets.arrow_orders"

RECEIVED = datetime(2026, 10, 6, 17, 0, tzinfo=UTC)
STORE_OPENS = datetime(2026, 10, 6, 9, 0)
CUSTOMERS = {
    "chicago": timezone(timedelta(hours=-5)),
    "tokyo": timezone(timedelta(hours=9)),
}


class WyrdOrder(BaseModel):
    """An order declared with Wyrd timestamp types."""

    order_id: str
    store_opens: TimestampNTZ
    received_at: TimestampLTZ
    submitted_at: TimestampTZ


class PydanticOrder(BaseModel):
    """The same order declared with Pydantic's own time types."""

    order_id: str
    store_opens: NaiveDatetime
    received_at: datetime


def _orders(
    wyrd_server: WyrdTestServer, model: type[BaseModel], table: str, rows: list[BaseModel]
) -> Bifrost:
    """Register ``table`` from ``model``, write ``rows``, and publish them."""

    orders = Bifrost(TableConfig(model, table))
    orders.register()
    for row in rows:
        orders.insert(row)
    orders.flush()
    wyrd_server.flush_bifrost()
    return orders


@pytest.fixture(scope="module")
def wyrd_orders(wyrd_server: WyrdTestServer) -> Bifrost:
    """One order per customer zone, written with Wyrd types."""

    return _orders(
        wyrd_server,
        WyrdOrder,
        WYRD_ORDERS,
        [
            WyrdOrder(
                order_id=customer,
                store_opens=STORE_OPENS,
                received_at=RECEIVED,
                submitted_at=RECEIVED.astimezone(zone),
            )
            for customer, zone in CUSTOMERS.items()
        ],
    )


@pytest.fixture(scope="module")
def pydantic_orders(wyrd_server: WyrdTestServer) -> Bifrost:
    """One order per customer zone, written with Pydantic types in the customer's zone."""

    return _orders(
        wyrd_server,
        PydanticOrder,
        PYDANTIC_ORDERS,
        [
            PydanticOrder(
                order_id=customer, store_opens=STORE_OPENS, received_at=RECEIVED.astimezone(zone)
            )
            for customer, zone in CUSTOMERS.items()
        ],
    )


@pytest.mark.integration
def test_wyrd_timestamps_read_back_as_wyrd_values(wyrd_orders: Bifrost) -> None:
    orders = wyrd_orders.sql(f"SELECT * FROM {WYRD_ORDERS} ORDER BY order_id", WyrdOrder)

    assert [order.order_id for order in orders] == ["chicago", "tokyo"]
    for order in orders:
        zone = CUSTOMERS[order.order_id]
        assert type(order.store_opens) is TimestampNTZ
        assert type(order.received_at) is TimestampLTZ
        assert type(order.submitted_at) is TimestampTZ
        assert order.store_opens == STORE_OPENS
        assert order.received_at == RECEIVED
        assert order.received_at.utcoffset() == timedelta(0)
        assert order.submitted_at == RECEIVED
        assert order.submitted_at.utcoffset() == zone.utcoffset(None)


@pytest.mark.integration
def test_the_customers_local_hour_is_queryable(wyrd_orders: Bifrost) -> None:
    hours = (
        wyrd_orders.sql(
            f"SELECT order_id, date_part('hour', submitted_at['local']) AS local_hour, "
            f"date_part('hour', submitted_at['utc']) AS utc_hour FROM {WYRD_ORDERS} ORDER BY order_id"
        )
        .to_arrow()
        .to_pylist()
    )

    assert [(row["order_id"], row["local_hour"], row["utc_hour"]) for row in hours] == [
        ("chicago", 12, 17),
        ("tokyo", 2, 17),
    ]


@pytest.mark.integration
def test_pydantic_timestamps_go_through_wyrd_types(pydantic_orders: Bifrost) -> None:
    columns = {
        field.name: field.type for field in TableConfig.describe(PYDANTIC_ORDERS).arrow_schema
    }
    orders = pydantic_orders.sql(
        f"SELECT * FROM {PYDANTIC_ORDERS} ORDER BY order_id", PydanticOrder
    )

    assert columns["store_opens"] == pyarrow.timestamp("us")
    assert columns["received_at"] == pyarrow.timestamp("us", tz="+00:00")
    assert [order.order_id for order in orders] == ["chicago", "tokyo"]
    for order in orders:
        assert type(order.store_opens) is TimestampNTZ
        assert type(order.received_at) is TimestampLTZ
        assert order.store_opens == STORE_OPENS
        assert order.received_at == RECEIVED


@pytest.mark.integration
def test_a_reading_without_a_zone_is_refused_for_an_instant(pydantic_orders: Bifrost) -> None:
    naive = PydanticOrder(
        order_id="naive", store_opens=STORE_OPENS, received_at=datetime(2026, 10, 6, 17)
    )

    with pytest.raises(WyrdError) as refused:
        pydantic_orders.insert(naive)

    assert refused.value.code == "WYRD_VALA_400_SCHEMA_PARSE"


@pytest.mark.integration
def test_an_instant_is_refused_for_a_wall_clock_column(pydantic_orders: Bifrost) -> None:
    zoned = {
        "order_id": "zoned",
        "store_opens": "2026-10-06T09:00:00-05:00",
        "received_at": "2026-10-06T17:00:00Z",
    }

    with pytest.raises(WyrdError) as refused:
        pydantic_orders.insert(zoned)

    assert refused.value.code == "WYRD_VALA_400_SCHEMA_PARSE"


@pytest.mark.integration
def test_an_arrow_instant_in_any_zone_is_one_instant_and_a_naive_one_is_refused(
    wyrd_server: WyrdTestServer,
) -> None:
    def received(zone: str) -> pyarrow.RecordBatch:
        return pyarrow.record_batch(
            {"received_at": pyarrow.array([RECEIVED], pyarrow.timestamp("us", tz=zone))}
        )

    orders = Bifrost(TableConfig.from_arrow(received("America/New_York").schema, ARROW_ORDERS))
    orders.register()
    for zone in ["UTC", "America/Chicago", "Asia/Tokyo"]:
        orders.write_batch(ARROW_ORDERS, received(zone))
    wyrd_server.flush_bifrost()
    naive = pyarrow.record_batch(
        {"received_at": pyarrow.array([datetime(2026, 10, 6, 17)], pyarrow.timestamp("us"))}
    )

    read = orders.sql(f"SELECT received_at FROM {ARROW_ORDERS}").to_arrow().column("received_at")
    with pytest.raises(WyrdError) as refused:
        orders.write_batch(ARROW_ORDERS, naive)

    assert read.type == pyarrow.timestamp("us", tz="+00:00")
    assert read.to_pylist() == [RECEIVED] * 3
    assert refused.value.code == "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
