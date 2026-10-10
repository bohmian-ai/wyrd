//! Per-query execution profile: the Oracle's analog of `ClickHouse`
//! `ProfileEvents`.
//!
//! One [`QueryProfile`] follows one SQL attempt from its catalog pin to its
//! stream's terminal. The Oracle's own steps are timed into `DataFusion`
//! [`Time`] metrics registered on an [`ExecutionPlanMetricsSet`], with the
//! same scoped-guard pattern every `DataFusion` operator uses, and the executed
//! physical root is retained so every operator's own metrics are folded in at
//! the end. The profile is emitted exactly once, when its last owner drops:
//! global counters for fleet-wide Pareto analysis and one DEBUG event for the
//! single query.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};

use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::metrics::{
    ExecutionPlanMetricsSet, MetricBuilder, MetricValue, Time,
};

/// Prefix naming the Oracle's own steps among the operator events.
const ORACLE_EVENT: &str = "Oracle";

/// Accumulated time and counts for one query, emitted when the last owner drops.
///
/// Shared by the attempt's planned cut and its returned stream. The stream is
/// the last owner on every successful path, and drops it only after every
/// descendant of the query has settled, so the emitted profile includes every
/// operator's final metrics.
///
/// Times are `DataFusion` nanosecond totals summed across partitions, so an
/// operator's time may exceed the query's wall time when partitions overlap.
/// Only the leader's operators are folded; work a distributed stage ran on a
/// follower is reported only as far as the distributed metrics rewrite
/// returns it to the leader's plan.
#[derive(Debug)]
pub(crate) struct QueryProfile {
    /// Owner of every Oracle step metric below, read back once on drop.
    metrics: ExecutionPlanMetricsSet,
    /// The read-only tenant-scoped statement that returns every pointer.
    pub(crate) cut_read: Time,
    /// Reading every acquired table's metadata and manifests.
    pub(crate) cut_materialize: Time,
    /// Listing live Scribe streams on the frozen roster.
    pub(crate) scribe_listing: Time,
    /// Registering the pinned cut's table providers.
    pub(crate) provider_setup: Time,
    /// Planning the one physical root from SQL.
    pub(crate) physical_planning: Time,
    /// Acquiring query admission, queueing included.
    pub(crate) admission: Time,
    /// Staging the read decision and binding sources to the retained root.
    pub(crate) audit_bind: Time,
    /// The executed root, retained so its operators' metrics fold in on drop.
    root: OnceLock<Arc<dyn ExecutionPlan>>,
}

impl QueryProfile {
    /// Registers every Oracle step metric on a fresh metrics set.
    pub(crate) fn new() -> Self {
        let metrics = ExecutionPlanMetricsSet::new();
        let step = |name: &'static str| {
            let time = Time::new();
            MetricBuilder::new(&metrics).build(MetricValue::Time {
                name: name.into(),
                time: time.clone(),
            });
            time
        };
        Self {
            cut_read: step("cut_read"),
            cut_materialize: step("cut_materialize"),
            scribe_listing: step("scribe_listing"),
            provider_setup: step("provider_setup"),
            physical_planning: step("physical_planning"),
            admission: step("admission"),
            audit_bind: step("audit_bind"),
            root: OnceLock::new(),
            metrics,
        }
    }

    /// Retains the root this query executes so its metrics fold in on drop.
    ///
    /// The root shares its metric handles with the running streams, so the
    /// values read on drop are final. Only the first root is kept; an attempt
    /// executes exactly one.
    pub(crate) fn retain_root(&self, root: &Arc<dyn ExecutionPlan>) {
        // An attempt executes one root; a second call has nothing to add.
        let _ = self.root.set(Arc::clone(root));
    }

    /// Collects every event: the Oracle's steps, then each operator's metrics.
    ///
    /// Events are keyed `<operator>.<metric>`, summed across partitions and
    /// across operators of the same name. Only time and count metrics are
    /// kept; gauges, timestamps, ratios, and pruning summaries do not sum
    /// meaningfully across queries.
    fn events(&self) -> BTreeMap<String, ProfileValue> {
        let mut events = BTreeMap::new();
        ProfileValue::fold(ORACLE_EVENT, &self.metrics.clone_inner(), &mut events);
        if let Some(root) = self.root.get() {
            Self::fold_plan(root.as_ref(), &mut events);
        }
        events
    }

    /// Folds one operator's metrics and then its children's.
    fn fold_plan(plan: &dyn ExecutionPlan, events: &mut BTreeMap<String, ProfileValue>) {
        if let Some(metrics) = plan.metrics() {
            ProfileValue::fold(plan.name(), &metrics.aggregate_by_name(), events);
        }
        for child in plan.children() {
            Self::fold_plan(child.as_ref(), events);
        }
    }
}

