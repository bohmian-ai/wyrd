//! The production Drift arm of the closed Verifier dispatch.
//!
//! [`DriftEngine`] loads the run's exact fitted baseline, asks Oracle for
//! fixed server-owned aggregates over the run's subject and frozen
//! `wyrd_event_time` window, and feeds only those aggregates to the existing
//! `vala-drift` scorers. [`ObservationWindow`] owns the typed DataFusion plans:
//! a schema-only scan of `vala.drift.observations` that Oracle replaces with
//! its tenant-authorized provider, the exact subject/series/window filter, and
//! one method-specific aggregate. No raw observation reaches Rust, and no plan
//! carries user SQL: fitted edges, labels, and chunk sizes are typed literals.
//!
//! Input that cannot be scored (an empty Custom window, a null value in a
//! numeric projection, a non-finite mean) completes as `Drift(None)`, the
//! inconclusive result without a report. A transient Oracle or registry
//! failure retries; a missing or mismatched fitted baseline terminates.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{Array, Float64Array, Int64Array, RecordBatch};
use datafusion::arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use datafusion::error::DataFusionError;
use datafusion::functions_aggregate::count::count_all;
use datafusion::functions_aggregate::expr_fn::{avg, count};
use datafusion::functions_window::expr_fn::row_number;
use datafusion::logical_expr::logical_plan::builder::LogicalTableSource;
use datafusion::logical_expr::{
    Expr, ExprFunctionExt, LogicalPlan, LogicalPlanBuilder, cast, col, lit, when,
};
use futures_util::StreamExt as _;
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, Oracle, QueryIpcDecoder, QueryOptions};
use vala_bifrost_redux::tables::builtin_table;
use vala_drift::psi::BinType;
use vala_drift::{
    FittedBaseline, PsiTargetCounts, SpcTargetChunks, score_custom_mean, score_psi_counts,
    score_spc_chunks,
};
use wyrd_runtime::permission::{Permission, PermissionSet};
use wyrd_runtime::{Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::card::drift::{DriftProfile, DriftSpec};
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuthMethod, QueryStreamFrame, QueryTerminalOutcome, VisibilityMode};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::verification::{DriftWindow, VerificationError};
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::verifier_runs::{ClaimedRun, RunInput, TerminalStatus};

use super::engines::{EngineOutcome, VerifierReport};

/// Stable error code when a PSI or SPC Verifier has no ready fitted baseline.
pub const BASELINE_NOT_READY: &str = "baseline_not_ready";
/// Stable error code when this process hosts no ready Oracle to plan against.
pub const ORACLE_UNAVAILABLE: &str = "oracle_unavailable";
/// Stable error code when the aggregate query failed transiently.
pub const DRIFT_QUERY_FAILED: &str = "drift_query_failed";
/// Stable error code when the Verifier, baseline, and run cannot be scored together.
pub const DRIFT_INVALID: &str = "drift_invalid";

/// Canonical Bifrost table every Drift plan scans.
const OBSERVATIONS: &str = "vala.drift.observations";
/// Aggregate bin of a null numeric value: the window is not scorable.
const NULL_BIN: i64 = -2;
/// Aggregate bin of a category absent from the fitted baseline.
const UNKNOWN_BIN: i64 = -1;

/// Build a terminal `errored` outcome with `code` and `message`.
fn terminal(code: &str, message: impl Into<String>) -> EngineOutcome {
    EngineOutcome::Terminal(
        TerminalStatus::Errored,
        VerificationError {
            code: code.to_owned(),
            message: message.into(),
        },
    )
}

/// Build a retryable outcome with `code` and `message`.
fn retry(code: &str, message: impl Into<String>) -> EngineOutcome {
    EngineOutcome::Retry(VerificationError {
        code: code.to_owned(),
        message: message.into(),
    })
}

/// One run's subject and frozen ingest-time window over the observation table.
///
/// A pure value: it builds the fixed logical plans and never executes them.
/// Every plan filters the exact subject `card_uid`, one feature `series`, and
/// `wyrd_event_time` in `[start, end)` before its method-specific aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationWindow {
    /// Observed subject Card UID as the managed `card_uid` column stores it.
    subject: String,
    /// Inclusive window start, microseconds since the Unix epoch, UTC.
    start_micros: i64,
    /// Exclusive window end, microseconds since the Unix epoch, UTC.
    end_micros: i64,
}

