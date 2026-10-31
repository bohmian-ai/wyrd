//! A caller reads published Bifrost rows with parameterized SQL; bound values
//! stay data, a built-in table nothing wrote reads as empty, and results
//! stream as Arrow batches closed by a terminal frame.

use arrow::array::AsArray;
use arrow::datatypes::Int64Type;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::json;
use wyrd_ai::bifrost::BifrostOptions;
use wyrd_ai::bifrost::{Correlation, QueryParam, TableConfig};
use wyrd_ai::{Bifrost, QueryTerminalOutcome, WyrdError};
use wyrd_spec::vala::ids::RunId;

use crate::support::Deployment;

/// The `bifrost.datasets` table the stories read, holding rows 1 to 3.
const QUERY_ROWS: &str = "bifrost.datasets.query_rows";

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
    let (deployment, bifrost) = live().await;
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    (deployment, bifrost)
}

/// A deployment whose administrator wrote rows 1 to 3 of [`QUERY_ROWS`]
/// that Scribe acknowledged and nothing published, so every query reads them
/// from the live tail, and the administrator's query client. Rows 1 and 2
/// share one managed `run_id` and row 3 carries another, so a query can
/// correlate on the column Wyrd stamps.
///
/// # Panics
/// Panics when the table cannot register or a row cannot be written.
async fn live() -> (Deployment, Bifrost) {
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
    let writer = Bifrost::connect_with(
        &admin,
        BifrostOptions {
            table: Some(table),
            ..BifrostOptions::default()
        },
    )
    .await
    .expect("writer connects");
    writer.register().await.expect("table registers");
    let (shared, other) = (RunId::new(), RunId::new());
    for (id, value, run_id) in [
        (1, "one", &shared),
        (2, "two", &shared),
        (3, "three", &other),
    ] {
        let row = QueryRow {
            id,
            value: value.to_owned(),
        };
        writer
            .insert(
                serde_json::to_vec(&row).expect("row serializes"),
                Correlation {
                    subject_card_uid: None,
                    run_id: Some(run_id.clone()),
                },
            )
            .expect("row inserts");
    }
    writer.shutdown().await.expect("writer drains");
    (
        deployment,
        Bifrost::connect_with(&admin, BifrostOptions::default())
            .await
            .expect("Bifrost connects"),
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

    let rows: Vec<Count> = Bifrost::connect_with(&deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT COUNT(*) AS n FROM bifrost.verification.execution_outputs",
            &[],
        )
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
    let denied = Bifrost::connect_with(
        &deployment.client(&deployment.scoped_key("query_denied", &["cards:read"]).await),
        BifrostOptions::default(),
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

/// One `id` result.
#[derive(Debug, PartialEq, Deserialize)]
struct Id {
    /// The selected row's ordinal.
    id: i64,
}

/// Scalar, `IN`, and correlated `EXISTS` subqueries over a live table,
/// including one correlated on the managed `run_id`, return the same rows as
/// their equivalent joins.
///
/// # Panics
/// Panics when a query fails or a subquery and its join disagree.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn live_table_subqueries_match_their_joins() {
    let (deployment, bifrost) = live().await;
    let cases = [
        (
            format!(
                "SELECT id FROM {QUERY_ROWS} WHERE id = \
                 (SELECT id FROM {QUERY_ROWS} ORDER BY id DESC LIMIT 1)"
            ),
            format!(
                "SELECT a.id FROM {QUERY_ROWS} a JOIN \
                 (SELECT MAX(id) AS id FROM {QUERY_ROWS}) b ON a.id = b.id"
            ),
            vec![3],
        ),
        (
            format!(
                "SELECT id FROM {QUERY_ROWS} WHERE id IN \
                 (SELECT id FROM {QUERY_ROWS} WHERE value <> 'two') ORDER BY id"
            ),
            format!(
                "SELECT a.id FROM {QUERY_ROWS} a JOIN {QUERY_ROWS} b \
                 ON a.id = b.id WHERE b.value <> 'two' ORDER BY a.id"
            ),
            vec![1, 3],
        ),
        (
            format!(
                "SELECT a.id FROM {QUERY_ROWS} a WHERE EXISTS \
                 (SELECT 1 FROM {QUERY_ROWS} b WHERE b.id = a.id + 1) ORDER BY a.id"
            ),
            format!(
                "SELECT a.id FROM {QUERY_ROWS} a JOIN {QUERY_ROWS} b \
                 ON b.id = a.id + 1 ORDER BY a.id"
            ),
            vec![1, 2],
        ),
        (
            format!(
                "SELECT a.id FROM {QUERY_ROWS} a WHERE EXISTS \
                 (SELECT 1 FROM {QUERY_ROWS} b WHERE b.run_id = a.run_id AND b.id <> a.id) \
                 ORDER BY a.id"
            ),
            format!(
                "SELECT DISTINCT a.id FROM {QUERY_ROWS} a JOIN {QUERY_ROWS} b \
                 ON b.run_id = a.run_id AND b.id <> a.id ORDER BY a.id"
            ),
            vec![1, 2],
        ),
    ];

    for (subquery, join, expected) in cases {
        let ids = |rows: Vec<Id>| rows.into_iter().map(|row| row.id).collect::<Vec<_>>();
        let from_subquery: Vec<Id> = bifrost
            .sql_as(&subquery, &[])
            .await
            .unwrap_or_else(|error| panic!("{subquery}: {}", WyrdError::from(error)));
        let from_join: Vec<Id> = bifrost
            .sql_as(&join, &[])
            .await
            .unwrap_or_else(|error| panic!("{join}: {}", WyrdError::from(error)));
        assert_eq!(ids(from_subquery), expected, "{subquery}");
        assert_eq!(ids(from_join), expected, "{join}");
    }
    deployment.shutdown().await;
}

/// An unknown column is a 400 carrying the planner's suggestion, alone, in a
/// three-way join, and inside a subquery.
///
/// # Panics
/// Panics when a query succeeds or is refused without the suggestion.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn unknown_column_carries_the_planners_suggestion() {
    let (deployment, bifrost) = live().await;

    for sql in [
        format!("SELECT r.valeu FROM {QUERY_ROWS} r"),
        format!(
            "SELECT r.valeu, r.valeu AS again FROM {QUERY_ROWS} r JOIN {QUERY_ROWS} s ON r.id = s.id \
             JOIN bifrost.verification.results v ON v.result_id = s.value"
        ),
        format!("SELECT id FROM {QUERY_ROWS} WHERE id IN (SELECT r.valeu FROM {QUERY_ROWS} r)"),
    ] {
        let refused = WyrdError::from(
            bifrost
                .sql(&sql, &[])
                .await
                .expect_err("the unknown column is refused"),
        );
        let message = refused.to_string();
        assert_eq!(
            refused.code(),
            "WYRD_VALA_400_QUERY_INVALID_SQL",
            "{sql}: {message}"
        );
        assert!(
            message.contains("r.valeu") && message.contains("Did you mean 'r.value'?"),
            "{sql}: {message}"
        );
    }
    deployment.shutdown().await;
}

/// A planning-time type mismatch and an ambiguous column reference are each a
/// 400 carrying the planner's own detail naming the fault.
///
/// # Panics
/// Panics when a query succeeds or is refused without the planner's detail.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn type_mismatch_and_ambiguous_reference_carry_the_planners_detail() {
    let (deployment, bifrost) = live().await;

    for (sql, detail) in [
        (
            format!("SELECT value + 1 AS n FROM {QUERY_ROWS}"),
            "Cannot coerce arithmetic expression Utf8 + Int64",
        ),
        (
            format!("SELECT id FROM {QUERY_ROWS} a JOIN {QUERY_ROWS} b ON a.id = b.id"),
            "Ambiguous reference to unqualified field id",
        ),
    ] {
        let refused = WyrdError::from(
            bifrost
                .sql(&sql, &[])
                .await
                .expect_err("the query is refused"),
        );
        let message = refused.to_string();
        assert_eq!(
            refused.code(),
            "WYRD_VALA_400_QUERY_INVALID_SQL",
            "{sql}: {message}"
        );
        assert!(message.contains(detail), "{sql}: {message}");
    }
    deployment.shutdown().await;
}

