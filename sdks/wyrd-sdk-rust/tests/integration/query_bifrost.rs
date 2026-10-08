//! A caller reads published Bifrost rows with parameterized SQL; bound values
//! stay data, a built-in table nothing wrote reads as empty, and results
//! stream as Arrow batches closed by a terminal frame.

use arrow::array::AsArray;
use arrow::datatypes::Int64Type;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wyrd_sdk::bifrost::{Correlation, QueryParam, TableConfig};
use wyrd_sdk::{Bifrost, QueryTerminalOutcome, WyrdError};

use crate::support::Deployment;

/// The `vala.datasets` table the stories read, holding rows 1 to 3.
const QUERY_ROWS: &str = "vala.datasets.query_rows";

/// One [`QUERY_ROWS`] row, as written and as read back.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct QueryRow {
    /// The row's ordinal.
    id: i64,
    /// The row's text.
    value: String,
}

/// One `COUNT(*)` result.
#[derive(Debug, PartialEq, Deserialize)]
struct Count {
    /// The counted rows.
    n: i64,
}

/// A deployment whose administrator wrote and published rows 1 to 3 of
/// [`QUERY_ROWS`], and the administrator's query client.
///
/// # Panics
/// Panics when the table cannot register or a row cannot be written.
async fn published() -> (Deployment, Bifrost) {
    let deployment = Deployment::start().await;
    let admin = deployment.admin();
    let table = TableConfig::from_json_schema(
        QUERY_ROWS,
        &json!({
            "type": "object",
            "properties": { "id": { "type": "integer" }, "value": { "type": "string" } },
            "required": ["id", "value"],
        }),
    )
    .expect("table declares");
    let writer = Bifrost::connect_with_table(&admin, table)
        .await
        .expect("writer connects");
    writer.register().await.expect("table registers");
    for (id, value) in [(1, "one"), (2, "two"), (3, "three")] {
        let row = QueryRow {
            id,
            value: value.to_owned(),
        };
        writer
            .insert(
                serde_json::to_vec(&row).expect("row serializes"),
                Correlation::default(),
            )
            .expect("row inserts");
    }
    writer.shutdown().await.expect("writer drains");
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    (
        deployment,
        Bifrost::connect(&admin).await.expect("Bifrost connects"),
    )
}

/// A bound `$1` filters the caller's rows.
///
/// # Panics
/// Panics when the query fails or returns other rows.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn parameterized_sql_returns_the_callers_rows() {
    let (deployment, bifrost) = published().await;

    let rows: Vec<QueryRow> = bifrost
        .sql_as(
            &format!("SELECT id, value FROM {QUERY_ROWS} WHERE id >= $1 ORDER BY id"),
            &[QueryParam::Int(2)],
        )
        .await
        .expect("query runs");

    assert_eq!(
        rows,
        [
            QueryRow {
                id: 2,
                value: "two".to_owned()
            },
            QueryRow {
                id: 3,
                value: "three".to_owned()
            },
        ]
    );
    deployment.shutdown().await;
}

/// A bound value that looks like SQL matches no row instead of widening the
/// filter.
///
/// # Panics
/// Panics when the query fails or returns a row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn bound_sql_text_is_treated_as_data() {
    let (deployment, bifrost) = published().await;

    let rows: Vec<QueryRow> = bifrost
        .sql_as(
            &format!("SELECT id, value FROM {QUERY_ROWS} WHERE value = $1"),
            &[QueryParam::String("one' OR '1'='1".to_owned())],
        )
        .await
        .expect("query runs");

    assert_eq!(rows, []);
    deployment.shutdown().await;
}

/// A built-in table no one has written reads as zero rows.
///
/// # Panics
/// Panics when the query fails or counts a row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn unwritten_builtin_table_reads_as_empty() {
    let deployment = Deployment::start().await;

    let rows: Vec<Count> = Bifrost::connect(&deployment.admin())
        .await
        .expect("Bifrost connects")
        .sql_as("SELECT COUNT(*) AS n FROM vala.eval.result_items", &[])
        .await
        .expect("query runs");

    assert_eq!(rows, [Count { n: 0 }]);
    deployment.shutdown().await;
}

/// A streamed query yields its rows as Arrow batches, then a successful
/// terminal frame counting them.
///
/// # Panics
/// Panics when the stream fails, a batch lacks `id`, or the rows or the
/// terminal differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn stream_yields_arrow_batches_and_a_terminal() {
    let (deployment, bifrost) = published().await;
    let mut stream = bifrost
        .stream(
            &format!("SELECT id FROM {QUERY_ROWS} ORDER BY id"),
            &[],
            None,
        )
        .await
        .expect("query starts");

    let mut ids = Vec::new();
    while let Some(batch) = stream.next_batch().await.expect("batch decodes") {
        let column = batch.column_by_name("id").expect("batch has id");
        ids.extend(column.as_primitive::<Int64Type>().values().iter().copied());
    }

    assert_eq!(ids, [1, 2, 3]);
    let terminal = stream.terminal().expect("the stream ended with a terminal");
    assert_eq!(
        (
            terminal.outcome,
            terminal.row_count,
            terminal.warnings.len()
        ),
        (QueryTerminalOutcome::Success, 3, 0)
    );
    deployment.shutdown().await;
}

/// A caller holding only `cards:read` cannot query Bifrost.
///
/// # Panics
/// Panics when the query succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn caller_without_bifrost_read_is_refused() {
    let (deployment, _) = published().await;
    let denied = Bifrost::connect(
        &deployment.client(&deployment.scoped_key("query_denied", &["cards:read"]).await),
    )
    .await
    .expect("Bifrost connects");

    let refused = denied
        .sql(&format!("SELECT id FROM {QUERY_ROWS}"), &[])
        .await
        .expect_err("the query is refused");

    assert_eq!(
        WyrdError::from(refused).code(),
        "WYRD_PERMISSION_403_DENIED_RBAC"
    );
    deployment.shutdown().await;
}
