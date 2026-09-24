//! The production Drift arm of the closed Verifier dispatch.
//!
//! [`DriftEngine`] loads the run's exact fitted baseline, asks Oracle for
//! fixed server-owned aggregates over the run's subject and frozen
//! `wyrd_event_time` window, and feeds only those aggregates to the existing
//! `vala-drift` scorers. [`ObservationWindow`] renders the fixed SQL: the
//! canonical `vala.drift.observations` table, the exact subject/series/window
//! filter, and one method-specific aggregate. The subject UID, feature name,
//! fitted edges and labels, and window bounds are escaped typed literals; a
//! Verifier contributes no SQL text.
//!
//! Each statement runs as the tenant's SYSTEM Drift reader: the engine mints a
//! token holding only `bifrost_query:read` on the tenant's registered
//! observation table, verifies it through the server's ordinary token
//! verifier, and dispatches through the ordinary query service — capability
//! admission, Gate, and a local or peer-forwarded Oracle — so no Oracle need
//! run in this process. Oracle's table authorization enforces the read and
//! records the read decision. The shared scheduled-query consumer settles
//! every stream, and each decoded aggregate batch is folded as it arrives.
//!
//! PSI and SPC first check completeness: a selected observation is a
//! `record_id` carrying at least one configured feature in the window, and
//! when any selected observation omits a configured feature or holds a null
//! or non-finite value the run completes as `Drift(None)`, the inconclusive
//! result without a report; nothing is dropped or imputed. An empty Custom
//! window and a non-finite Custom mean are unscorable the same way. A
//! transient mint, query, or registry failure retries; a missing, legacy, or
//! mismatched fitted baseline or a malformed aggregate terminates.

use std::collections::BTreeMap;
use std::time::Duration;

use arrow::array::{Array, Float64Array, Int64Array, RecordBatch};
use chrono::{DateTime, SecondsFormat, Utc};
use datafusion::sql::sqlparser::ast::Value;
use tokio_util::sync::CancellationToken;
use vala_drift::psi::BinType;
use vala_drift::{FITTED_FORMAT, FittedBaseline, SpcScorer, score_custom_mean, score_psi_counts};
use wyrd_auth::issuance::TenantTokenIssuer;
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::{DriftProfile, DriftSpec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::verification::{DriftWindow, VerificationError};
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::verifier_runs::{ClaimedRun, RunInput, TerminalStatus};

use super::engines::{EngineOutcome, VerifierReport};
use crate::components::auth::Caller;
use crate::query::scheduled::ScheduledQueryCaller;
use crate::state::AppState;

/// Stable error code when a PSI or SPC Verifier has no ready fitted baseline.
pub const BASELINE_NOT_READY: &str = "baseline_not_ready";
/// Stable error code when the reader could not be minted or the query failed.
pub const DRIFT_QUERY_FAILED: &str = "drift_query_failed";
/// Stable error code when the Verifier, baseline, and run cannot be scored together.
pub const DRIFT_INVALID: &str = "drift_invalid";
/// Stable error code when a PSI or SPC baseline was fitted before the current
/// fitted-profile format; the Verifier needs a new version and a new fit.
pub const BASELINE_LEGACY: &str = "baseline_legacy";

/// Canonical Bifrost table every Drift statement reads.
const OBSERVATIONS: &str = "vala.drift.observations";
/// Aggregate bin of a null or non-finite value: the window is not scorable.
const INVALID_BIN: i64 = -1;
/// SQL predicate true only for a finite `num_value`: null and NaN compare
/// false (IEEE) or outside the range (total order), and so do infinities.
const FINITE_NUM: &str =
    "COALESCE(num_value BETWEEN -1.7976931348623157e308 AND 1.7976931348623157e308, false)";

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
/// A pure value: it renders the fixed SQL and never executes it. Every
/// statement filters the exact subject `card_uid`, one feature `series`, and
/// `wyrd_event_time` in `[start, end)` before its method-specific aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationWindow {
    /// Observed subject Card UID as the managed `card_uid` column stores it.
    subject: String,
    /// Inclusive window start.
    start: DateTime<Utc>,
    /// Exclusive window end.
    end: DateTime<Utc>,
}

impl ObservationWindow {
    /// Bind `subject` and the frozen `window`.
    #[must_use]
    pub fn new(subject: &CardUid, window: &DriftWindow) -> Self {
        Self {
            subject: subject.to_string(),
            start: window.start,
            end: window.end,
        }
    }

    /// Render `value` as an escaped SQL string literal.
    fn text(value: &str) -> String {
        Value::SingleQuotedString(value.to_owned()).to_string()
    }

    /// Render `value` as a microsecond UTC timestamp string literal, which
    /// Oracle coerces to the `wyrd_event_time` column type.
    fn instant(value: DateTime<Utc>) -> String {
        Self::text(&value.to_rfc3339_opts(SecondsFormat::Micros, true))
    }

    /// The shared `FROM ... WHERE` clause selecting this window's `series` rows.
    fn rows(&self, series: &str) -> String {
        self.rows_in(&format!("series = {}", Self::text(series)))
    }

