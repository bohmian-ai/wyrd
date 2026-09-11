//! Concurrent sustained-load execution over a real test server or callbacks.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use thiserror::Error;
use tokio::task::JoinSet;
use wyrd_spec::ids::DataTenantId;

use super::assertions::{
    LatencyHistogram, LatencySummary, RowCountReconciliation, assert_no_cross_tenant_leak,
    reconcile_row_counts,
};
use super::workload::{QueryRequest, TenantWorkload};
use crate::WyrdTestServer;

type LoadFuture<T> = Pin<Box<dyn Future<Output = Result<T, LoadError>> + Send + 'static>>;
type IngestCallback = Arc<dyn Fn(IngestRequest) -> LoadFuture<usize> + Send + Sync + 'static>;
type QueryCallback = Arc<dyn Fn(QueryRequest) -> LoadFuture<QueryResponse> + Send + Sync + 'static>;

/// Request passed to a sustained-load ingest callback.
#[derive(Debug, Clone)]
pub struct IngestRequest {
    /// Tenant that owns the generated row.
    pub tenant: DataTenantId,
    /// Monotonic row sequence local to the workload.
    pub row_id: u64,
    /// Minimal generated row used by callback-driven smoke tests.
    pub row: Value,
}

/// Query result returned by a sustained-load query callback.
#[derive(Debug, Clone, Default)]
pub struct QueryResponse {
    /// Rows returned by the Oracle/client callback.
    pub rows: Vec<Value>,
}

/// Errors from sustained-load setup or callback execution.
#[derive(Debug, Error)]
pub enum LoadError {
    /// No tenant workloads were configured.
    #[error("sustained-load harness has no tenant workloads")]
    NoWorkloads,
    /// A callback returned an error.
    #[error("load callback failed for tenant {tenant}: {message}")]
    Callback {
        tenant: DataTenantId,
        message: String,
    },
    /// A latency histogram could not accept a sample.
    #[error("latency histogram failed: {0}")]
    Histogram(String),
    /// A test server could not be shut down.
    #[error("test server shutdown failed: {0}")]
    ServerShutdown(String),
    /// A workload task panicked or was cancelled.
    #[error("workload task failed: {0}")]
    Task(String),
}

/// Per-tenant sustained-load results.
#[derive(Debug, Clone)]
pub struct TenantLoadReport {
    /// Tenant exercised by this report.
    pub tenant: DataTenantId,
    /// Count reconciliation for this tenant.
    pub reconciliation: RowCountReconciliation,
    /// Number of query callbacks completed.
    pub query_count: u64,
    /// Query latency percentiles.
    pub latency: LatencySummary,
}

/// Aggregate sustained-load result.
#[derive(Debug, Clone)]
pub struct SustainedLoadReport {
    /// Per-tenant results in tenant ID order.
    pub tenants: Vec<TenantLoadReport>,
    /// Total rows missing from query results.
    pub missing_rows: u64,
    /// Maximum observed p99 latency in microseconds.
    pub p99_us: Option<u64>,
}

/// Multi-tenant, concurrent-ingest and concurrent-query journey harness.
pub struct SustainedLoadHarness {
    server: Option<WyrdTestServer>,
    workloads: Vec<TenantWorkload>,
    duration: Duration,
    ingest: Option<IngestCallback>,
    query: Option<QueryCallback>,
}

impl SustainedLoadHarness {
    /// Wrap a real [`WyrdTestServer`]. The server remains owned by the harness
    /// until [`Self::shutdown`] is called.
    #[must_use]
    pub fn new(server: WyrdTestServer) -> Self {
        Self {
            server: Some(server),
            workloads: Vec::new(),
            duration: Duration::from_secs(30),
            ingest: None,
            query: None,
        }
    }

    /// Create a callback-only harness for fast unit/integration tests.
    #[must_use]
    pub const fn without_server() -> Self {
        Self {
            server: None,
            workloads: Vec::new(),
            duration: Duration::from_secs(30),
            ingest: None,
            query: None,
        }
    }

