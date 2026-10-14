//! Subqueries over live rows held by a Scribe on another pod.
//!
//! A selective scalar subquery reaches the remote Scribe as a literal bound,
//! so the fragment returns only qualifying rows; a scalar subquery over two
//! rows is refused; and a common join over two remote live tables returns its
//! exact rows.

use std::sync::Arc;

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use datafusion_proto::protobuf::PhysicalPlanNode;
use serde::Deserialize;
use wyrd_client::bifrost::BifrostClientError;
use wyrd_server::config::BifrostTarget;
use wyrd_tonic::prost::Message as _;

use super::support::PeerJourneyError;
use crate::peer_cluster::PeerCluster;
use crate::support::{event_time_field, public_client, timed_layout, unique_table};

/// The `oracle` pod every query is submitted to; it holds no rows.
const COORDINATOR: usize = 0;

/// The only `scribe` pod, which holds every row live and unpublished.
const SCRIBE_POD: usize = 2;

/// Hourly routes the events table spans.
const HOURS: i64 = 4;

/// Rows in each hour, one second apart.
const ROWS_PER_HOUR: i64 = 50;

/// One hour in microseconds.
const HOUR_MICROS: i64 = 3_600_000_000;

/// Hour and second of the one event the pick table names.
const PICKED: (i64, i64) = (2, 7);

/// Two live tables on [`SCRIBE_POD`] and the instants they were written at.
struct LiveTables {
    /// The running topology.
    cluster: PeerCluster,
    /// Events: `id` and `event_time`, [`ROWS_PER_HOUR`] rows in each of
    /// [`HOURS`] consecutive hourly routes.
    events: String,
    /// Pick: one row, `id` 0, whose `event_time` is the event at [`PICKED`].
    pick: String,
    /// Start of the events table's first hour, in microseconds.
    first_hour: i64,
}

impl LiveTables {
    /// The `event_time` of the event in `hour` at `second`, in microseconds.
    const fn at(&self, hour: i64, second: i64) -> i64 {
        self.first_hour + hour * HOUR_MICROS + second * 1_000_000
    }

    /// The `id` of the event in `hour` at `second`.
    const fn id(hour: i64, second: i64) -> i64 {
        hour * ROWS_PER_HOUR + second
    }

    /// Registers `table` on [`SCRIBE_POD`] with an `id` and an hourly
    /// `event_time`.
    ///
    /// # Errors
    ///
    /// Returns the registration failure.
    async fn register(&self, table: &str) -> Result<(), PeerJourneyError> {
        self.cluster
            .register_table_with_layout(
                SCRIBE_POD,
                table,
                schema()
                    .fields()
                    .iter()
                    .map(|field| field.as_ref().clone())
                    .collect(),
                Some(timed_layout()),
            )
            .await?;
        Ok(())
    }

    /// Writes rows with `ids` and `times` to `table` and leaves them live on
    /// [`SCRIBE_POD`].
    ///
    /// # Errors
    ///
    /// Returns the batch or ingest failure.
    async fn ingest(
        &self,
        table: &str,
        ids: Vec<i64>,
        times: Vec<i64>,
    ) -> Result<(), PeerJourneyError> {
        let batch = RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(TimestampMicrosecondArray::from(times).with_timezone("UTC")),
            ],
        )?;
        self.cluster
            .ingest_live_batch(SCRIBE_POD, table, &batch)
            .await?;
        Ok(())
    }
}

/// One `id` result row.
#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Id {
    /// The event's id.
    id: i64,
}

/// One joined result row.
#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Joined {
    /// The event's id.
    id: i64,
    /// The pick row's id.
    pick: i64,
}

/// `event_time >= H1 AND event_time = (SELECT max(event_time) FROM pick)`
/// over live rows on a remote Scribe returns the one picked event, and the
/// Scribe reads and returns no more than that.
///
/// The static conjunct prunes H0 when the query is planned; the resolved
/// scalar, an instant in H2, narrows the rest to H2 at execution. So the
/// query dispatches one fragment to the pick table's route and one to H2 —
/// none twice, none to H1 or H3 — each footer reports one row, and the H2
/// fragment carries the scalar as a literal beside the static bound.
/// Shipping the static conjunct alone would read H1 to H3 and return all
/// their rows.
///
/// # Panics
///
/// Panics when the query fails, returns other rows, or the Scribe's fragment
/// count, footer rows, or last fragment plan differ from the above.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_scalar_subquery_ships_as_a_literal() {
    Box::pin(prove_remote_scalar_subquery_ships_as_a_literal())
        .await
        .expect("remote scalar subquery journey");
}

