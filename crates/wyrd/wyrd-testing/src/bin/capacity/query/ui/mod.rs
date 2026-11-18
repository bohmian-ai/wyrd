//! The `ui-sql` experiments: the original UI SQL, frozen from its builders,
//! run unchanged against deterministic UI-shaped evidence in each residence.
//!
//! The catalog is [`CATALOG`], copied verbatim from the design extraction
//! into the benchmark fixture owner. Every statement keeps its exact SQL
//! text; only its binds are resolved against the loaded fixture and frozen
//! before any window.

pub mod answer;
pub mod fixture;
pub mod workload;

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use wyrd_spec::vala::api::QueryParam;
use wyrd_testing::experiment::{Criterion, Declaration, Metric, Op, Step};

use answer::Order;
use fixture::Journey;

/// The benchmark case every ui-sql query declares to the server.
pub const CASE: &str = "ui-sql";

/// Where the rows are, in the order every run moves them.
pub const RESIDENCES: [&str; 3] = ["memtable", "staged", "published"];

/// The original pages, each its catalog family's statements as one bundle,
/// in the order every client cycles through them.
pub const PAGES: [&str; 4] = ["traces", "verification", "logs", "metrics"];

/// Concurrent clients of each residence's step, each cycling through every
/// page.
pub const CLIENTS: [u64; 2] = [1, 8];

/// The diagnostic observation of one page's or one statement's client
/// latency: `page.<page>` or `statement.<id>`.
#[must_use]
pub fn observation(kind: &str, name: &str) -> String {
    format!("{kind}.{name}")
}

/// The ui-sql declaration over `base` with the Bifrost read `metrics` and
/// the server's `limits`: one step per residence and [`CLIENTS`] count, in
/// residence order, each running every page in the same fixed cycle with
/// the level's standard windows and budget, judged on errors, exact typed
/// answers and the pooled p95 sample floor. Each page and each page
/// statement also reports its own client latency and sample count.
#[must_use]
pub fn declare(mut base: Declaration, mut metrics: Vec<Metric>, limits: Value) -> Declaration {
    "Over the deterministic default journey in the catalog's hour, what do the original UI pages, cycled in a fixed order by 1 and 8 concurrent clients, cost the server and client per page and per statement when the rows are in memtables, staged, and published, with every typed answer of every catalog statement identical in every residence?".clone_into(&mut base.question);
    let journey = Journey::DEFAULT;
    base.variables.insert("clients".to_owned(), json!(CLIENTS));
    base.controls.insert(
        "fixture".to_owned(),
        json!({
            "journey": journey,
            "exports": journey.exports(),
            "spans": journey.span_rows(),
            "logs": journey.log_rows(),
            "metric_points": journey.point_rows(),
            "hour_start_unix_nano": fixture::HOUR_START_NANOS,
        }),
    );
    base.controls.insert("limits".to_owned(), limits);
    base.controls.insert(
        "workload".to_owned(),
        json!("closed-loop public clients, each cycling traces, verification, logs and metrics pages from its own offset and sending its next page once the last answers: a page runs its family's independent statements concurrently, then the statements taking their results; every answer checked against its typed reference; rows placed in each residence by the benchmark control outside every window, where every catalog statement, timed or standalone, must answer as in memtables"),
    );
    let catalog = Catalog::frozen();
    base.controls.insert(
        "corpus".to_owned(),
        json!({
            "sha256": corpus_digest(),
            "source_revision": catalog.as_ref().map(|catalog| catalog.source_revision.clone()).unwrap_or_default(),
        }),
    );
    metrics.push(Metric::raw(
        "first_request_ms",
        "ms",
        "diagnostic: the first statement's time after the rows reached the step's residence, before any warmup",
    ));
    for page in PAGES {
        metrics.extend(Metric::diagnostic(
            &observation("page", page),
            &format!("client: one {page} page from its first statement sent to its last answer checked, late pages included; a percentile under its sample floor is inconclusive"),
        ));
    }
    for statement in catalog.iter().flat_map(|catalog| &catalog.queries) {
        if PAGES.contains(&statement.family.as_str()) {
            metrics.extend(Metric::diagnostic(
                &observation("statement", &statement.id),
                &format!("client: one `{}` from sent to its terminal, inside its page and overlapping its siblings; a percentile under its sample floor is inconclusive", statement.id),
            ));
        }
    }
    base.metrics = metrics;
    base.steps = RESIDENCES
        .into_iter()
        .flat_map(|residence| {
            CLIENTS.map(|clients| {
                Step::measured(
                    &format!("{residence}-ui-c{clients}"),
                    json!({ "residence": residence, "clients": clients }),
                )
            })
        })
        .collect();
    let ids: Vec<String> = base.steps.iter().map(|step| step.id.clone()).collect();
    base.criteria = vec![
        Criterion::every("no-errors", "errors", &ids, Op::Le, 0.0),
        Criterion::every("exact-answers", "wrong_results", &ids, Op::Le, 0.0),
        Criterion::every("p95-samples", "p95_ms", &ids, Op::Ge, 0.0).percentile(95),
    ];
    base
}

