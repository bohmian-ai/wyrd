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
use wyrd_testing::experiment::{Criterion, Declaration, Metric, Op, Step, WINDOW_SECONDS};

use fixture::Journey;

/// The benchmark case every ui-sql query declares to the server.
pub const CASE: &str = "ui-sql";

/// Where the rows are, in the order every run moves them.
pub const RESIDENCES: [&str; 3] = ["memtable", "staged", "published"];

/// The original pages, each its catalog family's statements as one bundle.
pub const PAGES: [&str; 4] = ["traces", "verification", "logs", "metrics"];

/// Concurrent pages each page bundle runs at.
pub const PAGE_CLIENTS: [u64; 2] = [1, 8];

/// The representative statements measured alone at one client.
pub const QUERIES: [&str; 15] = [
    "trace-list",
    "trace-counts",
    "verify-list",
    "logs-list",
    "metrics-chart",
    "metrics-histogram",
    "metrics-24h-chart",
    "metrics-nested-chart",
    "trace-child-match",
    "trace-page-after",
    "trace-waterfall",
    "span-detail",
    "log-detail",
    "stat-duration_p95-total",
    "stat-cost-buckets",
];

/// Seconds of every ui-sql repetition: a one-page bundle answers about a
/// dozen statements per round, so 10 s keeps every step well above the
/// p95 sample floor.
const WINDOW: u64 = 10;

/// Seconds the budget allows beyond the steps' windows: the journey load,
/// the reference pass, and each residence's placement and recheck.
const SETUP_SECONDS: u64 = 600;

/// The ui-sql declaration over `base` with the Bifrost read `metrics` and
/// the server's `limits`: every page at [`PAGE_CLIENTS`] and every
/// [`QUERIES`] statement at one client, in each residence in order, judged
/// on errors, exact typed answers and the p95 sample floor.
#[must_use]
pub fn declare(mut base: Declaration, mut metrics: Vec<Metric>, limits: Value) -> Declaration {
    "Over the deterministic default journey in the catalog's hour, what do the original UI pages at 1 and 8 concurrent pages and representative standalone UI statements cost the server and client when the rows are in memtables, staged, and published, with every typed answer identical in every residence?".clone_into(&mut base.question);
    let journey = Journey::DEFAULT;
    base.variables
        .insert("clients".to_owned(), json!(PAGE_CLIENTS));
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
        json!("closed-loop public-client pages, each sending its next page once the last answers: a page runs its family's independent statements concurrently, then the statements taking their results; every answer checked against its typed reference; rows placed in each residence by the benchmark control outside every window"),
    );
    base.controls.insert(
        "corpus".to_owned(),
        json!({
            "sha256": corpus_digest(),
            "source_revision": Catalog::frozen().map(|catalog| catalog.source_revision).unwrap_or_default(),
        }),
    );
    metrics.push(Metric::raw(
        "first_request_ms",
        "ms",
        "diagnostic: the first statement's time after the rows reached the step's residence, before any warmup",
    ));
    base.metrics = metrics;
    let mut steps = Vec::new();
    for residence in RESIDENCES {
        for page in PAGES {
            for clients in PAGE_CLIENTS {
                steps.push(Step::measured(
                    &format!("{residence}-{page}-page-c{clients}"),
                    json!({ "residence": residence, "page": page, "clients": clients, WINDOW_SECONDS: WINDOW }),
                ));
            }
        }
        for query in QUERIES {
            steps.push(Step::measured(
                &format!("{residence}-{query}-c1"),
                json!({ "residence": residence, "query": query, "clients": 1, WINDOW_SECONDS: WINDOW }),
            ));
        }
    }
    let ids: Vec<String> = steps.iter().map(|step| step.id.clone()).collect();
    let timing = base.timing;
    base.timing.budget_seconds = SETUP_SECONDS
        + ids.len() as u64 * (timing.warmup_seconds + u64::from(timing.repetitions) * WINDOW);
    base.steps = steps;
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
    use super::{Catalog, corpus_digest, param};

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
}
