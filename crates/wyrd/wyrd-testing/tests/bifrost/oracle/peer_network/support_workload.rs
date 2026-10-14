//! An analyst's subquery-heavy SQL over live rows on a remote Scribe pod
//! returns exactly what plain DataFusion returns over the same rows.

use wyrd_server::config::BifrostTarget;
use wyrd_testing::bifrost::support_workload::{
    ANALYTICAL_QUERIES, SUPPORT_WORKLOAD_SEED, SupportTable, SupportWorkload,
};

use super::support::PeerJourneyError;
use crate::peer_cluster::PeerCluster;
use crate::support::{public_client, unique_table};

/// The `oracle` pod every query is submitted to; it holds no rows.
const COORDINATOR: usize = 0;

/// The only `scribe` pod, which holds every row live and unpublished.
const SCRIBE_POD: usize = 2;

/// Every row of the support workload is live on a Scribe pod that is not
/// the coordinator. Every query in [`ANALYTICAL_QUERIES`], submitted through
/// the public client to an Oracle pod, answers exactly as plain DataFusion
/// does, and reads the remote tail over the private peer plane. Only the
/// remote Scribe holds rows, so a fragment that silently dropped them would
/// show up as a differing answer.
///
/// # Panics
///
/// Panics with every query whose answer differs or fails.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn remote_live_rows_answer_like_datafusion() {
    Box::pin(prove_remote_live_rows_answer_like_datafusion())
        .await
        .expect("remote live analytical journey");
}

/// Writes the workload live on the Scribe pod and compares every answer.
///
/// # Errors
///
/// Returns the setup failure, or one message listing every query that
/// differed or failed.
async fn prove_remote_live_rows_answer_like_datafusion() -> Result<(), PeerJourneyError> {
    let cluster = PeerCluster::start(&[
        BifrostTarget::Oracle,
        BifrostTarget::Oracle,
        BifrostTarget::Scribe,
    ])
    .await?;
    let workload = SupportWorkload::generate(SUPPORT_WORKLOAD_SEED);
    let prefix = unique_table("support");
    for table in SupportTable::ALL {
        let name = format!("{prefix}_{}", table.name());
        let fields = table
            .schema()
            .fields()
            .iter()
            .map(|field| field.as_ref().clone())
            .collect();
        cluster
            .register_table_with(SCRIBE_POD, &name, fields)
            .await?;
        cluster
            .ingest_live_batch(SCRIBE_POD, &name, &workload.batch(table)?)
            .await?;
    }
    cluster.refresh_snapshots().await?;

    let api_key = cluster.provision_public_api_key("support-analyst").await?;
    let client = public_client(cluster.server(COORDINATOR)?, &api_key)?;
    let bifrost = wyrd_client::Bifrost::query_only(&client);
    let oracle = workload.oracle()?;
    let fragments_before = cluster.scribe_fragments(SCRIBE_POD)?;
    let mut mismatches = Vec::new();
    for query in &ANALYTICAL_QUERIES {
        let expected = oracle.answer(query).await?;
        match bifrost
            .sql(&query.sql(&format!("vala.bifrost.{prefix}_")), &[])
            .await
        {
            Ok(result) => {
                let actual = query.rows(result.batches())?;
                if actual != expected {
                    mismatches.push(format!(
                        "{}: Bifrost {actual:?} != DataFusion {expected:?}",
                        query.name
                    ));
                }
            }
            Err(error) => mismatches.push(format!("{}: {error}", query.name)),
        }
    }
    if !mismatches.is_empty() {
        return Err(format!(
            "{} of {} queries differ over remote live rows:\n{}",
            mismatches.len(),
            ANALYTICAL_QUERIES.len(),
            mismatches.join("\n")
        )
        .into());
    }
    if cluster.scribe_fragments(SCRIBE_POD)? <= fragments_before {
        return Err("no query read the remote Scribe's live tail".into());
    }
    cluster.shutdown().await?;
    Ok(())
}