    /// The `FROM ... WHERE` clause selecting this window's subject rows that
    /// also satisfy `filter`.
    fn rows_in(&self, filter: &str) -> String {
        format!(
            "FROM {OBSERVATIONS} WHERE card_uid = {} AND {filter} \
             AND wyrd_event_time >= {} AND wyrd_event_time < {}",
            Self::text(&self.subject),
            Self::instant(self.start),
            Self::instant(self.end),
        )
    }

    /// Count the selected observations that are incomplete.
    ///
    /// Selected observations are the `record_id`s carrying at least one of
    /// the configured `numeric` or `categorical` series in the window;
    /// unrelated records never match. One is incomplete when it omits a
    /// configured series or any of its configured values is invalid: a
    /// numeric value that is null or non-finite, or a null category. Returns
    /// one `incomplete` row.
    ///
    /// # Errors
    /// Returns a description when no series is configured.
    pub fn incomplete(&self, numeric: &[&str], categorical: &[&str]) -> Result<String, String> {
        let list = |names: &[&str]| {
            names
                .iter()
                .map(|name| Self::text(name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let configured = numeric.len() + categorical.len();
        if configured == 0 {
            return Err("a Drift distribution needs at least one feature".to_owned());
        }
        let valid = if numeric.is_empty() {
            "str_value IS NOT NULL".to_owned()
        } else {
            format!(
                "CASE WHEN series IN ({}) THEN {FINITE_NUM} ELSE str_value IS NOT NULL END",
                list(numeric)
            )
        };
        let all = list(&[numeric, categorical].concat());
        Ok(format!(
            "SELECT COUNT(*) AS incomplete FROM (SELECT record_id {} GROUP BY record_id \
             HAVING COUNT(DISTINCT series) < {configured} \
             OR SUM(CASE WHEN {valid} THEN 0 ELSE 1 END) > 0)",
            self.rows_in(&format!("series IN ({all})"))
        ))
    }

    /// Group `bin` over `rows` and count each group as `(bin_id, n)`.
    fn count_bins(bin: &str, rows: &str) -> String {
        format!("SELECT bin_id, COUNT(*) AS n FROM (SELECT {bin} AS bin_id {rows}) GROUP BY bin_id")
    }

    /// Count `series` values into fitted `(lower, upper]` numeric bins.
    ///
    /// `edges` are the fitted bin edges (`bins + 1` values, open outer
    /// edges). Returns `(bin_id, n)` rows; a null or non-finite value lands
    /// in [`INVALID_BIN`] so the caller can refuse to score the window.
    ///
    /// # Errors
    /// Returns a description when `edges` has fewer than two entries or an
    /// inner edge is not finite.
    pub fn psi_numeric(&self, series: &str, edges: &[f64]) -> Result<String, String> {
        let bins = edges
            .len()
            .checked_sub(1)
            .filter(|bins| *bins > 0)
            .ok_or("a numeric PSI feature needs at least one bin")?;
        let mut case = format!("CASE WHEN NOT {FINITE_NUM} THEN {INVALID_BIN}");
        for (index, upper) in edges[1..bins].iter().enumerate() {
            if !upper.is_finite() {
                return Err("a fitted inner PSI edge is not finite".to_owned());
            }
            case.push_str(&format!(" WHEN num_value <= {upper:e} THEN {index}"));
        }
        case.push_str(&format!(" ELSE {} END", bins - 1));
        Ok(Self::count_bins(&case, &self.rows(series)))
    }

    /// Count `series` categories into fitted labels and the `other` bin.
    ///
    /// `labels` are the fitted labels in bin order and `other` the index of
    /// the reserved bin every unseen category lands in. Returns `(bin_id, n)`
    /// rows; a null category lands in [`INVALID_BIN`].
    #[must_use]
    pub fn psi_categorical(&self, series: &str, labels: &[&str], other: usize) -> String {
        let mut case = format!("CASE WHEN str_value IS NULL THEN {INVALID_BIN}");
        for (index, label) in labels.iter().enumerate() {
            case.push_str(&format!(
                " WHEN str_value = {} THEN {index}",
                Self::text(label)
            ));
        }
        case.push_str(&format!(" ELSE {other} END"));
        Self::count_bins(&case, &self.rows(series))
    }

    /// Form consecutive `subgroup_size` subgroups of `series` in observation
    /// order.
    ///
    /// Rows are numbered by `created_at`, then `record_id`; subgroup `k`
    /// holds rows `k·subgroup_size ..`, and only the last may be partial.
    /// Returns `(subgroup, n, numeric_n, mean, sd)` ordered by subgroup, with
    /// `sd` the sample standard deviation; `n != numeric_n` exposes a null.
    ///
    /// # Errors
    /// Returns a description when `subgroup_size` is below two.
    pub fn spc(&self, series: &str, subgroup_size: u32) -> Result<String, String> {
        if subgroup_size < 2 {
            return Err("an SPC subgroup holds at least two rows".to_owned());
        }
        Ok(format!(
            "SELECT subgroup, COUNT(*) AS n, COUNT(num_value) AS numeric_n, \
             AVG(num_value) AS mean, STDDEV_SAMP(num_value) AS sd \
             FROM (SELECT num_value, CAST((ROW_NUMBER() OVER (ORDER BY created_at ASC NULLS LAST, \
             record_id ASC NULLS LAST) - 1) / {subgroup_size} AS BIGINT) AS subgroup {}) \
             GROUP BY subgroup ORDER BY subgroup",
            self.rows(series)
        ))
    }

    /// Summarize `series` as one `(n, numeric_n, mean)` row.
    #[must_use]
    pub fn custom(&self, series: &str) -> String {
        format!(
            "SELECT COUNT(*) AS n, COUNT(num_value) AS numeric_n, AVG(num_value) AS mean {}",
            self.rows(series)
        )
    }
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

/// Fold one batch of `(bin_id, n)` rows into the fitted-bin `counts`.
///
/// Absent bins stay zero. Returns `Ok(false)` when an invalid value was
/// counted, which leaves the window unscorable.
///
/// # Errors
/// Returns a description for a malformed aggregate or an out-of-range bin.
pub fn fold_psi(batch: &RecordBatch, counts: &mut [u64]) -> Result<bool, String> {
    let (ids, ns) = (int64s(batch, "bin_id")?, int64s(batch, "n")?);
    let mut scorable = true;
    for (id, n) in ids.values().iter().zip(ns.values()) {
        let n = count_of(*n)?;
        if *id == INVALID_BIN {
            scorable = false;
            continue;
        }
        let slot = usize::try_from(*id)
            .ok()
            .and_then(|index| counts.get_mut(index))
            .ok_or_else(|| format!("aggregate bin {id} is not a fitted bin"))?;
        *slot += n;
    }
    Ok(scorable)
}

/// Feed one batch of ordered `(subgroup, n, numeric_n, mean, sd)` rows of
/// `feature` to `scorer`.
///
/// A partial subgroup is fed by size alone. Returns `Ok(false)` without
/// feeding further rows when a complete subgroup counted a null or has
/// non-finite statistics, which leaves the window unscorable.
///
/// # Errors
/// Returns a description for a malformed aggregate or a subgroup the scorer
/// refuses.
pub fn fold_spc(
    batch: &RecordBatch,
    feature: &wyrd_spec::ids::FeatureName,
    scorer: &mut SpcScorer,
) -> Result<bool, String> {
    let (ns, numeric, means, sds) = (
        int64s(batch, "n")?,
        int64s(batch, "numeric_n")?,
        float64s(batch, "mean")?,
        float64s(batch, "sd")?,
    );
    let value = |column: &Float64Array, row| {
        if column.is_valid(row) {
            column.value(row)
        } else {
            f64::NAN
        }
    };
    for row in 0..batch.num_rows() {
        let n = count_of(ns.value(row))?;
        let (mean, sd) = (value(means, row), value(sds, row));
        if n == u64::from(scorer.subgroup_size())
            && (ns.value(row) != numeric.value(row) || !mean.is_finite() || !sd.is_finite())
        {
            return Ok(false);
        }
        scorer
            .push(feature, n, mean, sd)
            .map_err(|error| error.to_string())?;
    }
    Ok(true)
}

/// Read one batch of the one-row `incomplete` count into `incomplete`.
///
/// # Errors
/// Returns a description for a malformed aggregate.
pub fn fold_incomplete(batch: &RecordBatch, incomplete: &mut u64) -> Result<(), String> {
    for value in int64s(batch, "incomplete")?.values() {
        *incomplete += count_of(*value)?;
    }
    Ok(())
}

/// Read one batch of the Custom `(n, numeric_n, mean)` aggregate into `row`.
///
/// # Errors
/// Returns a description for a malformed aggregate or a second row.
pub fn fold_custom(batch: &RecordBatch, row: &mut Option<Option<f64>>) -> Result<(), String> {
    if batch.num_rows() == 0 {
        return Ok(());
    }
    if batch.num_rows() > 1 || row.is_some() {
        return Err("the Custom aggregate is not exactly one row".to_owned());
    }
    let (n, numeric, mean) = (
        int64s(batch, "n")?.value(0),
        int64s(batch, "numeric_n")?.value(0),
        float64s(batch, "mean")?,
    );
    *row = Some(
        (n > 0 && n == numeric && mean.is_valid(0) && mean.value(0).is_finite())
            .then(|| mean.value(0)),
    );
    Ok(())
}

/// Owner of Drift execution: baseline loading, SYSTEM reads, scoring.
pub struct DriftEngine {
    /// Server state owning Postgres, the token verifier, and the query service.
    state: AppState,
    /// The one tenant token issuer the SYSTEM Drift reader is minted through.
    issuer: TenantTokenIssuer,
    /// Fitted baseline reads.
    baselines: DriftBaselineQueue,
    /// Deadline of one aggregate query.
    query_timeout: Duration,
}

impl DriftEngine {
    /// Build an engine reading through `state` as readers minted by `issuer`.
    #[must_use]
    pub fn new(state: AppState, issuer: TenantTokenIssuer, query_timeout: Duration) -> Self {
        Self {
            state,
            issuer,
            baselines: DriftBaselineQueue::default(),
            query_timeout,
        }
    }

    /// Execute one claimed Drift run of `verifier` for `tenant`.
    ///
    /// Loads the fitted baseline (PSI/SPC), runs one fixed aggregate per
    /// feature as the tenant's SYSTEM Drift reader, and scores the folded
    /// aggregates. Never fails: every failure is the [`EngineOutcome`] it
    /// maps to.
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
        let reader = Reader {
            engine: self,
            tenant,
            verifier,
        };
        match spec.profile.as_ref() {
            Some(DriftProfile::Custom(profile)) => {
                let mut row = None;
                reader
                    .fold(window.custom(&profile.metric_name), |batch| {
                        fold_custom(batch, &mut row)
                    })
                    .await?;
                match row.flatten() {
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
                let names = |bin_type: BinType| {
                    baseline
                        .features
                        .iter()
                        .filter(|(_, feature)| feature.bin_type == bin_type)
                        .map(|(name, _)| name.as_str())
                        .collect::<Vec<_>>()
                };
                let incomplete = window
                    .incomplete(&names(BinType::Numeric), &names(BinType::Categorical))
                    .map_err(&invalid)?;
                if !reader.complete(incomplete).await? {
                    return Ok(None);
                }
                let mut counts = BTreeMap::new();
                for (name, feature) in &baseline.features {
                    let sql = match feature.bin_type {
                        BinType::Numeric => feature
                            .numeric_edges()
                            .map_err(|error| error.to_string())
                            .and_then(|edges| window.psi_numeric(name.as_str(), &edges))
                            .map_err(&invalid)?,
                        BinType::Categorical => {
                            let (labels, other) = feature
                                .categorical_labels()
                                .map_err(|error| invalid(error.to_string()))?;
                            window.psi_categorical(name.as_str(), &labels, other)
                        }
                    };
                    let mut feature_counts = vec![0; feature.bins.len()];
                    let mut scorable = true;
                    reader
                        .fold(sql, |batch| {
                            scorable &= fold_psi(batch, &mut feature_counts)?;
                            Ok(())
                        })
                        .await?;
                    if !scorable {
                        return Ok(None);
                    }
                    counts.insert(name.clone(), feature_counts);
                }
                scored(score_psi_counts(&baseline, &counts, profile))
            }
            Some(DriftProfile::Spc(_)) => {
                let FittedBaseline::Spc(baseline) = self.fitted(tenant, &run.verifier_uid).await?
                else {
                    return Err(invalid(
                        "the fitted baseline is not an SPC baseline".to_owned(),
                    ));
                };
                let names = baseline
                    .features
                    .keys()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>();
                let incomplete = window.incomplete(&names, &[]).map_err(&invalid)?;
                if !reader.complete(incomplete).await? {
                    return Ok(None);
                }
                let mut scorer = SpcScorer::new(&baseline);
                for name in baseline.features.keys() {
                    let sql = window
                        .spc(name.as_str(), baseline.subgroup_size)
                        .map_err(&invalid)?;
                    let mut scorable = true;
                    reader
                        .fold(sql, |batch| {
                            if scorable {
                                scorable = fold_spc(batch, name, &mut scorer)?;
                            }
                            Ok(())
                        })
                        .await?;
                    if !scorable {
                        return Ok(None);
                    }
                }
                Ok(Some(scorer.finish()))
            }
            None => Err(invalid("the Drift Verifier has no profile".to_owned())),
        }
    }

    /// Load the ready fitted baseline of `verifier_uid`.
    ///
    /// A PSI or SPC profile whose `format` is not [`FITTED_FORMAT`] was fitted
    /// under earlier semantics and is refused before decoding; it is never
    /// rescored or migrated.
    ///
    /// # Errors
    /// Retries a registry failure; terminates with [`BASELINE_NOT_READY`] when
    /// no baseline is ready, [`BASELINE_LEGACY`] for an earlier format, and
    /// [`DRIFT_INVALID`] when the stored profile does not decode.
    async fn fitted(
        &self,
        tenant: DataTenantId,
        verifier_uid: &CardUid,
    ) -> Result<FittedBaseline, EngineOutcome> {
        let unavailable =
            |error: &dyn std::fmt::Display| retry(DRIFT_QUERY_FAILED, error.to_string());
        let mut conn = self
            .state
            .postgres
            .wyrd()
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let fitted = self
            .baselines
            .fitted(&mut conn, verifier_uid)
            .await
            .map_err(|error| unavailable(&error))?
            .ok_or_else(|| terminal(BASELINE_NOT_READY, "the Drift baseline is not fitted"))?;
        let format = fitted
            .as_object()
            .and_then(|method| method.values().next())
            .and_then(|profile| profile.get("format"))
            .and_then(serde_json::Value::as_u64);
        if format != Some(u64::from(FITTED_FORMAT)) {
            return Err(terminal(
                BASELINE_LEGACY,
                "the Drift baseline was fitted before the conventional PSI/SPC semantics; \
                 register a new Verifier version to refit it",
            ));
        }
        serde_json::from_value(fitted).map_err(|error| terminal(DRIFT_INVALID, error.to_string()))
    }
}

/// The SYSTEM Drift reader of one run: its tenant and attributed Verifier.
struct Reader<'a> {
    /// Engine owning the server state and issuer.
    engine: &'a DriftEngine,
    /// Run tenant every read is minted for.
    tenant: DataTenantId,
    /// Exact Verifier the read token is attributed to.
    verifier: &'a CardRef,
}

impl Reader<'_> {
    /// Mint and verify a fresh SYSTEM Drift read token and derive its caller.
    ///
    /// Returns `Ok(None)` when the tenant has never registered the observation
    /// table, so there is nothing to read.
    ///
    /// # Errors
    /// Retries when the tenant connection, mint, or verification fails.
    async fn caller(&self) -> Result<Option<Caller>, EngineOutcome> {
        let unavailable =
            |error: &dyn std::fmt::Display| retry(DRIFT_QUERY_FAILED, error.to_string());
        let mut conn = self
            .engine
            .state
            .postgres
            .wyrd()
            .tenant_conn(self.tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let Some(token) = self
            .engine
            .issuer
            .issue_system_drift_read_token(&mut conn, self.verifier)
            .await
            .map_err(|error| unavailable(&error))?
        else {
            return Ok(None);
        };
        drop(conn);
        let verifier = self
            .engine
            .state
            .auth
            .token_verifier
            .as_deref()
            .ok_or_else(|| retry(DRIFT_QUERY_FAILED, "no token verifier is configured"))?;
        let verified = verifier
            .verify(&token.access_token, &self.tenant)
            .map_err(|error| unavailable(&error))?;
        Ok(Some(Caller {
            data_tenant_id: self.tenant,
            principal: verified.principal,
            request_id: RequestId::now_v7(),
            delegation_chain: verified.delegation_chain,
        }))
    }

    /// Run the `incomplete` count `sql` and report whether every selected
    /// observation is complete.
    ///
    /// # Errors
    /// Returns the outcomes of [`Reader::fold`].
    async fn complete(&self, sql: String) -> Result<bool, EngineOutcome> {
        let mut incomplete = 0;
        self.fold(sql, |batch| fold_incomplete(batch, &mut incomplete))
            .await?;
        Ok(incomplete == 0)
    }

    /// Run `sql` as a fresh reader and hand each decoded batch to `fold`.
    ///
    /// A tenant with no observation table reads nothing and `fold` is never
    /// called. The stream is consumed and settled by the shared scheduled
    /// consumer; a batch `fold` refuses terminates the run as invalid.
    ///
    /// # Errors
    /// Retries a mint, admission, query, or stream failure; terminates on a
    /// malformed aggregate.
    async fn fold<F>(&self, sql: String, mut fold: F) -> Result<(), EngineOutcome>
    where
        F: FnMut(&RecordBatch) -> Result<(), String>,
    {
        let Some(caller) = self.caller().await? else {
            return Ok(());
        };
        let failed = |error: WyrdError| retry(DRIFT_QUERY_FAILED, error.to_string());
        let query = ScheduledQueryCaller::authenticated(
            self.engine.state.clone(),
            caller,
            CancellationToken::new(),
        )
        .map_err(failed)?;
        let request = BifrostQueryRequest {
            sql,
            visibility: VisibilityMode::Fused,
            freshness: FreshnessPolicy::Strict,
            deadline_ms: Some(
                i64::try_from(self.engine.query_timeout.as_millis()).unwrap_or(i64::MAX),
            ),
        };
        let mut malformed = None;
        let settled = query
            .run_with(request, |batch| {
                fold(&batch).map_err(|message| {
                    malformed = Some(message);
                    WyrdError::from(BifrostError::QueryStreamProtocol)
                })
            })
            .await;
        if let Some(message) = malformed {
            return Err(terminal(DRIFT_INVALID, message));
        }
        settled.map(drop).map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    //! Proof of the fixed Drift SQL and aggregate folds.
    //!
    //! Every statement is executed by DataFusion over the real
    //! `vala.drift.observations` schema, so fitted-edge equality, category
    //! escaping, `[start, end)` exclusion, subject/series isolation, and SPC
    //! observation order are proved on the rendered SQL rather than on its
    //! text. The fold tests pin malformed-aggregate refusal and the
    //! unscorable-window outcome, and the completeness count pins which
    //! observations are selected and which make the run inconclusive.

    use std::sync::Arc;

    use arrow::array::{
        Float64Array, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray,
    };
    use arrow::datatypes::{DataType, Field, Schema};
    use chrono::{TimeZone as _, Utc};
    use datafusion::catalog::{
        CatalogProvider, MemoryCatalogProvider, MemorySchemaProvider, SchemaProvider,
    };
    use datafusion::datasource::MemTable;
    use datafusion::prelude::SessionContext;
    use vala_bifrost_redux::tables::builtin_table;

    use super::*;

    /// One observation row: subject, series, numeric and string values,
    /// event and creation seconds past the window start, and record id.
    type Row<'a> = (
        &'a str,
        &'a str,
        Option<f64>,
        Option<&'a str>,
        i64,
        i64,
        &'a str,
    );

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

    /// The window start every fixture row is offset from.
    fn start() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 17, 0, 0, 0)
            .single()
            .expect("start")
    }

    /// The subject UID every in-scope fixture row carries.
    fn subject() -> CardUid {
        CardUid::from_uuid(uuid::Uuid::from_u128(
            0x0190_0000_0000_7000_8000_0000_0000_0001,
        ))
        .expect("a v7 UUID is a Card UID")
    }

    /// A window over [`subject`] from [`start`] for one hour.
    fn window() -> ObservationWindow {
        ObservationWindow::new(
            &subject(),
            &DriftWindow {
                start: start(),
                end: start() + chrono::Duration::hours(1),
            },
        )
    }

    /// Execute `sql` over `rows` stored in the real observation columns.
    ///
    /// Columns the statements never read are null, so every field is relaxed
    /// to nullable; names and types are the table's own.
    ///
    /// # Panics
    /// Panics when the fixture cannot be registered or the SQL fails.
    async fn run(sql: &str, rows: &[Row<'_>]) -> Vec<RecordBatch> {
        let table = (builtin_table("drift", "observations")
            .expect("observations is built in")
            .schema)();
        let schema = Arc::new(Schema::new(
            table
                .fields()
                .iter()
                .map(|field| field.as_ref().clone().with_nullable(true))
                .collect::<Vec<_>>(),
        ));
        let micros = |seconds: i64| start().timestamp_micros() + seconds * 1_000_000;
        let columns: Vec<Arc<dyn Array>> = schema
            .fields()
            .iter()
            .map(|field| -> Arc<dyn Array> {
                match field.name().as_str() {
                    "card_uid" => Arc::new(StringArray::from_iter_values(rows.iter().map(|r| r.0))),
                    "series" => Arc::new(StringArray::from_iter_values(rows.iter().map(|r| r.1))),
                    "num_value" => Arc::new(Float64Array::from_iter(rows.iter().map(|r| r.2))),
                    "str_value" => Arc::new(StringArray::from_iter(rows.iter().map(|r| r.3))),
                    "wyrd_event_time" => Arc::new(
                        TimestampMicrosecondArray::from_iter_values(
                            rows.iter().map(|r| micros(r.4)),
                        )
                        .with_timezone("UTC"),
                    ),
                    "created_at" => Arc::new(
                        TimestampMicrosecondArray::from_iter_values(
                            rows.iter().map(|r| micros(r.5)),
                        )
                        .with_timezone("UTC"),
                    ),
                    "record_id" => {
                        Arc::new(StringArray::from_iter_values(rows.iter().map(|r| r.6)))
                    }
                    _ => arrow::array::new_null_array(field.data_type(), rows.len()),
                }
            })
            .collect();
        let data = RecordBatch::try_new(Arc::clone(&schema), columns).expect("fixture batch");
        let context = SessionContext::new();
        let drift = Arc::new(MemorySchemaProvider::new());
        drift
            .register_table(
                "observations".to_owned(),
                Arc::new(MemTable::try_new(schema, vec![vec![data]]).expect("mem table")),
            )
            .expect("table registers");
        let catalog = Arc::new(MemoryCatalogProvider::new());
        catalog
            .register_schema("drift", drift)
            .expect("schema registers");
        context.register_catalog("vala", catalog);
        context
            .sql(sql)
            .await
            .expect("fixed SQL plans")
            .collect()
            .await
            .expect("fixed SQL executes")
    }

    /// Execute the completeness `sql` over `rows` and fold its count.
    ///
    /// # Panics
    /// Panics when the SQL fails or its aggregate is malformed.
    async fn incomplete(sql: &str, rows: &[Row<'_>]) -> u64 {
        let mut incomplete = 0;
        for batch in run(sql, rows).await {
            fold_incomplete(&batch, &mut incomplete).expect("well-formed");
        }
        incomplete
    }

    /// Numeric PSI counts land on the fitted `(lower, upper]` edges exactly,
    /// and only this subject's series inside `[start, end)` is counted.
    #[tokio::test]
    async fn psi_numeric_sql_bins_on_fitted_edges_inside_the_window() {
        let uid = subject().to_string();
        let other = "0190aaaa-0000-7000-8000-000000000009";
        let rows: Vec<Row<'_>> = vec![
            (&uid, "age", Some(10.0), None, 0, 0, "a"),
            (&uid, "age", Some(10.5), None, 1, 1, "b"),
            (&uid, "age", Some(20.0), None, 2, 2, "c"),
            (&uid, "age", Some(99.0), None, 3, 3, "d"),
            (&uid, "age", Some(1.0), None, 3600, 4, "end is exclusive"),
            (&uid, "age", Some(1.0), None, -1, 5, "before start"),
            (&uid, "height", Some(1.0), None, 4, 6, "other series"),
            (other, "age", Some(1.0), None, 4, 7, "other subject"),
        ];
        let sql = window()
            .psi_numeric("age", &[f64::NEG_INFINITY, 10.0, 20.0, f64::INFINITY])
            .expect("numeric SQL");
        let mut counts = vec![0; 3];
        for batch in run(&sql, &rows).await {
            assert!(fold_psi(&batch, &mut counts).expect("well-formed"));
        }
        assert_eq!(counts, vec![1, 2, 1]);

        for bad in [
            None,
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(f64::NEG_INFINITY),
        ] {
            let invalid: Vec<Row<'_>> = vec![(&uid, "age", bad, None, 0, 0, "a")];
            let mut counts = vec![0; 3];
            let scorable = run(&sql, &invalid)
                .await
                .iter()
                .all(|batch| fold_psi(batch, &mut counts).expect("well-formed"));
            assert!(!scorable, "{bad:?} leaves the window unscorable");
        }
        assert!(window().psi_numeric("age", &[0.0]).is_err());
        assert!(
            window()
                .psi_numeric("age", &[f64::NEG_INFINITY, f64::NAN, f64::INFINITY])
                .is_err()
        );
    }

    /// Categorical PSI counts fitted labels, sends unseen categories to the
    /// `other` bin, marks a null category invalid, and escapes quoted labels
    /// and series.
    #[tokio::test]
    async fn psi_categorical_sql_escapes_labels_and_counts_unknowns_as_other() {
        let uid = subject().to_string();
        let rows: Vec<Row<'_>> = vec![
            (&uid, "it's", None, Some("o'neil"), 0, 0, "a"),
            (&uid, "it's", None, Some("red"), 1, 1, "b"),
            (&uid, "it's", None, Some("red"), 2, 2, "c"),
            (&uid, "it's", None, Some("mauve"), 3, 3, "d"),
            (&uid, "it's", None, Some("teal"), 4, 4, "e"),
        ];
        let sql = window().psi_categorical("it's", &["red", "o'neil", "blue"], 3);
        let mut counts = vec![0; 4];
        for batch in run(&sql, &rows).await {
            assert!(fold_psi(&batch, &mut counts).expect("well-formed"));
        }
        assert_eq!(counts, vec![2, 1, 0, 2]);

        let null: Vec<Row<'_>> = vec![(&uid, "it's", None, None, 0, 0, "a")];
        let mut counts = vec![0; 4];
        let scorable = run(&sql, &null)
            .await
            .iter()
            .all(|batch| fold_psi(batch, &mut counts).expect("well-formed"));
        assert!(!scorable, "a null category leaves the window unscorable");
        let stray = batch(&[("bin_id", vec![9]), ("n", vec![1])], &[]);
        assert!(fold_psi(&stray, &mut counts).is_err());
    }

    /// SPC subgroups follow `created_at`, then `record_id`, return the mean
    /// and sample standard deviation, keep a partial trailing subgroup
    /// visible, and report nulls through `n != numeric_n`.
    #[tokio::test]
    async fn spc_sql_orders_subgroups_by_creation_then_record() {
        let uid = subject().to_string();
        let rows: Vec<Row<'_>> = vec![
            (&uid, "age", Some(4.0), None, 0, 2, "a"),
            (&uid, "age", Some(1.0), None, 1, 0, "b"),
            (&uid, "age", Some(3.0), None, 2, 1, "b"),
            (&uid, "age", Some(2.0), None, 3, 1, "a"),
            (&uid, "age", None, None, 4, 3, "a"),
        ];
        let sql = window().spc("age", 2).expect("spc SQL");
        let batches = run(&sql, &rows).await;
        let subgroups = batches
            .iter()
            .flat_map(|batch| {
                let (ns, numeric, means, sds) = (
                    int64s(batch, "n").expect("n"),
                    int64s(batch, "numeric_n").expect("numeric_n"),
                    float64s(batch, "mean").expect("mean"),
                    float64s(batch, "sd").expect("sd"),
                );
                (0..batch.num_rows())
                    .map(|row| {
                        (
                            ns.value(row),
                            numeric.value(row),
                            means.is_valid(row).then(|| means.value(row)),
                            sds.is_valid(row).then(|| sds.value(row)),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let half = std::f64::consts::FRAC_1_SQRT_2;
        assert_eq!(
            subgroups,
            vec![
                (2, 2, Some(1.5), Some(half)),
                (2, 2, Some(3.5), Some(half)),
                (1, 0, None, None)
            ]
        );
        assert!(window().spc("age", 1).is_err());
    }

    /// The SPC fold feeds complete subgroups and a partial trailing one to
    /// the scorer, and stops on a null or non-finite complete subgroup.
    ///
    /// # Panics
    /// Panics when a fold outcome differs.
    #[test]
    fn spc_fold_feeds_subgroups_and_refuses_invalid_ones() {
        let feature = wyrd_spec::ids::FeatureName::new("age").expect("feature");
        let values = (0..40).map(|row| Some(f64::from(row % 3))).collect();
        let baseline = vala_drift::fit_spc_baseline(
            &batch(&[], &[("age", values)]),
            &wyrd_spec::card::drift::SpcProfile { sample_size: 2 },
            std::slice::from_ref(&feature),
        )
        .expect("baseline fits");
        let aggregate = |n: Vec<i64>, numeric: Vec<i64>, mean: Vec<Option<f64>>, sd| {
            batch(
                &[("n", n), ("numeric_n", numeric)],
                &[("mean", mean), ("sd", sd)],
            )
        };
        let mut scorer = SpcScorer::new(&baseline);
        let fed = aggregate(
            vec![2, 2, 1],
            vec![2, 2, 1],
            vec![Some(0.0), Some(5.0), Some(0.0)],
            vec![Some(0.5), Some(0.5), None],
        );
        assert!(fold_spc(&fed, &feature, &mut scorer).expect("well-formed"));
        assert_eq!(
            scorer.finish().verdict,
            vala_drift::DriftVerdict::Inconclusive,
            "a partial trailing subgroup is inconclusive"
        );
        for (numeric, mean) in [(1, Some(0.0)), (2, Some(f64::NAN)), (2, None)] {
            let mut scorer = SpcScorer::new(&baseline);
            let invalid = aggregate(vec![2], vec![numeric], vec![mean], vec![Some(0.5)]);
            assert!(!fold_spc(&invalid, &feature, &mut scorer).expect("well-formed"));
        }
    }

    /// Completeness selects records carrying a configured series in the
    /// window, ignores unrelated records, and counts one incomplete for an
    /// omitted feature, a null, NaN, or infinite numeric value, or a null
    /// category.
    #[tokio::test]
    async fn completeness_flags_omitted_and_invalid_features_only() {
        let uid = subject().to_string();
        let uid = uid.as_str();
        let sql = window()
            .incomplete(&["x"], &["c"])
            .expect("completeness SQL");
        let complete = vec![
            (uid, "x", Some(1.0), Some("1"), 0, 0, "r1"),
            (uid, "c", None, Some("a"), 0, 0, "r1"),
            (uid, "unrelated", None, None, 0, 0, "r2"),
            (uid, "x", None, None, 7200, 0, "outside the window"),
        ];
        assert_eq!(incomplete(&sql, &complete).await, 0);
        assert_eq!(
            incomplete(&sql, &[]).await,
            0,
            "an empty window selects nothing"
        );
        let omitted = vec![(uid, "x", Some(1.0), Some("1"), 0, 0, "r1")];
        assert_eq!(incomplete(&sql, &omitted).await, 1);
        for bad in [
            None,
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(f64::NEG_INFINITY),
        ] {
            let rows = vec![
                (uid, "x", bad, None, 0, 0, "r1"),
                (uid, "c", None, Some("a"), 0, 0, "r1"),
            ];
            assert_eq!(incomplete(&sql, &rows).await, 1, "{bad:?}");
        }
        let null_category = vec![
            (uid, "x", Some(1.0), Some("1"), 0, 0, "r1"),
            (uid, "c", None, None, 0, 0, "r1"),
        ];
        assert_eq!(incomplete(&sql, &null_category).await, 1);
        assert!(window().incomplete(&[], &[]).is_err());
    }

    /// The Custom aggregate is one row over the window; only a non-empty,
    /// all-numeric, finite window is scorable, and a second row is refused.
    #[tokio::test]
    async fn custom_sql_is_one_row_and_requires_a_complete_window() {
        let uid = subject().to_string();
        let rows: Vec<Row<'_>> = vec![
            (&uid, "latency", Some(2.0), None, 0, 0, "a"),
            (&uid, "latency", Some(3.0), None, 1, 1, "b"),
        ];
        let sql = window().custom("latency");
        let mut row = None;
        for batch in run(&sql, &rows).await {
            fold_custom(&batch, &mut row).expect("well-formed");
        }
        assert_eq!(row, Some(Some(2.5)));

        let mut empty = None;
        for batch in run(&sql, &[]).await {
            fold_custom(&batch, &mut empty).expect("well-formed");
        }
        assert_eq!(empty, Some(None), "an empty window is one unscorable row");

        let aggregate = |n, numeric, mean| {
            batch(
                &[("n", vec![n]), ("numeric_n", vec![numeric])],
                &[("mean", vec![mean])],
            )
        };
        let mut partial = None;
        fold_custom(&aggregate(3, 2, Some(2.5)), &mut partial).expect("well-formed");
        assert_eq!(partial, Some(None));
        let mut infinite = None;
        fold_custom(&aggregate(3, 3, Some(f64::INFINITY)), &mut infinite).expect("well-formed");
        assert_eq!(infinite, Some(None));
        assert!(fold_custom(&aggregate(3, 3, Some(1.0)), &mut row).is_err());
        let malformed = batch(&[("n", vec![1])], &[]);
        assert!(fold_custom(&malformed, &mut None).is_err());
    }
}
