//! The common analytical SQL a data engineer or data scientist writes —
//! filters, aggregates, joins, subqueries, a CTE, and a window — returns its
//! exact rows over two small tables that are part published and part live.

use serde_json::{Value, json};
use wyrd_ai::Bifrost;
use wyrd_ai::bifrost::BifrostOptions;
use wyrd_ai::bifrost::{BifrostClientError, Correlation, TableConfig};

use crate::support::Deployment;

/// Customers: `customer_id`, `name`, and a nullable `region`.
const CUSTOMERS: &str = "vala.datasets.coverage_customers";

/// Orders: `order_id`, a nullable `customer_id`, and `amount`.
const ORDERS: &str = "vala.datasets.coverage_orders";

/// One query of the corpus and the exact rows it must return.
///
/// Rows are JSON objects keyed by column; a NULL column is absent from its
/// object, as the SDK's row conversion omits it.
struct Case {
    /// What the query exercises, reported when it differs.
    name: &'static str,
    /// The SQL, over [`CUSTOMERS`] and [`ORDERS`].
    sql: String,
    /// The rows it must return, in order.
    expected: Value,
}

/// Every corpus query returns exactly its expected rows when customers 1–2
/// and orders 10–12 are published and the rest are live in Scribe, and every
/// [`refusals`] query fails at execution.
///
/// Order 13 has no customer and order 14 names customer 5, who does not
/// exist, so outer joins, `NOT IN` over a NULL, and `NOT EXISTS` each have a
/// row that decides them.
///
/// # Panics
/// Panics when a table cannot register or a row cannot be written, with
/// every query whose rows differ or that fails, and with every refusal query
/// that returns rows or fails other than at execution.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn common_queries_answer_over_published_and_live_rows() {
    let deployment = Deployment::start().await;
    let admin = deployment.admin();
    let customers = writer(
        &admin,
        CUSTOMERS,
        json!({
            "customer_id": { "type": "integer" },
            "name": { "type": "string" },
            "region": { "type": ["string", "null"] },
        }),
        &["customer_id", "name"],
    )
    .await;
    let orders = writer(
        &admin,
        ORDERS,
        json!({
            "order_id": { "type": "integer" },
            "customer_id": { "type": ["integer", "null"] },
            "amount": { "type": "integer" },
        }),
        &["order_id", "amount"],
    )
    .await;

    write(
        &customers,
        &[
            json!({"customer_id": 1, "name": "ada", "region": "east"}),
            json!({"customer_id": 2, "name": "bo", "region": "west"}),
        ],
    )
    .await;
    write(
        &orders,
        &[
            json!({"order_id": 10, "customer_id": 1, "amount": 50}),
            json!({"order_id": 11, "customer_id": 1, "amount": 30}),
            json!({"order_id": 12, "customer_id": 2, "amount": 70}),
        ],
    )
    .await;
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("first rows publish");
    write(
        &customers,
        &[
            json!({"customer_id": 3, "name": "cy"}),
            json!({"customer_id": 4, "name": "di", "region": "east"}),
        ],
    )
    .await;
    write(
        &orders,
        &[
            json!({"order_id": 13, "amount": 20}),
            json!({"order_id": 14, "customer_id": 5, "amount": 40}),
            json!({"order_id": 15, "customer_id": 2, "amount": 30}),
        ],
    )
    .await;

    let bifrost = Bifrost::connect_with(&admin, BifrostOptions::default())
        .await
        .expect("Bifrost connects");
    let mut mismatches = Vec::new();
    for case in cases() {
        match bifrost.sql_as::<Value>(&case.sql, &[]).await {
            Ok(rows) if Value::Array(rows.clone()) == case.expected => {}
            Ok(rows) => mismatches.push(format!(
                "{}: got {rows:?}, expected {}",
                case.name, case.expected
            )),
            Err(error) => mismatches.push(format!("{}: {error}", case.name)),
        }
    }
    for (name, sql) in refusals() {
        match bifrost.sql_as::<Value>(&sql, &[]).await {
            Err(BifrostClientError::Transport(refusal))
                if refusal.code() == "WYRD_VALA_500_QUERY_EXECUTION_FAILED" => {}
            other => mismatches.push(format!(
                "{name}: got {other:?}, expected an execution refusal"
            )),
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    deployment.shutdown().await;
}

/// The corpus, over the rows [`common_queries_answer_over_published_and_live_rows`] writes.
fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "projection, filter, order, limit",
            sql: format!(
                "SELECT order_id, amount FROM {ORDERS} WHERE amount >= 20 \
                 ORDER BY amount DESC, order_id LIMIT 3"
            ),
            expected: json!([
                {"order_id": 12, "amount": 70},
                {"order_id": 10, "amount": 50},
                {"order_id": 14, "amount": 40},
            ]),
        },
        Case {
            name: "filtered GROUP BY with HAVING",
            sql: format!(
                "SELECT customer_id, SUM(amount) AS total, COUNT(*) AS n FROM {ORDERS} \
                 WHERE customer_id IS NOT NULL GROUP BY customer_id \
                 HAVING SUM(amount) > 45 ORDER BY customer_id"
            ),
            expected: json!([
                {"customer_id": 1, "total": 80, "n": 2},
                {"customer_id": 2, "total": 100, "n": 2},
            ]),
        },
        Case {
            name: "inner join",
            sql: format!(
                "SELECT o.order_id, c.name FROM {ORDERS} o \
                 JOIN {CUSTOMERS} c ON o.customer_id = c.customer_id ORDER BY o.order_id"
            ),
            expected: json!([
                {"order_id": 10, "name": "ada"},
                {"order_id": 11, "name": "ada"},
                {"order_id": 12, "name": "bo"},
                {"order_id": 15, "name": "bo"},
            ]),
        },
        Case {
            name: "left join",
            sql: format!(
                "SELECT o.order_id, c.name FROM {ORDERS} o \
                 LEFT JOIN {CUSTOMERS} c ON o.customer_id = c.customer_id ORDER BY o.order_id"
            ),
            expected: json!([
                {"order_id": 10, "name": "ada"},
                {"order_id": 11, "name": "ada"},
                {"order_id": 12, "name": "bo"},
                {"order_id": 13},
                {"order_id": 14},
                {"order_id": 15, "name": "bo"},
            ]),
        },
        Case {
            name: "scalar subquery",
            sql: format!(
                "SELECT order_id FROM {ORDERS} \
                 WHERE amount = (SELECT MAX(amount) FROM {ORDERS})"
            ),
            expected: json!([{"order_id": 12}]),
        },
        Case {
            name: "single-row scalar subquery",
            sql: format!(
                "SELECT order_id FROM {ORDERS} \
                 WHERE amount = (SELECT amount FROM {ORDERS} WHERE order_id = 11) \
                 ORDER BY order_id"
            ),
            expected: json!([{"order_id": 11}, {"order_id": 15}]),
        },
        Case {
            name: "scalar subquery over no rows",
            sql: format!(
                "SELECT order_id FROM {ORDERS} \
                 WHERE amount = (SELECT amount FROM {ORDERS} WHERE order_id = 99)"
            ),
            expected: json!([]),
        },
        Case {
            name: "scalar subquery yielding NULL",
            sql: format!(
                "SELECT order_id FROM {ORDERS} \
                 WHERE customer_id = (SELECT customer_id FROM {ORDERS} WHERE order_id = 13)"
            ),
            expected: json!([]),
        },
        Case {
            name: "correlated scalar subquery",
            sql: format!(
                "SELECT c.name, (SELECT SUM(o.amount) FROM {ORDERS} o \
                 WHERE o.customer_id = c.customer_id) AS total \
                 FROM {CUSTOMERS} c ORDER BY c.customer_id"
            ),
            expected: json!([
                {"name": "ada", "total": 80},
                {"name": "bo", "total": 100},
                {"name": "cy"},
                {"name": "di"},
            ]),
        },
        Case {
            name: "IN",
            sql: format!(
                "SELECT name FROM {CUSTOMERS} \
                 WHERE customer_id IN (SELECT customer_id FROM {ORDERS}) ORDER BY name"
            ),
            expected: json!([{"name": "ada"}, {"name": "bo"}]),
        },
        Case {
            name: "NOT IN over a NULL key",
            sql: format!(
                "SELECT name FROM {CUSTOMERS} \
                 WHERE customer_id NOT IN (SELECT customer_id FROM {ORDERS})"
            ),
            expected: json!([]),
        },
        Case {
            name: "NOT IN without NULL keys",
            sql: format!(
                "SELECT name FROM {CUSTOMERS} WHERE customer_id NOT IN \
                 (SELECT customer_id FROM {ORDERS} WHERE customer_id IS NOT NULL) ORDER BY name"
            ),
            expected: json!([{"name": "cy"}, {"name": "di"}]),
        },
        Case {
            name: "EXISTS",
            sql: format!(
                "SELECT name FROM {CUSTOMERS} c WHERE EXISTS \
                 (SELECT 1 FROM {ORDERS} o WHERE o.customer_id = c.customer_id) ORDER BY name"
            ),
            expected: json!([{"name": "ada"}, {"name": "bo"}]),
        },
        Case {
            name: "NOT EXISTS",
            sql: format!(
                "SELECT name FROM {CUSTOMERS} c WHERE NOT EXISTS \
                 (SELECT 1 FROM {ORDERS} o WHERE o.customer_id = c.customer_id) ORDER BY name"
            ),
            expected: json!([{"name": "cy"}, {"name": "di"}]),
        },
        Case {
            name: "CTE joined to a second table",
            sql: format!(
                "WITH a AS (SELECT customer_id, SUM(amount) AS total FROM {ORDERS} \
                 GROUP BY customer_id) \
                 SELECT b.name, a.total FROM {CUSTOMERS} b \
                 INNER JOIN a ON a.customer_id = b.customer_id ORDER BY b.name"
            ),
            expected: json!([
                {"name": "ada", "total": 80},
                {"name": "bo", "total": 100},
            ]),
        },
        Case {
            name: "RANK window",
            sql: format!(
                "SELECT order_id, RANK() OVER (ORDER BY amount DESC) AS r \
                 FROM {ORDERS} ORDER BY order_id"
            ),
            expected: json!([
                {"order_id": 10, "r": 2},
                {"order_id": 11, "r": 4},
                {"order_id": 12, "r": 1},
                {"order_id": 13, "r": 6},
                {"order_id": 14, "r": 3},
                {"order_id": 15, "r": 4},
            ]),
        },
    ]
}

