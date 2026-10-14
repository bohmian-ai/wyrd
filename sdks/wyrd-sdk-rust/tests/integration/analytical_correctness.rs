//! An analyst's subquery-heavy SQL over a seeded support workload returns
//! exactly what plain DataFusion returns over the same rows. This holds
//! whether Bifrost has published the rows, holds them live in Scribe, or
//! holds some of each.

use wyrd_ai::Bifrost;
use wyrd_ai::bifrost::TableConfig;
use wyrd_testing::bifrost::support_workload::{
    ANALYTICAL_QUERIES, SUPPORT_WORKLOAD_SEED, SupportTable, SupportWorkload,
};

use crate::support::Deployment;

/// The namespace prefix of the three support tables.
const PREFIX: &str = "vala.datasets.support_";

/// Where the workload's rows sit when the queries run.
#[derive(Debug, Clone, Copy)]
enum DataState {
    /// Every row is published.
    Published,
    /// Every row was acknowledged by Scribe and none is published.
    Live,
    /// The first half of each table is published and the rest is live.
    Mixed,
}

/// Every row published: each query matches plain DataFusion.
///
/// # Panics
/// Panics with every query whose answer differs or fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn published_rows_answer_like_datafusion() {
    answers_match_datafusion(DataState::Published).await;
}

/// Every row live in Scribe: each query matches plain DataFusion.
///
/// # Panics
/// Panics with every query whose answer differs or fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn live_rows_answer_like_datafusion() {
    answers_match_datafusion(DataState::Live).await;
}

/// Half of every table published and half live: each query matches plain
/// DataFusion.
///
/// # Panics
/// Panics with every query whose answer differs or fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn mixed_rows_answer_like_datafusion() {
    answers_match_datafusion(DataState::Mixed).await;
}

/// Writes the workload through the SDK in `state` and runs every query in
/// [`ANALYTICAL_QUERIES`] through it, comparing each answer with the oracle's.
///
/// Every query runs even after one fails, so the panic lists all mismatches.
///
/// # Panics
/// Panics when a table cannot register or a batch cannot be written, and
/// with every mismatched or failed query.
async fn answers_match_datafusion(state: DataState) {
    let deployment = Deployment::start().await;
    let admin = deployment.admin();
    let workload = SupportWorkload::generate(SUPPORT_WORKLOAD_SEED);
    let writer = Bifrost::connect(&admin).await.expect("Bifrost connects");

    let mut later = Vec::new();
    for table in SupportTable::ALL {
        let fqn = format!("{PREFIX}{}", table.name());
        writer.use_table(
            TableConfig::from_arrow(&fqn, table.schema()).expect("support table declares"),
        );
        writer.register().await.expect("support table registers");
        let batch = workload.batch(table).expect("support batch builds");
        let (now, rest) = match state {
            DataState::Published | DataState::Live => (batch.clone(), None),
            DataState::Mixed => {
                let half = batch.num_rows() / 2;
                (
                    batch.slice(0, half),
                    Some(batch.slice(half, batch.num_rows() - half)),
                )
            }
        };
        writer.write_batch(&fqn, &now).await.expect("rows write");
        later.extend(rest.map(|rest| (fqn, rest)));
    }
    if !matches!(state, DataState::Live) {
        deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
    }
    for (fqn, rest) in &later {
        writer
            .write_batch(fqn, rest)
            .await
            .expect("live rows write");
    }

    let oracle = workload.oracle().expect("oracle loads");
    let mut mismatches = Vec::new();
    for query in &ANALYTICAL_QUERIES {
        let expected = oracle.answer(query).await.expect(query.name);
        match writer.sql(&query.sql(PREFIX), &[]).await {
            Ok(result) => {
                let actual = query.rows(result.batches()).expect("rows render");
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
    assert!(
        mismatches.is_empty(),
        "{state:?}: {} of {} queries differ:\n{}",
        mismatches.len(),
        ANALYTICAL_QUERIES.len(),
        mismatches.join("\n")
    );
    deployment.shutdown().await;
}