/// The frozen catalog: SQL, binds, builder inputs, dependencies and the UI
/// source revision and hashes it was extracted from.
pub const CATALOG: &str =
    include_str!("../../../../../../../../benchmarks/fixtures/bifrost/ui-sql/queries.json");

/// The frozen catalog, parsed.
#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    /// The UI source revision the builders ran at.
    pub source_revision: String,
    /// Every statement, in catalog order.
    pub queries: Vec<Statement>,
}

/// One original UI statement.
#[derive(Debug, Clone, Deserialize)]
pub struct Statement {
    /// The statement id, such as `trace-list`.
    pub id: String,
    /// Its page family: `traces`, `verification`, `logs`, `metrics`,
    /// `variants` or `statstrip`.
    pub family: String,
    /// The statements whose results its binds take, by id.
    pub depends_on: Vec<String>,
    /// The exact SQL text.
    pub sql: String,
    /// The example binds, as JSON strings and numbers.
    pub params: Vec<Value>,
}

impl Statement {
    /// How much row order this statement's SQL decides: [`Order::Total`]
    /// unless it is in [`TIED`].
    #[must_use]
    pub fn order(&self) -> Order<'static> {
        TIED.iter()
            .find(|(id, _)| *id == self.id)
            .map_or(Order::Total, |(_, order)| *order)
    }
}

