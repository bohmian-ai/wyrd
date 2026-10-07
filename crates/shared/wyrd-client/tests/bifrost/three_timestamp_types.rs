//! Record orders with the three Wyrd timestamp types and read them back.
//!
//! An order has a store opening time (`TIMESTAMP_NTZ`, a wall-clock reading),
//! the moment the server received it (`TIMESTAMP_LTZ`, one instant), and the
//! moment the customer submitted it in their own zone (`TIMESTAMP_TZ`, the
//! instant plus the customer's wall clock). A table declared with Wyrd types
//! and one declared with chrono's own types both go through the Wyrd types,
//! and the customer's local hour is queryable.

use std::sync::Arc;

use arrow::array::{Array, RecordBatch, TimestampMicrosecondArray};
use arrow::compute::concat_batches;
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use wyrd_client::bifrost::{
    Bifrost, Correlation, TableConfig, TimestampLtz, TimestampNtz, TimestampTz,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{DataTypeSpec, TimeUnit as WireTimeUnit, UTC_TIME_ZONE};
use wyrd_testing::server::WyrdTestServer;

use crate::pg_tests::admin_client;

/// The table orders declared with Wyrd types are written to.
const WYRD_ORDERS: &str = "vala.datasets.wyrd_orders";
/// The table orders declared with chrono types are written to.
const CHRONO_ORDERS: &str = "vala.datasets.chrono_orders";
/// The table Arrow orders are written to.
const ARROW_ORDERS: &str = "vala.datasets.arrow_orders";

/// An order declared with Wyrd timestamp types.
#[derive(Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
struct WyrdOrder {
    /// The customer the order came from.
    order_id: String,
    /// When the store opens, on its own wall clock.
    store_opens: TimestampNtz,
    /// When the server received the order.
    received_at: TimestampLtz,
    /// When the customer submitted the order, in the customer's zone.
    submitted_at: TimestampTz,
}

/// The same order declared with chrono's own time types.
#[derive(Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
struct ChronoOrder {
    /// The customer the order came from.
    order_id: String,
    /// When the store opens, on its own wall clock.
    store_opens: NaiveDateTime,
    /// When the server received the order.
    received_at: DateTime<Utc>,
}

/// The moment every order was received.
fn received() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 6, 17, 0, 0)
        .single()
        .expect("the moment is one instant")
}

/// The store's opening wall-clock reading.
fn store_opens() -> NaiveDateTime {
    received().naive_utc() - chrono::Duration::hours(8)
}

/// Each customer and the zone they submit from.
fn customers() -> [(&'static str, FixedOffset); 2] {
    let east = |hours| FixedOffset::east_opt(hours * 3600).expect("a valid offset");
    [("chicago", east(-5)), ("tokyo", east(9))]
}

/// Register `table` from `T` and write `rows` as JSON, then publish them.
///
/// # Panics
///
/// Panics when registration, an insert, the flush, or publication fails.
async fn orders<T: schemars::JsonSchema, R: Serialize>(
    srv: &WyrdTestServer,
    table: &str,
    rows: &[R],
) -> Bifrost {
    let client = admin_client(srv, "three-timestamp-types").await;
    let orders = Bifrost::connect_with_table(
        &client,
        TableConfig::from_model::<T>(table).expect("the model declares a table"),
    )
    .await
    .expect("the writer connects");
    orders.register().await.expect("the table registers");
    for row in rows {
        orders
            .insert(
                serde_json::to_vec(row).expect("the row serializes"),
                Correlation::default(),
            )
            .expect("the row is admitted");
    }
    orders.flush().await.expect("the rows flush");
    srv.flush_bifrost().await.expect("the rows publish");
    orders
}

/// One Wyrd-typed order per customer.
async fn wyrd_orders(srv: &WyrdTestServer) -> Bifrost {
    let rows = customers().map(|(customer, zone)| WyrdOrder {
        order_id: customer.to_owned(),
        store_opens: TimestampNtz(store_opens()),
        received_at: TimestampLtz(received()),
        submitted_at: TimestampTz(received().with_timezone(&zone)),
    });
    orders::<WyrdOrder, _>(srv, WYRD_ORDERS, &rows).await
}

/// Wyrd timestamps read back as Wyrd values: the wall clock, the instant, and
/// the instant in each customer's offset.
///
/// # Panics
///
/// Panics when a value reads back with another reading, instant, or offset.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn wyrd_timestamps_read_back_as_wyrd_values() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let orders = wyrd_orders(&srv).await;

    let read: Vec<WyrdOrder> = orders
        .sql_as(&format!("SELECT * FROM {WYRD_ORDERS} ORDER BY order_id"))
        .await
        .expect("the orders read back");

    assert_eq!(read.len(), 2);
    for (order, (customer, zone)) in read.iter().zip(customers()) {
        assert_eq!(order.order_id, customer);
        assert_eq!(order.store_opens, TimestampNtz(store_opens()));
        assert_eq!(order.received_at, TimestampLtz(received()));
        assert_eq!(order.submitted_at, TimestampTz(received().into()));
        assert_eq!(order.submitted_at.0.offset(), &zone);
    }
    srv.shutdown().await.expect("server shutdown");
}