/// A caller granted only `bifrost.datasets` who names another schema's table is
/// refused without learning its columns, even when the SQL misspells one.
///
/// # Panics
/// Panics when the query succeeds, or its refusal differs or names a column.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn unauthorized_table_refusal_reveals_no_columns() {
    let (deployment, _) = live().await;
    let datasets_only = json!({
        "resource": "bifrost_query",
        "action": "read",
        "scope": { "bifrost": { "schema": { "catalog": "bifrost", "schema": "datasets" } } },
    });
    deployment
        .server()
        .seed_role("datasets_reader", &[datasets_only])
        .await
        .expect("role seeds");
    let reader = Bifrost::connect_with(
        &deployment.client(
            &deployment
                .key("datasets_reader", &["datasets_reader"])
                .await,
        ),
        BifrostOptions::default(),
    )
    .await
    .expect("Bifrost connects");

    let rows: Vec<Id> = reader
        .sql_as(&format!("SELECT id FROM {QUERY_ROWS} ORDER BY id"), &[])
        .await
        .expect("the granted table reads");
    assert_eq!(rows.len(), 3);
    let refused = WyrdError::from(
        reader
            .sql(
                &format!(
                    "SELECT q.id, v.verdcit FROM {QUERY_ROWS} q \
                     JOIN bifrost.verification.results v ON v.result_id = q.value"
                ),
                &[],
            )
            .await
            .expect_err("the ungranted table is refused"),
    );
    let message = refused.to_string();
    assert_eq!(refused.code(), "WYRD_VALA_403_QUERY_FORBIDDEN", "{message}");
    assert!(
        !message.contains("verdict") && !message.contains("verdcit"),
        "{message}"
    );
    deployment.shutdown().await;
}