/// The statements whose final `ORDER BY` leaves rows tied, and the keys it
/// does order: per-bucket charts with several rows a bucket, grouped
/// statements with no order, and limits that may cut a tie.
const TIED: [(&str, Order<'static>); 13] = [
    ("trace-counts", Order::Keys(&[])),
    ("verify-chart-verdict", Order::Keys(&["at"])),
    ("verify-chart-verifier", Order::Keys(&["at"])),
    ("logs-timeline", Order::Keys(&["at"])),
    ("metrics-chart", Order::Keys(&["at"])),
    ("metrics-histogram", Order::Keys(&["at"])),
    (
        "metrics-group-keys",
        Order::Cut {
            keys: &[],
            limit: 100,
        },
    ),
    ("metrics-apart-chart", Order::Keys(&["at"])),
    ("metrics-apart-histogram", Order::Keys(&["at"])),
    ("metrics-24h-chart", Order::Keys(&["at"])),
    ("metrics-nested-chart", Order::Keys(&["at"])),
    (
        "metric-points-list",
        Order::Cut {
            keys: &["time_unix_nano"],
            limit: 50,
        },
    ),
    ("stat-cost-buckets", Order::Keys(&["at"])),
];

impl Catalog {
    /// The parsed [`CATALOG`].
    ///
    /// # Errors
    ///
    /// Returns the parse failure of a malformed catalog.
    pub fn frozen() -> Result<Self, serde_json::Error> {
        serde_json::from_str(CATALOG)
    }
}

/// The lowercase hex SHA-256 of [`CATALOG`]: the corpus identity a record
/// carries, so only runs of the same SQL compare.
#[must_use]
pub fn corpus_digest() -> String {
    hex::encode(Sha256::digest(CATALOG.as_bytes()))
}

/// The query parameter one JSON bind is: a string, an integer, or a finite
/// float.
///
/// # Errors
///
/// Returns a message for any other JSON value.
pub fn param(value: &Value) -> Result<QueryParam, String> {
    match value {
        Value::String(text) => Ok(QueryParam::String(text.clone())),
        Value::Number(number) => number
            .as_i64()
            .map(QueryParam::Int)
            .or_else(|| number.as_f64().map(QueryParam::Float))
            .ok_or_else(|| format!("unrepresentable number bind {number}")),
        Value::Null => Ok(QueryParam::Null),
        Value::Bool(flag) => Ok(QueryParam::Bool(*flag)),
        other => Err(format!("unsupported bind {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::{Catalog, Order, corpus_digest, param};

    /// The frozen catalog parses whole: 69 statements in six families, from
    /// the pinned revision, with numeric binds kept integral.
    ///
    /// # Panics
    ///
    /// Panics when the catalog drifts or a bind is misread.
    #[test]
    fn frozen_catalog_holds_the_original_statements() {
        let catalog = Catalog::frozen().expect("catalog");
        assert_eq!(
            catalog.source_revision,
            "4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36"
        );
        assert_eq!(catalog.queries.len(), 69);
        let mut families: Vec<&str> = catalog.queries.iter().map(|q| q.family.as_str()).collect();
        families.dedup();
        assert_eq!(
            families,
            [
                "traces",
                "verification",
                "logs",
                "metrics",
                "variants",
                "statstrip"
            ]
        );
        let chart = catalog
            .queries
            .iter()
            .find(|q| q.id == "metrics-chart")
            .expect("metrics-chart");
        assert_eq!(
            param(&chart.params[4]),
            Ok(wyrd_spec::vala::api::QueryParam::Int(60))
        );
        assert_eq!(
            catalog
                .queries
                .iter()
                .find(|q| q.id == "trace-counts")
                .map(|q| q.depends_on.clone()),
            Some(vec!["trace-list".to_owned()])
        );
        assert_eq!(corpus_digest().len(), 64);
    }

    /// Each tied statement's SQL ends ordering exactly its declared keys,
    /// under its declared limit, and the lists and buckets stay total.
    ///
    /// # Panics
    ///
    /// Panics when a declared order disagrees with the frozen SQL.
    #[test]
    fn tied_statements_declare_the_order_their_sql_leaves() {
        let catalog = Catalog::frozen().expect("catalog");
        let order = |id: &str| {
            catalog
                .queries
                .iter()
                .find(|statement| statement.id == id)
                .expect("statement")
                .order()
        };
        assert_eq!(order("logs-timeline"), Order::Keys(&["at"]));
        assert_eq!(
            order("metric-points-list"),
            Order::Cut {
                keys: &["time_unix_nano"],
                limit: 50
            }
        );
        assert_eq!(order("trace-list"), Order::Total);
        assert_eq!(order("stat-logs-buckets"), Order::Total);
        for statement in &catalog.queries {
            let (keys, limit) = match statement.order() {
                Order::Total => continue,
                Order::Keys(keys) => (keys, None),
                Order::Cut { keys, limit } => (keys, Some(limit)),
            };
            let ending = [
                (!keys.is_empty()).then(|| format!("ORDER BY {}", keys.join(", "))),
                limit.map(|limit| format!("LIMIT {limit}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            let tail = statement
                .sql
                .rsplit(')')
                .next()
                .unwrap_or_default()
                .replace(" DESC", "")
                .replace(" ASC", "");
            assert!(tail.ends_with(&ending), "{}: {tail}", statement.id);
            assert_eq!(
                keys.is_empty(),
                !tail.contains("ORDER BY"),
                "{}",
                statement.id
            );
        }
    }
}