/// The customer's local hour and the UTC hour are both queryable.
///
/// # Panics
///
/// Panics when an hour differs from the customer's wall clock or UTC.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn the_customers_local_hour_is_queryable() {
    /// One order's hours.
    #[derive(Debug, PartialEq, Deserialize)]
    struct Hours {
        /// The customer.
        order_id: String,
        /// The hour on the customer's wall clock.
        local_hour: i32,
        /// The hour in UTC.
        utc_hour: i32,
    }
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let orders = wyrd_orders(&srv).await;

    let hours: Vec<Hours> = orders
        .sql_as(&format!(
            "SELECT order_id, CAST(date_part('hour', submitted_at['local']) AS INT) AS local_hour, \
             CAST(date_part('hour', submitted_at['utc']) AS INT) AS utc_hour \
             FROM {WYRD_ORDERS} ORDER BY order_id"
        ))
        .await
        .expect("the hours read back");

    assert_eq!(
        hours,
        [("chicago", 12), ("tokyo", 2)].map(|(order_id, local_hour)| Hours {
            order_id: order_id.to_owned(),
            local_hour,
            utc_hour: 17,
        })
    );
    srv.shutdown().await.expect("server shutdown");
}

/// chrono's own types declare Wyrd timestamp columns and read back as the
/// same reading and instant; a reading with no zone is refused for an
/// instant, and an instant is refused for a wall-clock column.
///
/// # Panics
///
/// Panics when a column has another type, a value reads back changed, or a
/// refused row is admitted or refused with another code.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn chrono_timestamps_go_through_wyrd_types() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let rows = customers().map(|(customer, zone)| {
        serde_json::json!({
            "order_id": customer,
            "store_opens": store_opens(),
            "received_at": received().with_timezone(&zone),
        })
    });
    let orders = orders::<ChronoOrder, _>(&srv, CHRONO_ORDERS, &rows).await;

    let described = orders.describe(CHRONO_ORDERS).await.expect("describes");
    let read: Vec<ChronoOrder> = orders
        .sql_as(&format!("SELECT * FROM {CHRONO_ORDERS} ORDER BY order_id"))
        .await
        .expect("the orders read back");
    let refused = |row: serde_json::Value| {
        let refused = orders
            .insert(
                serde_json::to_vec(&row).expect("the row serializes"),
                Correlation::default(),
            )
            .expect_err("the row is refused");
        WyrdError::from(&refused).code()
    };

    let column = |name: &str| {
        described
            .user_fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.data_type.clone())
    };
    let timestamp = |tz: Option<&str>| DataTypeSpec::Timestamp {
        unit: WireTimeUnit::Microsecond,
        tz: tz.map(str::to_owned),
    };
    assert_eq!(column("store_opens"), Some(timestamp(None)));
    assert_eq!(column("received_at"), Some(timestamp(Some(UTC_TIME_ZONE))));
    assert_eq!(
        read,
        customers().map(|(customer, _)| ChronoOrder {
            order_id: customer.to_owned(),
            store_opens: store_opens(),
            received_at: received(),
        })
    );
    assert_eq!(
        refused(serde_json::json!({
            "order_id": "naive",
            "store_opens": store_opens(),
            "received_at": received().naive_utc(),
        })),
        "WYRD_VALA_400_SCHEMA_PARSE"
    );
    assert_eq!(
        refused(serde_json::json!({
            "order_id": "zoned",
            "store_opens": "2026-10-06T09:00:00-05:00",
            "received_at": received(),
        })),
        "WYRD_VALA_400_SCHEMA_PARSE"
    );
    srv.shutdown().await.expect("server shutdown");
}

/// A one-row `ARROW_ORDERS` batch whose receipt time is labelled `zone`, or
/// naive.
///
/// # Panics
///
/// Panics when the batch cannot be built.
fn arrow_order(zone: Option<&str>) -> RecordBatch {
    RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new(
            "received_at",
            DataType::Timestamp(TimeUnit::Microsecond, zone.map(Into::into)),
            true,
        )])),
        vec![Arc::new(
            TimestampMicrosecondArray::from(vec![received().timestamp_micros()])
                .with_timezone_opt(zone),
        )],
    )
    .expect("the order batch builds")
}

/// An Arrow instant labelled with any zone, in the declaration or in a
/// write, is stored as the one UTC instant, and a naive Arrow time is refused
/// for the instant column.
///
/// # Panics
///
/// Panics when the column reads back with another type or instant, or the
/// naive batch is accepted or refused with another code.
#[tokio::test]
#[ignore = "requires the controlled Postgres journey harness"]
async fn an_arrow_instant_in_any_zone_is_one_instant_and_a_naive_one_is_refused() {
    let srv = WyrdTestServer::start_bound().await.expect("server starts");
    let client = admin_client(&srv, "three-timestamp-types").await;
    let orders = Bifrost::connect_with_table(
        &client,
        TableConfig::from_arrow(ARROW_ORDERS, arrow_order(Some("America/New_York")).schema())
            .expect("the schema declares a table"),
    )
    .await
    .expect("the writer connects");
    orders.register().await.expect("the table registers");
    for zone in ["UTC", "America/Chicago", "Asia/Tokyo"] {
        orders
            .write_batch(ARROW_ORDERS, &arrow_order(Some(zone)))
            .await
            .expect("the batch writes");
    }
    srv.flush_bifrost().await.expect("the rows publish");

    let read = orders
        .sql(&format!("SELECT received_at FROM {ARROW_ORDERS}"))
        .await
        .expect("the orders read back");
    let read = concat_batches(read.schema(), read.batches()).expect("one read batch");
    let refused = orders
        .write_batch(ARROW_ORDERS, &arrow_order(None))
        .await
        .expect_err("a naive time is refused");

    let received_at = read
        .column(0)
        .as_any()
        .downcast_ref::<TimestampMicrosecondArray>()
        .expect("received_at is a microsecond timestamp");
    assert_eq!(received_at.timezone(), Some(UTC_TIME_ZONE));
    assert_eq!(
        received_at.values().to_vec(),
        vec![received().timestamp_micros(); 3]
    );
    assert_eq!(
        WyrdError::from(&refused).code(),
        "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
    );
    srv.shutdown().await.expect("server shutdown");
}