impl ObservationWindow {
    /// Bind `subject` and the frozen `window`.
    #[must_use]
    pub fn new(subject: &CardUid, window: &DriftWindow) -> Self {
        Self {
            subject: subject.to_string(),
            start_micros: window.start.timestamp_micros(),
            end_micros: window.end.timestamp_micros(),
        }
    }

    /// Scan the observation table and keep only this window's `series` rows.
    ///
    /// The scan source is schema-only; Oracle swaps in the tenant-authorized
    /// provider for the canonical table name before execution.
    ///
    /// # Errors
    /// Returns a planning error when the built-in table is unknown or
    /// DataFusion rejects the filter.
    fn series(&self, series: &str) -> Result<LogicalPlanBuilder, DataFusionError> {
        let table = builtin_table("drift", "observations").ok_or_else(|| {
            DataFusionError::Plan("vala.drift.observations is not a built-in table".to_owned())
        })?;
        let source = Arc::new(LogicalTableSource::new((table.schema)()));
        let micros = |value| {
            lit(ScalarValue::TimestampMicrosecond(
                Some(value),
                Some("UTC".into()),
            ))
        };
        LogicalPlanBuilder::scan(OBSERVATIONS, source, None)?.filter(
            col("card_uid")
                .eq(lit(self.subject.as_str()))
                .and(col("series").eq(lit(series)))
                .and(col("wyrd_event_time").gt_eq(micros(self.start_micros)))
                .and(col("wyrd_event_time").lt(micros(self.end_micros))),
        )
    }

    /// Count `series` values into fitted `(lower, upper]` numeric bins.
    ///
    /// `edges` are the fitted bin edges (`bins + 1` values, open outer
    /// edges). Returns `(bin_id, n)` rows; a null value lands in
    /// [`NULL_BIN`] so the caller can refuse to score the window.
    ///
    /// # Errors
    /// Returns a planning error when `edges` has fewer than two entries or
    /// DataFusion rejects the plan.
    pub fn psi_numeric(&self, series: &str, edges: &[f64]) -> Result<LogicalPlan, DataFusionError> {
        let bins = edges
            .len()
            .checked_sub(1)
            .filter(|bins| *bins > 0)
            .ok_or_else(|| {
                DataFusionError::Plan("a numeric PSI feature needs at least one bin".to_owned())
            })?;
        let value = || col("num_value");
        let mut case = when(value().is_null(), lit(NULL_BIN));
        for (index, upper) in edges[1..bins].iter().enumerate() {
            case.when(value().lt_eq(lit(*upper)), lit(bin_id(index)?));
        }
        let bin = case.otherwise(lit(bin_id(bins - 1)?))?;
        Self::count_bins(self.series(series)?, bin)
    }

    /// Count non-null `series` categories into fitted labels.
    ///
    /// Returns `(bin_id, n)` rows; a category absent from `labels` lands in
    /// [`UNKNOWN_BIN`], which counts toward the target total only.
    ///
    /// # Errors
    /// Returns a planning error when DataFusion rejects the plan.
    pub fn psi_categorical(
        &self,
        series: &str,
        labels: &[&str],
    ) -> Result<LogicalPlan, DataFusionError> {
        let value = || col("str_value");
        let bin = match labels.split_first() {
            None => lit(UNKNOWN_BIN),
            Some((first, rest)) => {
                let mut case = when(value().eq(lit(*first)), lit(0_i64));
                for (index, label) in rest.iter().enumerate() {
                    case.when(value().eq(lit(*label)), lit(bin_id(index + 1)?));
                }
                case.otherwise(lit(UNKNOWN_BIN))?
            }
        };
        Self::count_bins(self.series(series)?.filter(value().is_not_null())?, bin)
    }

    /// Group `input` by `bin` and count each group as `(bin_id, n)`.
    ///
    /// # Errors
    /// Returns a planning error when DataFusion rejects the aggregate.
    fn count_bins(input: LogicalPlanBuilder, bin: Expr) -> Result<LogicalPlan, DataFusionError> {
        input
            .aggregate(vec![bin.alias("bin_id")], vec![count_all().alias("n")])?
            .build()
    }