/// Scalar subqueries that must fail because their input has two rows: the
/// published order 11 and the live order 15, both of amount 30.
///
/// Joined as a set, the equal values would match outer rows or multiply them
/// instead of refusing the query; the last query has no outer row to match.
fn refusals() -> Vec<(&'static str, String)> {
    let two_rows = format!("(SELECT amount FROM {ORDERS} WHERE order_id IN (11, 15))");
    vec![
        (
            "two-row scalar subquery in a filter",
            format!("SELECT order_id FROM {ORDERS} WHERE amount = {two_rows}"),
        ),
        (
            "two-row scalar subquery in a projection",
            format!("SELECT order_id, {two_rows} AS amount FROM {ORDERS}"),
        ),
        (
            "two-row scalar subquery with no outer row to match",
            format!("SELECT order_id FROM {ORDERS} WHERE order_id > 99 AND amount = {two_rows}"),
        ),
    ]
}

/// Registers `table` with `properties`, requiring `required`, and returns
/// its writer.
///
/// # Panics
/// Panics when the table cannot declare, connect, or register.
async fn writer(
    admin: &wyrd_ai::WyrdClient,
    table: &str,
    properties: Value,
    required: &[&str],
) -> Bifrost {
    let config = TableConfig::from_json_schema(
        table,
        &json!({ "type": "object", "properties": properties, "required": required }),
    )
    .expect("table declares");
    let writer = Bifrost::connect_with(
        admin,
        BifrostOptions {
            table: Some(config),
            ..BifrostOptions::default()
        },
    )
    .await
    .expect("writer connects");
    writer.register().await.expect("table registers");
    writer
}

/// Writes `rows` and waits until Scribe acknowledged them.
///
/// # Panics
/// Panics when a row cannot enqueue or the flush fails.
async fn write(writer: &Bifrost, rows: &[Value]) {
    for row in rows {
        writer
            .insert(
                serde_json::to_vec(row).expect("row serializes"),
                Correlation::default(),
            )
            .expect("row inserts");
    }
    writer.flush().await.expect("rows flush");
}