impl Drop for QueryProfile {
    /// Emits the profile once: global counters and one DEBUG event.
    ///
    /// Counters are `oracle_query_profile_nanoseconds_total{event}` for times
    /// and `oracle_query_profile_count_total{event}` for counts, beside
    /// `oracle_query_profiles_total`, so a scrape delta divided by the profile
    /// count is the mean per query.
    fn drop(&mut self) {
        let events = self.events();
        let mut rendered = String::new();
        for (event, value) in &events {
            let (family, amount, unit) = match *value {
                ProfileValue::Nanoseconds(amount) => {
                    ("oracle_query_profile_nanoseconds_total", amount, "ns")
                }
                ProfileValue::Count(amount) => ("oracle_query_profile_count_total", amount, ""),
            };
            metrics::counter!(family, "event" => event.clone()).increment(amount);
            let _ = write!(rendered, "{event}={amount}{unit} ");
        }
        metrics::counter!("oracle_query_profiles_total").increment(1);
        tracing::debug!(profile = rendered.trim_end(), "Oracle query profile");
    }
}

/// One profile event's accumulated value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileValue {
    /// Summed `DataFusion` time, in nanoseconds.
    Nanoseconds(u64),
    /// Summed `DataFusion` count.
    Count(u64),
}

impl ProfileValue {
    /// Classifies one `DataFusion` metric, keeping only additive times and counts.
    fn of(value: &MetricValue) -> Option<Self> {
        let amount = u64::try_from(value.as_usize()).unwrap_or(u64::MAX);
        match value {
            MetricValue::ElapsedCompute(_) | MetricValue::Time { .. } => {
                Some(Self::Nanoseconds(amount))
            }
            MetricValue::OutputRows(_)
            | MetricValue::OutputBytes(_)
            | MetricValue::OutputBatches(_)
            | MetricValue::SpillCount(_)
            | MetricValue::SpilledBytes(_)
            | MetricValue::SpilledRows(_)
            | MetricValue::Count { .. } => Some(Self::Count(amount)),
            _ => None,
        }
    }

    /// Adds every additive metric of one operator under `<operator>.<metric>`.
    fn fold(
        operator: &str,
        metrics: &datafusion::physical_plan::metrics::MetricsSet,
        events: &mut BTreeMap<String, Self>,
    ) {
        for metric in metrics.iter() {
            let value = metric.value();
            let Some(addition) = Self::of(value) else {
                continue;
            };
            events
                .entry(format!("{operator}.{}", value.name()))
                .and_modify(|current| *current = current.plus(addition))
                .or_insert(addition);
        }
    }

    /// Sums two values of the same kind, saturating; a kind clash keeps `self`.
    const fn plus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Nanoseconds(left), Self::Nanoseconds(right)) => {
                Self::Nanoseconds(left.saturating_add(right))
            }
            (Self::Count(left), Self::Count(right)) => Self::Count(left.saturating_add(right)),
            (kept, _) => kept,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::execution::TaskContext;
    use datafusion::physical_expr::expressions::lit;
    use datafusion::physical_plan::empty::EmptyExec;
    use datafusion::physical_plan::filter::FilterExec;
    use datafusion::physical_plan::{ExecutionPlan, collect};

    use super::{ProfileValue, QueryProfile};

    /// A timed Oracle step and the executed root's operators both appear,
    /// keyed by operator and metric name.
    #[tokio::test]
    async fn profile_folds_oracle_steps_and_operator_metrics() {
        let profile = QueryProfile::new();
        profile
            .cut_read
            .add_duration(std::time::Duration::from_micros(5));
        let empty = Arc::new(EmptyExec::new(Arc::new(Schema::new(vec![Field::new(
            "id",
            DataType::Int64,
            false,
        )]))));
        let root: Arc<dyn ExecutionPlan> =
            Arc::new(FilterExec::try_new(lit(true), empty).expect("filter over an empty input"));
        profile.retain_root(&root);
        collect(Arc::clone(&root), Arc::new(TaskContext::default()))
            .await
            .expect("execute the empty root");

        let events = profile.events();

        assert_eq!(
            events.get("Oracle.cut_read"),
            Some(&ProfileValue::Nanoseconds(5_000))
        );
        assert_eq!(
            events.get("FilterExec.output_rows"),
            Some(&ProfileValue::Count(0))
        );
    }
}
