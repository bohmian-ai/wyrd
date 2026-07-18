//! Load-result assertions and latency summaries.

use std::time::Duration;

use hdrhistogram::Histogram;
use serde_json::Value;
use thiserror::Error;
use wyrd_spec::ids::DataTenantId;

use super::workload::{ExpectedRowShape, TenantIdentity};

const HISTOGRAM_SIGFIGS: u8 = 3;

/// Errors produced while recording a latency sample.
#[derive(Debug, Error)]
pub enum LatencyError {
    /// HdrHistogram could not be created or accepted a sample.
    #[error("latency histogram error: {0}")]
    Histogram(String),
}

/// HdrHistogram-backed latency accumulator.
#[derive(Debug)]
pub struct LatencyHistogram {
    histogram: Histogram<u64>,
}

impl LatencyHistogram {
    /// Create an empty microsecond latency histogram.
    pub fn new() -> Result<Self, LatencyError> {
        Histogram::new(HISTOGRAM_SIGFIGS)
            .map(|histogram| Self { histogram })
            .map_err(|error| LatencyError::Histogram(error.to_string()))
    }

    /// Record a duration rounded up to one microsecond.
    pub fn record(&mut self, duration: Duration) -> Result<(), LatencyError> {
        let micros = duration.as_micros().max(1);
        let micros = u64::try_from(micros)
            .map_err(|_| LatencyError::Histogram("duration exceeds u64 microseconds".to_owned()))?;
        self.histogram
            .record(micros)
            .map_err(|error| LatencyError::Histogram(error.to_string()))
    }

    /// Return the number of recorded samples.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.histogram.len()
    }

    /// Return the nearest recorded p50 sample in microseconds.
    #[must_use]
    pub fn p50_us(&self) -> Option<u64> {
        (self.count() > 0).then(|| self.histogram.value_at_quantile(0.50))
    }

    /// Return the nearest recorded p99 sample in microseconds.
    #[must_use]
    pub fn p99_us(&self) -> Option<u64> {
        (self.count() > 0).then(|| self.histogram.value_at_quantile(0.99))
    }

    /// Summarize the currently recorded samples.
    #[must_use]
    pub fn summary(&self) -> LatencySummary {
        LatencySummary {
            count: self.count(),
            p50_us: self.p50_us(),
            p99_us: self.p99_us(),
        }
    }
}

/// Percentile summary for one workload's query latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencySummary {
    /// Number of query samples.
    pub count: u64,
    /// p50 latency in microseconds, if samples exist.
    pub p50_us: Option<u64>,
    /// p99 latency in microseconds, if samples exist.
    pub p99_us: Option<u64>,
}

/// Result of reconciling submitted rows with unique queryable rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowCountReconciliation {
    /// Number of rows accepted by ingest callbacks.
    pub ingested_rows: u64,
    /// Number of distinct rows returned by query callbacks.
    pub queryable_rows: u64,
    /// Number of ingested rows not observed in query results.
    pub missing_rows: u64,
}

/// Compare ingested and queryable row counts.
#[must_use]
pub const fn reconcile_row_counts(
    ingested_rows: u64,
    queryable_rows: u64,
) -> RowCountReconciliation {
    RowCountReconciliation {
        ingested_rows,
        queryable_rows,
        missing_rows: ingested_rows.saturating_sub(queryable_rows),
    }
}

/// Assert that every returned row belongs to the requesting tenant.
///
/// This deliberately panics because a tenant leak is a hard test failure, not
/// a workload measurement. The panic includes the row ID and both tenant
/// values so a failing sustained-load case is immediately reproducible.
pub fn assert_no_cross_tenant_leak(
    principal_tenant: DataTenantId,
    rows: &[Value],
    shape: &ExpectedRowShape,
) {
    let expected = principal_tenant.to_string();
    for row in rows {
        let row_id = row
            .get(&shape.row_id_field)
            .map_or_else(|| "<missing>".to_owned(), value_as_display);
        let row_tenant = row
            .get(&shape.tenant_field)
            .map_or_else(|| "<missing>".to_owned(), value_as_display);
        let matches = match shape.tenant_identity {
            TenantIdentity::DataTenantId => row_tenant == expected,
            TenantIdentity::NamespacePath => row_tenant.contains(&expected),
        };
        assert!(
            matches,
            "cross-tenant leak: (row_id={row_id}, principal_tenant={expected}, row_tenant={row_tenant})"
        );
    }
}

fn value_as_display(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_reports_percentiles() {
        let mut histogram = LatencyHistogram::new().expect("histogram creates");
        histogram
            .record(Duration::from_micros(10))
            .expect("sample records");
        histogram
            .record(Duration::from_micros(100))
            .expect("sample records");
        assert_eq!(histogram.count(), 2);
        assert!(histogram.p50_us().is_some());
        assert!(histogram.p99_us().is_some());
    }

    #[test]
    fn row_count_reconciliation_reports_missing_rows() {
        assert_eq!(
            reconcile_row_counts(10, 7),
            RowCountReconciliation {
                ingested_rows: 10,
                queryable_rows: 7,
                missing_rows: 3,
            }
        );
    }

    #[test]
    #[should_panic(expected = "cross-tenant leak")]
    fn cross_tenant_rows_panic_with_offending_values() {
        let tenant = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        assert_no_cross_tenant_leak(
            tenant,
            &[serde_json::json!({
                "row_id": "row-1",
                "data_tenant_id": other.to_string(),
            })],
            &ExpectedRowShape::default(),
        );
    }
}
