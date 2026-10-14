//! A join over live rows held by a Scribe on another pod.

use std::sync::Arc;

use arrow::array::{Int64Array, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use serde::Deserialize;
use wyrd_server::config::BifrostTarget;

use super::support::PeerJourneyError;
use crate::peer_cluster::PeerCluster;
use crate::support::{event_time_field, public_client, unique_table};

/// The `oracle` pod the query is submitted to; it holds no rows.
const COORDINATOR: usize = 0;

/// The only `scribe` pod, which holds every row live and unpublished.
const SCRIBE_POD: usize = 2;

/// Rows in the events table, one second apart.
const EVENTS: i64 = 200;

/// The event the pick table names.
const PICKED: i64 = 107;

/// One joined result row.
#[derive(Debug, PartialEq, Eq, Deserialize)]
struct Joined {
    /// The event's id.
    id: i64,
    /// The pick row's id.
    pick: i64,
}

/// A join of two tables whose rows are all live on a remote Scribe returns
/// exactly the matching pair.
///
/// # Panics
///
/// Panics when the cluster cannot start, a table cannot register or ingest,
/// or the query fails or returns other rows.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_live_join_returns_its_rows() {
    Box::pin(prove_remote_live_join_returns_its_rows())
        .await
        .expect("remote live join journey");
}

/// Drives [`remote_live_join_returns_its_rows`].
///
/// Starts two Oracles and a Scribe, leaves an events table of [`EVENTS`]
/// rows and a one-row pick table naming event [`PICKED`] live on the Scribe,
/// and joins them on `event_time` through [`COORDINATOR`].
///
/// # Errors
///
/// Returns the setup failure or a message naming the wrong rows.
async fn prove_remote_live_join_returns_its_rows() -> Result<(), PeerJourneyError> {
    let cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;
    let start = chrono::Utc::now().timestamp_micros();
    let at = |id: i64| start + id * 1_000_000;
    let (events, pick) = (unique_table("live_events"), unique_table("live_pick"));
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        event_time_field(),
    ]));
    let ids: Vec<i64> = (0..EVENTS).collect();
    let times = ids.iter().map(|id| at(*id)).collect();
    for (table, ids, times) in [(&events, ids, times), (&pick, vec![0], vec![at(PICKED)])] {
        cluster
            .register_table_with(
                SCRIBE_POD,
                table,
                schema
                    .fields()
                    .iter()
                    .map(|field| field.as_ref().clone())
                    .collect(),
            )
            .await?;
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(TimestampMicrosecondArray::from(times).with_timezone("UTC")),
            ],
        )?;
        cluster.ingest_live_batch(SCRIBE_POD, table, &batch).await?;
    }
    cluster.refresh_snapshots().await?;

    let api_key = cluster.provision_public_api_key("live-join").await?;
    let client = public_client(cluster.server(COORDINATOR)?, &api_key)?;
    let rows: Vec<Joined> = wyrd_client::Bifrost::query_only(&client)
        .sql_as(
            &format!(
                "SELECT e.id, p.id AS pick FROM vala.bifrost.{events} e \
                 JOIN vala.bifrost.{pick} p ON e.event_time = p.event_time"
            ),
            &[],
        )
        .await?;
    let expected = vec![Joined {
        id: PICKED,
        pick: 0,
    }];
    if rows != expected {
        return Err(format!("the join returned {rows:?}, not {expected:?}").into());
    }
    cluster.shutdown().await?;
    Ok(())
}