/// Drives [`remote_scalar_subquery_ships_as_a_literal`].
///
/// # Errors
///
/// Returns the setup failure or the first claim that broke.
async fn prove_remote_scalar_subquery_ships_as_a_literal() -> Result<(), PeerJourneyError> {
    let tables = Box::pin(seed()).await?;
    let (fragments, (rows, _)) = (
        tables.cluster.scribe_fragments(SCRIBE_POD)?,
        tables.cluster.scribe_fragment_output(SCRIBE_POD)?,
    );
    let ids: Vec<Id> = query(
        &tables,
        &format!(
            "SELECT id FROM vala.bifrost.{events} \
             WHERE event_time >= '{h1}' \
             AND event_time = (SELECT max(event_time) FROM vala.bifrost.{pick})",
            events = tables.events,
            pick = tables.pick,
            h1 = rfc3339(tables.at(1, 0)),
        ),
    )
    .await?;
    let expected = vec![Id {
        id: LiveTables::id(PICKED.0, PICKED.1),
    }];
    if ids != expected {
        return Err(format!("the query returned {ids:?}, not {expected:?}").into());
    }
    let (rows_after, plan) = tables.cluster.scribe_fragment_output(SCRIBE_POD)?;
    let dispatched = tables.cluster.scribe_fragments(SCRIBE_POD)? - fragments;
    if dispatched != 2 {
        return Err(format!("the query opened {dispatched} fragments, not pick's and H2's").into());
    }
    if rows_after - rows != 2 {
        return Err(format!(
            "the fragments returned {} rows, not one each",
            rows_after - rows
        )
        .into());
    }
    let plan = format!("{:?}", PhysicalPlanNode::decode(plan.as_slice())?);
    for (name, micros) in [
        ("static bound", tables.at(1, 0)),
        ("resolved scalar", tables.at(PICKED.0, PICKED.1)),
    ] {
        if !plan.contains(&micros.to_string()) {
            return Err(format!("the H2 fragment lacks the {name} {micros}").into());
        }
    }
    tables.cluster.shutdown().await?;
    Ok(())
}

/// A scalar subquery whose input has two rows, one published and one live
/// on a remote Scribe, is refused in a filter and in a projection instead of
/// matching or multiplying outer rows.
///
/// Both rows name the picked event's instant, so the duplicate is still two
/// rows, and a filter treating the subquery as a set would return the picked
/// event.
///
/// # Panics
///
/// Panics when setup fails, or when either query returns rows or fails other
/// than at execution.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_two_row_scalar_subquery_is_refused() {
    Box::pin(prove_remote_two_row_scalar_subquery_is_refused())
        .await
        .expect("remote two-row scalar subquery journey");
}

/// Drives [`remote_two_row_scalar_subquery_is_refused`].
///
/// # Errors
///
/// Returns the setup failure, or a message naming the query that returned
/// rows or failed other than at execution.
async fn prove_remote_two_row_scalar_subquery_is_refused() -> Result<(), PeerJourneyError> {
    let tables = Box::pin(seed()).await?;
    let pair = unique_table("live_pair");
    let picked = tables.at(PICKED.0, PICKED.1);
    tables.register(&pair).await?;
    tables.ingest(&pair, vec![0], vec![picked]).await?;
    tables.cluster.flush(SCRIBE_POD).await?;
    tables.ingest(&pair, vec![1], vec![picked]).await?;
    tables.cluster.refresh_snapshots().await?;
    let api_key = tables
        .cluster
        .provision_public_api_key("live-subquery")
        .await?;
    let client = public_client(tables.cluster.server(COORDINATOR)?, &api_key)?;
    let bifrost = wyrd_client::Bifrost::query_only(&client);
    let scalar = format!("(SELECT event_time FROM vala.bifrost.{pair})");
    for sql in [
        format!(
            "SELECT id FROM vala.bifrost.{events} WHERE event_time = {scalar}",
            events = tables.events
        ),
        format!(
            "SELECT id, {scalar} AS picked FROM vala.bifrost.{events}",
            events = tables.events
        ),
    ] {
        match bifrost.sql_as::<serde_json::Value>(&sql, &[]).await {
            Ok(rows) => {
                return Err(format!("{sql} returned {} rows, not a refusal", rows.len()).into());
            }
            Err(BifrostClientError::Transport(refusal))
                if refusal.code() == "WYRD_VALA_500_QUERY_EXECUTION_FAILED" => {}
            Err(error) => {
                return Err(format!("{sql} failed other than at execution: {error}").into());
            }
        }
    }
    tables.cluster.shutdown().await?;
    Ok(())
}