    /// Set the fixed load window.
    #[must_use]
    pub const fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Add one tenant workload.
    #[must_use]
    pub fn with_workload(mut self, workload: TenantWorkload) -> Self {
        self.workloads.push(workload);
        self
    }

    /// Set the callback that submits rows to the Scribe/client path.
    #[must_use]
    pub fn with_ingest<F, Fut>(mut self, callback: F) -> Self
    where
        F: Fn(IngestRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<usize, LoadError>> + Send + 'static,
    {
        self.ingest = Some(Arc::new(move |request| Box::pin(callback(request))));
        self
    }

    /// Set the callback that queries the Oracle/client path.
    #[must_use]
    pub fn with_query<F, Fut>(mut self, callback: F) -> Self
    where
        F: Fn(QueryRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<QueryResponse, LoadError>> + Send + 'static,
    {
        self.query = Some(Arc::new(move |request| Box::pin(callback(request))));
        self
    }

    /// Return the wrapped test server, if this harness owns one.
    #[must_use]
    pub fn server(&self) -> Option<&WyrdTestServer> {
        self.server.as_ref()
    }

    /// Run all tenant workloads concurrently for the configured window.
    pub async fn run(&self) -> Result<SustainedLoadReport, LoadError> {
        if self.workloads.is_empty() {
            return Err(LoadError::NoWorkloads);
        }

        let ingest = self.ingest.as_ref().map_or_else(default_ingest, Arc::clone);
        let query = self.query.as_ref().map_or_else(default_query, Arc::clone);
        let mut tasks = JoinSet::new();

        for workload in self.workloads.iter().cloned() {
            let ingest = Arc::clone(&ingest);
            let query = Arc::clone(&query);
            tasks.spawn(run_workload(workload, self.duration, ingest, query));
        }

        let mut reports = Vec::new();
        while let Some(result) = tasks.join_next().await {
            reports.push(result.map_err(|error| LoadError::Task(error.to_string()))??);
        }
        reports.sort_by_key(|report| report.tenant);

        let missing_rows = reports
            .iter()
            .map(|report| report.reconciliation.missing_rows)
            .sum();
        let p99_us = reports
            .iter()
            .filter_map(|report| report.latency.p99_us)
            .max();
        Ok(SustainedLoadReport {
            tenants: reports,
            missing_rows,
            p99_us,
        })
    }

    /// Shut down the wrapped test server after a journey completes.
    pub async fn shutdown(mut self) -> Result<(), LoadError> {
        if let Some(server) = self.server.take() {
            server
                .shutdown()
                .await
                .map_err(|error| LoadError::ServerShutdown(error.to_string()))?;
        }
        Ok(())
    }
}

async fn run_workload(
    workload: TenantWorkload,
    duration: Duration,
    ingest: IngestCallback,
    query: QueryCallback,
) -> Result<TenantLoadReport, LoadError> {
    let deadline = Instant::now() + duration;
    let tenant = workload.tenant;
    let expected_shape = workload.expected_row_shape.clone();
    let ingest_interval = Duration::from_nanos(
        1_000_000_000_u64
            .checked_div(workload.rows_per_sec)
            .unwrap_or(1_000_000_000)
            .max(1),
    );
    let query_interval = Duration::from_millis(10);
    let mut tasks: JoinSet<Result<WorkloadTaskResult, LoadError>> = JoinSet::new();

    let ingest_workload = workload.rows_per_sec;
    let ingest_callback = Arc::clone(&ingest);
    tasks.spawn(async move {
        let mut row_id = 0_u64;
        let mut accepted = 0_u64;
        while row_id == 0 || Instant::now() < deadline {
            let request = IngestRequest {
                tenant,
                row_id,
                row: TenantWorkload::default_row(row_id, tenant),
            };
            let count = ingest_callback(request)
                .await
                .map_err(|error| LoadError::Callback {
                    tenant,
                    message: error.to_string(),
                })?;
            accepted += u64::try_from(count).map_err(|_| LoadError::Callback {
                tenant,
                message: "ingest callback returned a count larger than u64".to_owned(),
            })?;
            row_id = row_id.saturating_add(1);
            if ingest_workload == 0 {
                break;
            }
            tokio::time::sleep(ingest_interval).await;
        }
        Ok::<_, LoadError>(WorkloadTaskResult::Ingested(accepted))
    });

    let query_count = workload.concurrent_query_count.max(1);
    for _ in 0..query_count {
        let query_callback = Arc::clone(&query);
        let query_generator = Arc::clone(&workload.query_generator);
        let expected_shape = expected_shape.clone();
        tasks.spawn(async move {
            let mut sequence = 0_u64;
            let mut query_count = 0_u64;
            let mut queryable_rows = std::collections::HashSet::new();
            let mut samples = Vec::new();
            while sequence == 0 || Instant::now() < deadline {
                let request = QueryRequest {
                    tenant,
                    sequence,
                    query: query_generator(sequence),
                };
                let started = Instant::now();
                let response =
                    query_callback(request)
                        .await
                        .map_err(|error| LoadError::Callback {
                            tenant,
                            message: error.to_string(),
                        })?;
                let elapsed = started.elapsed();
                let sample: u64 = elapsed
                    .as_micros()
                    .try_into()
                    .map_or(u64::MAX, |value| value);
                let sample = sample.max(1);
                samples.push(sample);
                assert_no_cross_tenant_leak(tenant, &response.rows, &expected_shape);
                for row in response.rows {
                    let row_id = row
                        .get(&expected_shape.row_id_field)
                        .map_or_else(|| row.to_string(), Value::to_string);
                    queryable_rows.insert(row_id);
                }
                query_count = query_count.saturating_add(1);
                sequence = sequence.saturating_add(1);
                tokio::time::sleep(query_interval).await;
            }
            Ok::<_, LoadError>(WorkloadTaskResult::Queried {
                query_count,
                rows: queryable_rows,
                histogram: samples,
            })
        });
    }

    let mut ingested_rows = 0_u64;
    let mut query_count = 0_u64;
    let mut queryable_rows = std::collections::HashSet::new();
    let mut latency =
        LatencyHistogram::new().map_err(|error| LoadError::Histogram(error.to_string()))?;
    while let Some(result) = tasks.join_next().await {
        match result.map_err(|error| LoadError::Task(error.to_string()))?? {
            WorkloadTaskResult::Ingested(count) => ingested_rows = count,
            WorkloadTaskResult::Queried {
                query_count: count,
                rows,
                histogram,
            } => {
                query_count += count;
                queryable_rows.extend(rows);
                for sample in histogram.into_iter() {
                    latency
                        .record(Duration::from_micros(sample))
                        .map_err(|error| LoadError::Histogram(error.to_string()))?;
                }
            }
        }
    }

    let queryable_rows = u64::try_from(queryable_rows.len()).map_err(|_| LoadError::Callback {
        tenant,
        message: "queryable row set exceeds u64".to_owned(),
    })?;

    Ok(TenantLoadReport {
        tenant,
        reconciliation: reconcile_row_counts(ingested_rows, queryable_rows),
        query_count,
        latency: latency.summary(),
    })
}

enum WorkloadTaskResult {
    Ingested(u64),
    Queried {
        query_count: u64,
        rows: std::collections::HashSet<String>,
        histogram: Vec<u64>,
    },
}

fn default_ingest() -> IngestCallback {
    Arc::new(|_request| Box::pin(async { Ok(1) }))
}

fn default_query() -> QueryCallback {
    Arc::new(|_request| Box::pin(async { Ok(QueryResponse::default()) }))
}
