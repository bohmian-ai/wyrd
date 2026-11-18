//! The `ui-sql` experiments: the original UI SQL, frozen from its builders,
//! run unchanged against deterministic UI-shaped evidence in each residence.
//!
//! The catalog is [`CATALOG`], copied verbatim from the design extraction
//! into the benchmark fixture owner. Every statement keeps its exact SQL
//! text; only its binds are resolved against the loaded fixture and frozen
//! before any window.

pub mod answer;

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use wyrd_spec::vala::api::QueryParam;

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

    /// The statement `id`.
    #[must_use]
    pub fn statement(&self, id: &str) -> Option<&Statement> {
        self.queries.iter().find(|statement| statement.id == id)
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
            ["traces", "verification", "logs", "metrics", "variants", "statstrip"]
        );
        let chart = catalog.statement("metrics-chart").expect("metrics-chart");
        assert_eq!(
            param(&chart.params[4]),
            Ok(wyrd_spec::vala::api::QueryParam::Int(60))
        );
        assert_eq!(
            catalog.statement("trace-counts").map(|q| q.depends_on.clone()),
            Some(vec!["trace-list".to_owned()])
        );
        assert_eq!(corpus_digest().len(), 64);
    }
}