    /// Form consecutive `chunk_size` subgroups of `series` in observation order.
    ///
    /// Rows are numbered by `created_at`, then `record_id`; subgroup `k`
    /// holds rows `k·chunk_size ..` and the last may be a shorter trailing
    /// chunk, as the existing scorer forms them. Returns
    /// `(chunk, n, numeric, mean)` ordered by chunk; `n != numeric` exposes a
    /// null value.
    ///
    /// # Errors
    /// Returns a planning error when `chunk_size` is zero or DataFusion
    /// rejects the plan.
    pub fn spc(&self, series: &str, chunk_size: u32) -> Result<LogicalPlan, DataFusionError> {
        if chunk_size == 0 {
            return Err(DataFusionError::Plan(
                "an SPC chunk size is positive".to_owned(),
            ));
        }
        let rn = row_number()
            .order_by(vec![
                col("created_at").sort(true, false),
                col("record_id").sort(true, false),
            ])
            .build()?
            .alias("rn");
        let chunk = cast(
            (cast(col("rn"), DataType::Int64) - lit(1_i64)) / lit(i64::from(chunk_size)),
            DataType::Int64,
        );
        self.series(series)?
            .window(vec![rn])?
            .aggregate(
                vec![chunk.alias("chunk")],
                vec![
                    count_all().alias("n"),
                    count(col("num_value")).alias("numeric"),
                    avg(col("num_value")).alias("mean"),
                ],
            )?
            .sort(vec![col("chunk").sort(true, false)])?
            .build()
    }

    /// Summarize `series` as one `(n, numeric, mean)` row.
    ///
    /// # Errors
    /// Returns a planning error when DataFusion rejects the plan.
    pub fn custom(&self, series: &str) -> Result<LogicalPlan, DataFusionError> {
        self.series(series)?
            .aggregate(
                Vec::<Expr>::new(),
                vec![
                    count_all().alias("n"),
                    count(col("num_value")).alias("numeric"),
                    avg(col("num_value")).alias("mean"),
                ],
            )?
            .build()
    }
}

/// Convert a fitted bin index into its `bin_id` literal.
///
/// # Errors
/// Returns a planning error when the index does not fit in `i64`.
fn bin_id(index: usize) -> Result<i64, DataFusionError> {
    i64::try_from(index).map_err(|_| DataFusionError::Plan("too many fitted bins".to_owned()))
}

/// Read a non-null `Int64` column `name` of `batch`.
///
/// # Errors
/// Returns a description when the column is missing, not `Int64`, or null.
fn int64s<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int64Array, String> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .filter(|column| column.null_count() == 0)
        .ok_or_else(|| format!("aggregate column {name} is not a non-null Int64"))
}

/// Read a `Float64` column `name` of `batch`.
///
/// # Errors
/// Returns a description when the column is missing or not `Float64`.
fn float64s<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Float64Array, String> {
    batch
        .column_by_name(name)
        .and_then(|column| column.as_any().downcast_ref::<Float64Array>())
        .ok_or_else(|| format!("aggregate column {name} is not Float64"))
}

/// Convert an aggregate count to `u64`.
///
/// # Errors
/// Returns a description when the count is negative.
fn count_of(value: i64) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| "aggregate count is negative".to_owned())
}

/// Fold `(bin_id, n)` rows into target counts over `bins` fitted bins.
///
/// Absent bins stay zero; [`UNKNOWN_BIN`] adds to the total only. Returns
/// `Ok(None)` when a null value was counted, which leaves the window
/// unscorable.
///
/// # Errors
/// Returns a description for a malformed aggregate or an out-of-range bin.
pub fn psi_counts(batches: &[RecordBatch], bins: usize) -> Result<Option<PsiTargetCounts>, String> {
    let mut counts = PsiTargetCounts {
        bins: vec![0; bins],
        total: 0,
    };
    for batch in batches {
        let (ids, ns) = (int64s(batch, "bin_id")?, int64s(batch, "n")?);
        for (id, n) in ids.values().iter().zip(ns.values()) {
            let n = count_of(*n)?;
            match *id {
                NULL_BIN => return Ok(None),
                UNKNOWN_BIN => {}
                id => {
                    let slot = usize::try_from(id)
                        .ok()
                        .and_then(|index| counts.bins.get_mut(index))
                        .ok_or_else(|| format!("aggregate bin {id} is not a fitted bin"))?;
                    *slot += n;
                }
            }
            counts.total += n;
        }
    }
    Ok(Some(counts))
}