/// A join of two tables whose rows are all live on a remote Scribe returns
/// exactly the matching pair.
///
/// # Panics
///
/// Panics when the query fails or returns other rows.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_live_join_returns_its_rows() {
    Box::pin(prove_remote_live_join_returns_its_rows())
        .await
        .expect("remote live join journey");
}

/// Drives [`remote_live_join_returns_its_rows`].
///
/// # Errors
///
/// Returns the setup failure or a message naming the wrong rows.
async fn prove_remote_live_join_returns_its_rows() -> Result<(), PeerJourneyError> {
    let tables = Box::pin(seed()).await?;
    let rows: Vec<Joined> = query(
        &tables,
        &format!(
            "SELECT e.id, p.id AS pick FROM vala.bifrost.{events} e \
             JOIN vala.bifrost.{pick} p ON e.event_time = p.event_time",
            events = tables.events,
            pick = tables.pick,
        ),
    )
    .await?;
    let expected = vec![Joined {
        id: LiveTables::id(PICKED.0, PICKED.1),
        pick: 0,
    }];
    if rows != expected {
        return Err(format!("the join returned {rows:?}, not {expected:?}").into());
    }
    tables.cluster.shutdown().await?;
    Ok(())
}

/// Starts two Oracles and a Scribe, and leaves both tables live on the
/// Scribe, the events table over the [`HOURS`] hours ending with the current
/// one.
///
/// # Errors
///
/// Returns the cluster, registration, or ingest failure.
async fn seed() -> Result<LiveTables, PeerJourneyError> {
    let cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;
    let now = chrono::Utc::now().timestamp_micros();
    let first_hour = now - now.rem_euclid(HOUR_MICROS) - (HOURS - 1) * HOUR_MICROS;
    let tables = LiveTables {
        cluster,
        events: unique_table("live_events"),
        pick: unique_table("live_pick"),
        first_hour,
    };
    let (ids, times): (Vec<i64>, Vec<i64>) = (0..HOURS)
        .flat_map(|hour| (0..ROWS_PER_HOUR).map(move |second| (hour, second)))
        .map(|(hour, second)| (LiveTables::id(hour, second), tables.at(hour, second)))
        .unzip();
    for (table, ids, times) in [
        (&tables.events, ids, times),
        (&tables.pick, vec![0], vec![tables.at(PICKED.0, PICKED.1)]),
    ] {
        tables.register(table).await?;
        tables.ingest(table, ids, times).await?;
    }
    tables.cluster.refresh_snapshots().await?;
    Ok(tables)
}

/// The schema of every table here: a required `id` and an `event_time`.
fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        event_time_field(),
    ]))
}

/// Renders `micros` since the epoch as an RFC 3339 UTC instant, a literal
/// `DataFusion` casts to the `event_time` column's type.
fn rfc3339(micros: i64) -> String {
    chrono::DateTime::from_timestamp_micros(micros)
        .map(|instant| instant.to_rfc3339())
        .unwrap_or_default()
}

/// Runs `sql` through the public client on [`COORDINATOR`] and returns its
/// rows as `T`.
///
/// # Errors
///
/// Returns the credential, client, query, or row conversion failure.
async fn query<T: serde::de::DeserializeOwned>(
    tables: &LiveTables,
    sql: &str,
) -> Result<Vec<T>, PeerJourneyError> {
    let api_key = tables
        .cluster
        .provision_public_api_key("live-subquery")
        .await?;
    let client = public_client(tables.cluster.server(COORDINATOR)?, &api_key)?;
    Ok(wyrd_client::Bifrost::query_only(&client)
        .sql_as(sql, &[])
        .await?)
}