/// The `bifrost.datasets` table the attribution story writes.
const ATTRIBUTED_ROWS: &str = "bifrost.datasets.attributed_rows";

/// One [`ATTRIBUTED_ROWS`] row read back with the pair Wyrd stamped on it.
#[derive(Debug, PartialEq, Deserialize)]
struct AttributedRow {
    /// The row's ordinal.
    id: i64,
    /// Who the write was for.
    sub_principal_id: String,
    /// Who performed it.
    act_principal_id: String,
}

/// One Arrow IPC frame of [`ATTRIBUTED_ROWS`] holding row `id`, plus a
/// `sub_principal_id` and/or `act_principal_id` column carrying `sub` and
/// `act` when given, exactly as an otherwise-valid producer frame would.
///
/// # Panics
/// Panics when the batch or its IPC encoding cannot be built.
fn attributed_frame(id: i64, sub: Option<&str>, act: Option<&str>) -> Vec<u8> {
    use std::sync::Arc;

    use arrow::array::{ArrayRef, Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};

    let mut fields = vec![Field::new("id", DataType::Int64, false)];
    let mut columns: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(vec![id]))];
    for (name, value) in [("sub_principal_id", sub), ("act_principal_id", act)] {
        if let Some(value) = value {
            fields.push(Field::new(name, DataType::Utf8, false));
            columns.push(Arc::new(StringArray::from(vec![value])));
        }
    }
    let schema = Arc::new(Schema::new(fields));
    let batch =
        arrow::record_batch::RecordBatch::try_new(Arc::clone(&schema), columns).expect("frame");
    let mut ipc = Vec::new();
    let mut writer =
        arrow::ipc::writer::StreamWriter::try_new(&mut ipc, schema.as_ref()).expect("IPC writer");
    writer.write(&batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    drop(writer);
    ipc
}