/// Fold ordered `(chunk, n, numeric, mean)` rows into SPC target chunks.
///
/// Returns `Ok(None)` when any chunk counted a null value.
///
/// # Errors
/// Returns a description for a malformed aggregate.
pub fn spc_chunks(batches: &[RecordBatch]) -> Result<Option<SpcTargetChunks>, String> {
    let mut chunks = SpcTargetChunks {
        rows: 0,
        means: Vec::new(),
    };
    for batch in batches {
        let (ns, numeric, means) = (
            int64s(batch, "n")?,
            int64s(batch, "numeric")?,
            float64s(batch, "mean")?,
        );
        for row in 0..batch.num_rows() {
            if ns.value(row) != numeric.value(row) || means.is_null(row) {
                return Ok(None);
            }
            chunks.rows += count_of(ns.value(row))?;
            chunks.means.push(means.value(row));
        }
    }
    Ok(Some(chunks))
}

/// Read the single Custom `(n, numeric, mean)` row as a scorable mean.
///
/// Returns `Ok(None)` for an empty window, a null value, or a non-finite mean.
///
/// # Errors
/// Returns a description when the aggregate is not exactly one row.
pub fn custom_mean(batches: &[RecordBatch]) -> Result<Option<f64>, String> {
    let mut rows = batches.iter().filter(|batch| batch.num_rows() > 0);
    let (Some(batch), None) = (rows.next(), rows.next()) else {
        return Err("the Custom aggregate is not exactly one row".to_owned());
    };
    if batch.num_rows() != 1 {
        return Err("the Custom aggregate is not exactly one row".to_owned());
    }
    let (n, numeric, mean) = (
        int64s(batch, "n")?.value(0),
        int64s(batch, "numeric")?.value(0),
        float64s(batch, "mean")?,
    );
    Ok(
        (n > 0 && n == numeric && mean.is_valid(0) && mean.value(0).is_finite())
            .then(|| mean.value(0)),
    )
}

/// Owner of Drift execution: baseline loading, Oracle aggregates, scoring.
pub struct DriftEngine {
    /// Wyrd Postgres owner for the fitted baseline and SYSTEM principal reads.
    postgres: WyrdPostgres,
    /// This process's Oracle; `None` when the process hosts no Oracle role.
    oracle: Option<Arc<Oracle>>,
    /// Fitted baseline reads.
    baselines: DriftBaselineQueue,
    /// Deadline of one aggregate query.
    query_timeout: Duration,
}

