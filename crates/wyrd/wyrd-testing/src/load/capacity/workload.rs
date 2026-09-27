//! The fixed `vala.datasets.query_capacity` fixture and its nine SQL strings.
//!
//! Everything the benchmark writes and asks lives here, in plain constants and
//! one-line builders, so a reader can check a report's numbers against the
//! exact data shape and statements without following a workload DSL.

use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use sha2::{Digest as _, Sha256};

/// Fully qualified table every benchmark query reads.
///
/// `vala.datasets` because it is the one namespace whose tables a public
/// client may register.
pub const TABLE: &str = "vala.datasets.query_capacity";

/// Rows written, flushed, and published before the no-live baseline.
pub const PUBLISHED_ROWS: i64 = 1_048_576;

/// IDs per published batch; each batch is flushed and published on its own.
pub const PUBLISHED_BATCH_ROWS: i64 = 131_072;

/// Rows per public ingest request, so no single request approaches the wire
/// limit while one published batch still lands as one flush.
pub const INGEST_REQUEST_ROWS: i64 = 16_384;

/// First ID acknowledged into Scribe but never flushed.
pub const LIVE_START: i64 = PUBLISHED_ROWS;

/// One past the last live ID: 32,768 acknowledged, unflushed rows.
pub const LIVE_END: i64 = 1_081_344;

/// Rows each short query returns.
pub const SHORT_ROWS: i64 = 20;

/// Short-query buckets; bucket `b` reads the first IDs of published batch `b`.
pub const SHORT_BUCKETS: u64 = 8;

/// The one 64-byte payload every row carries; queries never project it.
pub const PAYLOAD: &str = "wyrd-bifrost-query-capacity-fixed-payload-0123456789abcdefghijkl";

/// Returns the two-column user schema: non-null `id` and `payload`.
#[must_use]
pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("payload", DataType::Utf8, false),
    ]))
}

/// Builds the rows with IDs `start..end` in ascending order.
///
/// # Errors
///
/// Returns the Arrow error when the columns do not match [`schema`], which is
/// a fixture defect rather than a runtime condition.
pub fn rows(start: i64, end: i64) -> Result<RecordBatch, arrow::error::ArrowError> {
    let ids: Int64Array = (start..end).collect();
    let payloads: StringArray = (start..end).map(|_| Some(PAYLOAD)).collect();
    RecordBatch::try_new(schema(), vec![Arc::new(ids), Arc::new(payloads)])
}

/// Returns the short read for `bucket`, exactly as issued.
#[must_use]
pub fn short_sql(bucket: u64) -> String {
    let first = bucket * PUBLISHED_BATCH_ROWS.unsigned_abs();
    format!(
        "SELECT id FROM {TABLE} WHERE id >= {first} AND id < {} ORDER BY id LIMIT {SHORT_ROWS}",
        first + SHORT_ROWS.unsigned_abs()
    )
}

/// Returns the exact, ordered IDs the short read for `bucket` must return.
#[must_use]
pub fn short_expected(bucket: u64) -> Vec<i64> {
    let first = i64::try_from(bucket).unwrap_or(i64::MAX) * PUBLISHED_BATCH_ROWS;
    (first..first + SHORT_ROWS).collect()
}

/// Returns the live read that must return every acknowledged, unflushed ID.
#[must_use]
pub fn live_sql() -> String {
    format!("SELECT id FROM {TABLE} WHERE id >= {LIVE_START} AND id < {LIVE_END}")
}

/// Returns the lowercase hex SHA-256 of `ids` as little-endian `i64`s.
///
/// Callers pass IDs in the order they are compared: returned order for a short
/// read, sorted order for the unordered live read.
#[must_use]
pub fn id_digest(ids: &[i64]) -> String {
    let mut hasher = Sha256::new();
    for id in ids {
        hasher.update(id.to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture is exactly the shape and statements the benchmark reports.
    ///
    /// # Panics
    ///
    /// Panics when the payload, a statement, or an expected ID set drifts.
    #[test]
    fn query_capacity_fixture_is_exact() {
        assert_eq!(PAYLOAD.len(), 64);
        assert_eq!(PUBLISHED_BATCH_ROWS * 8, PUBLISHED_ROWS);
        assert_eq!(LIVE_END - LIVE_START, 32_768);
        assert_eq!(
            short_sql(7),
            "SELECT id FROM vala.datasets.query_capacity WHERE id >= 917504 AND id < 917524 \
             ORDER BY id LIMIT 20"
        );
        assert_eq!(short_expected(7).first(), Some(&917_504));
        assert_eq!(short_expected(7).len(), 20);
        assert_eq!(
            live_sql(),
            "SELECT id FROM vala.datasets.query_capacity WHERE id >= 1048576 AND id < 1081344"
        );
        let batch = rows(0, 3).expect("fixture rows");
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.schema(), schema());
    }
}