/// Rows carry the verified subject and actor Wyrd stamps, and neither name
/// can be declared or written by a client (AC-005/016, AC-013/014).
///
/// Service A writes row 1 directly and Service B, acting for A, writes row
/// 2; after shutdown and publication they read back as A/A and A/B. Over the
/// public ingest transport, A's otherwise-valid frames naming B as subject,
/// B as actor, or B as both are refused by Scribe's server-owned column check
/// and land nothing, while A's otherwise identical control frame of row 4
/// lands as A/A, so the refusal is the managed column's. Declaring
/// `sub_principal_id` or `act_principal_id` as a table column, inserting
/// either as a row key, and registering either over the raw route are each
/// refused with the reserved-column error. Another tenant's administrator
/// cannot see the table.
///
/// # Panics
/// Panics when a write, read, or refusal differs from the story.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn subject_actor_rows_and_reserved_columns() {
    const RESERVED: &str = "WYRD_VALA_400_BIFROST_RESERVED_COLUMN";
    let deployment = Deployment::start().await;
    let server = deployment.server();
    let a = server
        .bootstrap_service("attribution_a", &["editor"])
        .await
        .expect("A bootstraps");
    let b = server
        .bootstrap_service("attribution_b", &["editor"])
        .await
        .expect("B bootstraps");
    let a_key = a.api_key().expect("A has a key");
    let b_key = b.api_key().expect("B has a key");
    let schema = json!({
        "type": "object",
        "properties": { "id": { "type": "integer" } },
        "required": ["id"],
    });
    let table = TableConfig::from_json_schema(ATTRIBUTED_ROWS, &schema).expect("table declares");
    let direct = deployment.client(a_key.expose_secret());
    let writer = Bifrost::connect_with(
        &direct,
        BifrostOptions {
            table: Some(table.clone()),
            ..BifrostOptions::default()
        },
    )
    .await
    .expect("A connects");
    writer.register().await.expect("table registers");
    writer
        .insert(br#"{"id": 1}"#.to_vec(), Correlation::default())
        .expect("A's row inserts");
    let spoofed = writer
        .insert(
            br#"{"id": 3, "act_principal_id": "someone-else"}"#.to_vec(),
            Correlation::default(),
        )
        .expect_err("a row key may not name the actor");
    assert_eq!(WyrdError::from(spoofed).code(), RESERVED);
    writer.shutdown().await.expect("A's writer drains");

    let subject = server
        .exchange_api_key(a_key)
        .await
        .expect("A's token exchanges");
    let delegated = deployment
        .rooted(
            b_key.expose_secret(),
            b.card_ref().expect("B has its fixture root"),
        )
        .on_behalf_of(
            secrecy::SecretString::from(subject),
            wyrd_spec::auth::TokenAudience::Bifrost,
        )
        .await
        .expect("B acts for A");
    let writer = Bifrost::connect_with(
        &delegated,
        BifrostOptions {
            table: Some(table),
            ..BifrostOptions::default()
        },
    )
    .await
    .expect("B for A connects");
    writer
        .insert(br#"{"id": 2}"#.to_vec(), Correlation::default())
        .expect("B's row for A inserts");
    writer.shutdown().await.expect("B's writer drains");

    let ingest = wyrd_testing::bifrost::write::RawIngest::connect(&direct)
        .await
        .expect("A's public ingest transport connects");
    let forger = b.id().to_string();
    for (id, sub, act) in [
        (10, Some(forger.as_str()), None),
        (11, None, Some(forger.as_str())),
        (12, Some(forger.as_str()), Some(forger.as_str())),
    ] {
        let forged = ingest
            .insert(
                ATTRIBUTED_ROWS,
                wyrd_spec::ids::uuid7().parse().expect("batch id"),
                attributed_frame(id, sub, act),
            )
            .await
            .expect_err("the server refuses a forged managed column");
        assert_eq!(
            forged.code(),
            "WYRD_VALA_400_OTLP_REQUEST_MALFORMED",
            "row {id}: {forged}"
        );
    }
    ingest
        .insert(
            ATTRIBUTED_ROWS,
            wyrd_spec::ids::uuid7().parse().expect("batch id"),
            attributed_frame(4, None, None),
        )
        .await
        .expect("A's control frame lands");
    server.flush_bifrost().await.expect("rows publish");

    let rows: Vec<AttributedRow> = Bifrost::connect_with(
        &deployment.admin(),
        BifrostOptions::default(),
    )
    .await
    .expect("Bifrost connects")
    .sql_as(
        &format!(
            "SELECT id, sub_principal_id, act_principal_id FROM {ATTRIBUTED_ROWS} ORDER BY id"
        ),
        &[],
    )
    .await
    .expect("rows read");
    let (a, b) = (a.id().to_string(), b.id().to_string());
    assert_eq!(
        rows,
        [
            AttributedRow {
                id: 1,
                sub_principal_id: a.clone(),
                act_principal_id: a.clone(),
            },
            AttributedRow {
                id: 2,
                sub_principal_id: a.clone(),
                act_principal_id: b,
            },
            AttributedRow {
                id: 4,
                sub_principal_id: a.clone(),
                act_principal_id: a,
            },
        ]
    );

    for column in ["sub_principal_id", "act_principal_id"] {
        let declared = TableConfig::from_json_schema(
            "bifrost.datasets.spoofed_rows",
            &json!({ "type": "object", "properties": { column: { "type": "string" } } }),
        )
        .expect_err("a managed name is not a user column");
        assert_eq!(WyrdError::from(&declared).code(), RESERVED, "{column}");
        let registered = deployment
            .admin()
            .request_json::<_, serde_json::Value>(
                reqwest::Method::POST,
                "/v1/bifrost/tables",
                Some(&wyrd_spec::vala::api::RegisterTableRequest {
                    namespace: "bifrost.datasets".to_owned(),
                    name: "spoofed_rows".to_owned(),
                    fields: vec![wyrd_spec::vala::api::FieldSpec {
                        name: column.to_owned(),
                        data_type: wyrd_spec::vala::api::DataTypeSpec::Utf8,
                        nullable: true,
                        metadata: std::collections::BTreeMap::new(),
                    }],
                    physical_layout: None,
                    compaction_target_file_size_bytes: None,
                    compaction_type: None,
                }),
            )
            .await
            .expect_err("the server refuses a managed name");
        assert_eq!(registered.code(), RESERVED, "{column}");
    }

    let foreign = Bifrost::connect_with(
        &deployment.client(&deployment.other_tenant_admin("attribution-other").await),
        BifrostOptions::default(),
    )
    .await
    .expect("the other tenant connects");
    foreign
        .sql(&format!("SELECT id FROM {ATTRIBUTED_ROWS}"), &[])
        .await
        .expect_err("another tenant cannot read the table");
    deployment.shutdown().await;
}