impl DriftEngine {
    /// Build an engine over `postgres` and this process's `oracle`.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        oracle: Option<Arc<Oracle>>,
        query_timeout: Duration,
    ) -> Self {
        Self {
            postgres,
            oracle,
            baselines: DriftBaselineQueue::default(),
            query_timeout,
        }
    }

    /// Execute one claimed Drift run of `verifier` for `tenant`.
    ///
    /// Loads the fitted baseline (PSI/SPC), plans one aggregate per feature,
    /// runs each through Oracle as the tenant's SYSTEM reader scoped to
    /// `verifier`, and scores the aggregates. Never fails: every failure is
    /// the [`EngineOutcome`] it maps to.
    pub async fn verify(
        &self,
        tenant: DataTenantId,
        verifier: &CardRef,
        run: &ClaimedRun,
        spec: &DriftSpec,
    ) -> EngineOutcome {
        match self.try_verify(tenant, verifier, run, spec).await {
            Ok(report) => EngineOutcome::Completed(VerifierReport::Drift(report)),
            Err(outcome) => outcome,
        }
    }

    /// Execute a run, returning the scored report or `None` when unscorable.
    ///
    /// # Errors
    /// Returns the terminal or retry outcome of the first failure.
    async fn try_verify(
        &self,
        tenant: DataTenantId,
        verifier: &CardRef,
        run: &ClaimedRun,
        spec: &DriftSpec,
    ) -> Result<Option<vala_drift::DriftReport>, EngineOutcome> {
        let RunInput::DriftWindow(window) = &run.input else {
            return Err(terminal(
                DRIFT_INVALID,
                "a Drift run needs a drift window input",
            ));
        };
        let window = ObservationWindow::new(&run.subject_card_uid, window);
        let invalid = |message: String| terminal(DRIFT_INVALID, message);
        let scored = |result: Result<vala_drift::DriftReport, vala_drift::DriftScoreError>| {
            result.map(Some).map_err(|error| invalid(error.to_string()))
        };
        match spec.profile.as_ref() {
            Some(DriftProfile::Custom(profile)) => {
                let plan = window
                    .custom(&profile.metric_name)
                    .map_err(|e| invalid(e.to_string()))?;
                let batches = self.aggregate(tenant, verifier, plan).await?;
                match custom_mean(&batches).map_err(invalid)? {
                    Some(mean) => scored(score_custom_mean(mean, profile)),
                    None => Ok(None),
                }
            }
            Some(DriftProfile::Psi(profile)) => {
                let FittedBaseline::Psi(baseline) = self.fitted(tenant, &run.verifier_uid).await?
                else {
                    return Err(invalid(
                        "the fitted baseline is not a PSI baseline".to_owned(),
                    ));
                };
                let mut counts = BTreeMap::new();
                for (name, feature) in &baseline.features {
                    let plan = match feature.bin_type {
                        BinType::Numeric => feature
                            .numeric_edges()
                            .map_err(|e| invalid(e.to_string()))
                            .and_then(|edges| {
                                window
                                    .psi_numeric(name.as_str(), &edges)
                                    .map_err(|e| invalid(e.to_string()))
                            })?,
                        BinType::Categorical => {
                            let labels = feature
                                .bins
                                .iter()
                                .map(|bin| bin.categorical_value.as_deref().unwrap_or_default())
                                .collect::<Vec<_>>();
                            window
                                .psi_categorical(name.as_str(), &labels)
                                .map_err(|e| invalid(e.to_string()))?
                        }
                    };
                    let batches = self.aggregate(tenant, verifier, plan).await?;
                    let Some(feature_counts) =
                        psi_counts(&batches, feature.bins.len()).map_err(invalid)?
                    else {
                        return Ok(None);
                    };
                    counts.insert(name.clone(), feature_counts);
                }
                scored(score_psi_counts(&baseline, &counts, profile))
            }
            Some(DriftProfile::Spc(profile)) => {
                let FittedBaseline::Spc(baseline) = self.fitted(tenant, &run.verifier_uid).await?
                else {
                    return Err(invalid(
                        "the fitted baseline is not an SPC baseline".to_owned(),
                    ));
                };
                let mut chunks = BTreeMap::new();
                for name in baseline.features.keys() {
                    let plan = window
                        .spc(name.as_str(), baseline.chunk_size)
                        .map_err(|e| invalid(e.to_string()))?;
                    let batches = self.aggregate(tenant, verifier, plan).await?;
                    let Some(feature_chunks) = spc_chunks(&batches).map_err(invalid)? else {
                        return Ok(None);
                    };
                    chunks.insert(name.clone(), feature_chunks);
                }
                scored(score_spc_chunks(&baseline, &chunks, profile))
            }
            None => Err(invalid("the Drift Verifier has no profile".to_owned())),
        }
    }

    /// Load the ready fitted baseline of `verifier_uid`.
    ///
    /// # Errors
    /// Retries a registry failure; terminates when no baseline is ready or the
    /// stored profile does not decode.
    async fn fitted(
        &self,
        tenant: DataTenantId,
        verifier_uid: &CardUid,
    ) -> Result<FittedBaseline, EngineOutcome> {
        let unavailable =
            |error: &dyn std::fmt::Display| retry(DRIFT_QUERY_FAILED, error.to_string());
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let fitted = self
            .baselines
            .fitted(&mut conn, verifier_uid)
            .await
            .map_err(|error| unavailable(&error))?
            .ok_or_else(|| terminal(BASELINE_NOT_READY, "the Drift baseline is not fitted"))?;
        serde_json::from_value(fitted).map_err(|error| terminal(DRIFT_INVALID, error.to_string()))
    }

    /// Build the tenant SYSTEM reader context scoped to `verifier`.
    ///
    /// The SYSTEM principal is the tenant's persisted internal identity; it
    /// holds only Bifrost query read, and Oracle audits the read decision.
    ///
    /// # Errors
    /// Retries when the principal cannot be read or the tenant has none.
    async fn context(
        &self,
        tenant: DataTenantId,
        verifier: &CardRef,
    ) -> Result<AuthorizedQueryContext, EngineOutcome> {
        let unavailable =
            |error: &dyn std::fmt::Display| retry(DRIFT_QUERY_FAILED, error.to_string());
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let principal = system_principal_id(&mut conn)
            .await
            .map_err(|error| unavailable(&error))?
            .ok_or_else(|| retry(DRIFT_QUERY_FAILED, "the tenant has no SYSTEM principal"))?;
        drop(conn);
        let permission = Permission::bifrost_query_read();
        AuthorizedQueryContext::try_new(
            Principal::new(
                PrincipalId::new(principal),
                PrincipalKind::System {
                    card_ref_scope: CardRefScope::own(verifier),
                },
                tenant,
                Vec::new(),
                PermissionSet::from_iter([permission.clone()]),
            ),
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            permission,
        )
        .map_err(|error| unavailable(&error))
    }

    /// Run `plan` through Oracle and collect its aggregate batches.
    ///
    /// A tenant that has never written an observation owns no table yet,
    /// which is an empty window rather than a failure.
    ///
    /// # Errors
    /// Retries when no Oracle is hosted here, or the query is refused, fails,
    /// or its stream is malformed or incomplete.
    async fn aggregate(
        &self,
        tenant: DataTenantId,
        verifier: &CardRef,
        plan: LogicalPlan,
    ) -> Result<Vec<RecordBatch>, EngineOutcome> {
        let Some(oracle) = &self.oracle else {
            return Err(retry(ORACLE_UNAVAILABLE, "this process hosts no Oracle"));
        };
        let context = self.context(tenant, verifier).await?;
        let failed = |error: BifrostError| retry(DRIFT_QUERY_FAILED, error.to_string());
        let options = QueryOptions {
            visibility: VisibilityMode::Fused,
            deadline: Instant::now() + self.query_timeout,
        };
        let mut stream = match oracle.query_plan(context, plan, options).await {
            Ok(stream) => stream,
            Err(BifrostError::TableNotFound { .. }) => return Ok(Vec::new()),
            Err(error) => return Err(failed(error)),
        };
        let mut decoder = QueryIpcDecoder::new();
        let mut batches = Vec::new();
        let decode = |error: &dyn std::fmt::Display| retry(DRIFT_QUERY_FAILED, error.to_string());
        while let Some(frame) = stream.frames.next().await {
            match frame.map_err(failed)? {
                QueryStreamFrame::Schema(schema) => {
                    decoder
                        .accept_schema(&schema.arrow_ipc_schema)
                        .map_err(|error| decode(&error))?;
                }
                QueryStreamFrame::Batch(batch) => batches.push(
                    decoder
                        .accept_batch(&batch.arrow_ipc_batch)
                        .map_err(|error| decode(&error))?,
                ),
                QueryStreamFrame::Terminal(terminal) => {
                    if terminal.outcome == QueryTerminalOutcome::Failed {
                        return Err(failed(
                            terminal
                                .error
                                .map_or(BifrostError::QueryExecutionFailed, |error| {
                                    crate::query::service::terminal_error_to_bifrost(error.code)
                                }),
                        ));
                    }
                    decoder
                        .accept_eos(&terminal.arrow_ipc_eos)
                        .map_err(|error| decode(&error))?;
                    return Ok(batches);
                }
            }
        }
        Err(failed(BifrostError::QueryStreamIncomplete))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{Float64Array, Int64Array, RecordBatch};
    use arrow::datatypes::{DataType, Field, Schema};
    use chrono::{TimeZone as _, Utc};

    use super::*;

    /// Build a batch of named `Int64` and `Float64` columns.
    ///
    /// # Panics
    /// Panics when the columns do not form a valid batch.
    fn batch(ints: &[(&str, Vec<i64>)], floats: &[(&str, Vec<Option<f64>>)]) -> RecordBatch {
        let mut fields = Vec::new();
        let mut columns: Vec<Arc<dyn Array>> = Vec::new();
        for (name, values) in ints {
            fields.push(Field::new(*name, DataType::Int64, false));
            columns.push(Arc::new(Int64Array::from(values.clone())));
        }
        for (name, values) in floats {
            fields.push(Field::new(*name, DataType::Float64, true));
            columns.push(Arc::new(Float64Array::from(values.clone())));
        }
        RecordBatch::try_new(Arc::new(Schema::new(fields)), columns).expect("valid batch")
    }

    /// A window over one subject and a one-hour span.
    fn window() -> ObservationWindow {
        ObservationWindow::new(
            &CardUid::from_uuid(uuid::Uuid::now_v7()).expect("a v7 UUID is a Card UID"),
            &DriftWindow {
                start: Utc
                    .with_ymd_and_hms(2026, 9, 17, 0, 0, 0)
                    .single()
                    .expect("start"),
                end: Utc
                    .with_ymd_and_hms(2026, 9, 17, 1, 0, 0)
                    .single()
                    .expect("end"),
            },
        )
    }

    /// Every fixed plan builds, filters the exact subject/series/window, and
    /// returns only its aggregate columns.
    #[test]
    fn plans_filter_the_window_and_return_aggregates_only() {
        let window = window();
        let plans = [
            (
                window
                    .psi_numeric("age", &[f64::NEG_INFINITY, 10.0, 20.0, f64::INFINITY])
                    .expect("numeric plan"),
                vec!["bin_id", "n"],
            ),
            (
                window
                    .psi_categorical("color", &["red", "blue"])
                    .expect("categorical plan"),
                vec!["bin_id", "n"],
            ),
            (
                window.spc("age", 5).expect("spc plan"),
                vec!["chunk", "n", "numeric", "mean"],
            ),
            (
                window.custom("latency").expect("custom plan"),
                vec!["n", "numeric", "mean"],
            ),
        ];
        for (plan, columns) in plans {
            let names = plan
                .schema()
                .fields()
                .iter()
                .map(|field| field.name().as_str())
                .collect::<Vec<_>>();
            assert_eq!(names, columns);
            let rendered = plan.display_indent().to_string();
            assert!(
                rendered.contains("TableScan: vala.drift.observations"),
                "{rendered}"
            );
            assert!(rendered.contains(&window.subject), "{rendered}");
            assert!(rendered.contains("wyrd_event_time >="), "{rendered}");
            assert!(rendered.contains("wyrd_event_time <"), "{rendered}");
        }
        assert!(window.spc("age", 0).is_err());
        assert!(window.psi_numeric("age", &[0.0]).is_err());
    }

    /// PSI counts zero-fill absent bins, add unknown categories to the total
    /// only, and refuse a window with a null value.
    #[test]
    fn psi_counts_zero_fill_and_count_unknowns() {
        let rows = batch(&[("bin_id", vec![2, -1, 0]), ("n", vec![5, 3, 7])], &[]);
        let counts = psi_counts(&[rows], 4).expect("counts").expect("scorable");
        assert_eq!(counts.bins, vec![7, 0, 5, 0]);
        assert_eq!(counts.total, 15);
        let null = batch(&[("bin_id", vec![0, -2]), ("n", vec![5, 1])], &[]);
        assert_eq!(psi_counts(&[null], 2).expect("counts"), None);
        let stray = batch(&[("bin_id", vec![9]), ("n", vec![1])], &[]);
        assert!(psi_counts(&[stray], 2).is_err());
        assert_eq!(
            psi_counts(&[], 2).expect("empty"),
            Some(PsiTargetCounts {
                bins: vec![0, 0],
                total: 0
            })
        );
    }

    /// SPC chunks keep order, sum rows, and refuse a chunk with a null value.
    #[test]
    fn spc_chunks_keep_order_and_refuse_nulls() {
        let rows = batch(
            &[
                ("chunk", vec![0, 1]),
                ("n", vec![5, 2]),
                ("numeric", vec![5, 2]),
            ],
            &[("mean", vec![Some(1.5), Some(3.0)])],
        );
        let chunks = spc_chunks(&[rows]).expect("chunks").expect("scorable");
        assert_eq!(chunks.rows, 7);
        assert_eq!(chunks.means, vec![1.5, 3.0]);
        let null = batch(
            &[("chunk", vec![0]), ("n", vec![5]), ("numeric", vec![4])],
            &[("mean", vec![Some(1.0)])],
        );
        assert_eq!(spc_chunks(&[null]).expect("chunks"), None);
    }

    /// The Custom mean is scorable only for a non-empty, all-numeric, finite window.
    #[test]
    fn custom_mean_requires_a_complete_finite_window() {
        let row = |n, numeric, mean| {
            batch(
                &[("n", vec![n]), ("numeric", vec![numeric])],
                &[("mean", vec![mean])],
            )
        };
        assert_eq!(
            custom_mean(&[row(3, 3, Some(2.5))]).expect("row"),
            Some(2.5)
        );
        assert_eq!(custom_mean(&[row(0, 0, None)]).expect("row"), None);
        assert_eq!(custom_mean(&[row(3, 2, Some(2.5))]).expect("row"), None);
        assert_eq!(
            custom_mean(&[row(3, 3, Some(f64::INFINITY))]).expect("row"),
            None
        );
        assert!(custom_mean(&[]).is_err());
    }
}
