//! Native physical-plan exchange splitting for Oracle follower execution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::error::DataFusionError;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::aggregates::{AggregateExec, AggregateMode};
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
use datafusion::physical_plan::limit::GlobalLimitExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;

use super::codec::RemoteScanExec;
use super::sealed_fragment_schema_fingerprint;

/// One native follower subtree and the leader placeholder that receives its Arrow output.
pub(super) struct FollowerSubtree {
    /// Stable leader-side result identity.
    pub(super) result_scan_id: String,
    /// Native child retained below the selected exchange boundary.
    pub(super) plan: Arc<dyn ExecutionPlan>,
    /// Exact authenticated provider identities referenced by this subtree.
    pub(super) source_scan_ids: Vec<String>,
}

/// Leader plan plus every disjoint native follower subtree.
pub(super) struct SplitPhysicalPlan {
    /// Native leader plan retaining final/global operators.
    pub(super) leader: Arc<dyn ExecutionPlan>,
    /// Disjoint follower children retaining local/partial operators.
    pub(super) followers: Vec<FollowerSubtree>,
}

/// Splits at native repartition/coalesce/merge exchanges and fails closed on unsupported shapes.
///
/// # Errors
/// Returns a plan error for joins, windows, unknown Wyrd extensions, or a plan with no remote scan.
pub(super) fn split_physical_plan(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
) -> Result<SplitPhysicalPlan, DataFusionError> {
    validate_supported(plan.as_ref())?;
    let mut followers = Vec::new();
    let leader = split_node(plan, source_groups, &mut followers)?;
    if followers.is_empty() {
        let source_scan_ids = collect_source_scan_ids(leader.as_ref());
        if source_scan_ids.is_empty() {
            return Err(DataFusionError::Plan(
                "distributed Oracle plan contains no authenticated remote source".to_owned(),
            ));
        }
        if contains_leader_only_operator(leader.as_ref()) {
            return Err(DataFusionError::Plan(
                "distributed Oracle plan has no safe follower boundary below its leader operators"
                    .to_owned(),
            ));
        }
        let result_scan_id = "oracle-result-0".to_owned();
        let schema = leader.schema();
        followers.push(FollowerSubtree {
            result_scan_id: result_scan_id.clone(),
            plan: leader,
            source_scan_ids,
        });
        return Ok(SplitPhysicalPlan {
            leader: Arc::new(RemoteScanExec::new(
                result_scan_id,
                sealed_fragment_schema_fingerprint(schema.as_ref()),
                schema,
            )),
            followers,
        });
    }
    Ok(SplitPhysicalPlan { leader, followers })
}

/// Recursively replaces exchange children that contain authenticated source placeholders.
fn split_node(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
    followers: &mut Vec<FollowerSubtree>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    let children = plan.children();
    if children.is_empty() {
        return Ok(plan);
    }
    let at_exchange = plan.as_any().is::<RepartitionExec>()
        || plan.as_any().is::<CoalescePartitionsExec>()
        || plan.as_any().is::<SortPreservingMergeExec>();
    let groups = collect_source_scan_ids(plan.as_ref())
        .into_iter()
        .filter_map(|scan_id| source_groups.get(&scan_id))
        .collect::<HashSet<_>>();
    let at_role_boundary = plan.name() == "UnionExec" && groups.len() > 1;
    let mut replacements = Vec::with_capacity(children.len());
    for child in children {
        let source_scan_ids = collect_source_scan_ids(child.as_ref());
        let child_groups = source_scan_ids
            .iter()
            .filter_map(|scan_id| source_groups.get(scan_id))
            .collect::<HashSet<_>>();
        if ((at_exchange && child_groups.len() <= 1) || at_role_boundary)
            && !source_scan_ids.is_empty()
            && !contains_leader_only_operator(child.as_ref())
        {
            let result_scan_id = format!("oracle-result-{}", followers.len());
            let schema = child.schema();
            followers.push(FollowerSubtree {
                result_scan_id: result_scan_id.clone(),
                plan: Arc::clone(child),
                source_scan_ids,
            });
            replacements.push(Arc::new(RemoteScanExec::new(
                result_scan_id,
                sealed_fragment_schema_fingerprint(schema.as_ref()),
                schema,
            )) as Arc<dyn ExecutionPlan>);
        } else {
            replacements.push(split_node(Arc::clone(child), source_groups, followers)?);
        }
    }
    plan.with_new_children(replacements)
}

/// Reports whether a subtree retains semantics that must execute once on the leader.
fn contains_leader_only_operator(plan: &dyn ExecutionPlan) -> bool {
    if plan.as_any().is::<GlobalLimitExec>()
        || plan
            .as_any()
            .downcast_ref::<SortExec>()
            .is_some_and(|sort| !sort.preserve_partitioning())
        || plan.as_any().is::<SortPreservingMergeExec>()
        || plan
            .as_any()
            .downcast_ref::<AggregateExec>()
            .is_some_and(|aggregate| {
                !matches!(
                    aggregate.mode(),
                    AggregateMode::Partial | AggregateMode::PartialReduce
                )
            })
    {
        return true;
    }
    plan.children()
        .into_iter()
        .any(|child| contains_leader_only_operator(child.as_ref()))
}

/// Collects unique source placeholders without including leader result placeholders.
fn collect_source_scan_ids(plan: &dyn ExecutionPlan) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut scans = Vec::new();
    collect_scans(plan, &mut seen, &mut scans);
    scans
}

/// Depth-first placeholder collection preserves deterministic physical-plan order.
fn collect_scans(plan: &dyn ExecutionPlan, seen: &mut HashSet<String>, scans: &mut Vec<String>) {
    if let Some(scan) = plan.as_any().downcast_ref::<RemoteScanExec>()
        && !scan.scan_id().starts_with("oracle-result-")
        && seen.insert(scan.scan_id().to_owned())
    {
        scans.push(scan.scan_id().to_owned());
    }
    for child in plan.children() {
        collect_scans(child.as_ref(), seen, scans);
    }
}

/// Rejects operators whose distributed semantics are not approved for this splitter.
fn validate_supported(plan: &dyn ExecutionPlan) -> Result<(), DataFusionError> {
    let name = plan.name();
    if name.contains("Join") || name.contains("Window") {
        return Err(DataFusionError::Plan(format!(
            "unsupported distributed Oracle operator: {name}"
        )));
    }
    if name.ends_with("Exec")
        && plan.children().is_empty()
        && !matches!(
            name,
            "RemoteScanExec" | "EmptyExec" | "DataSourceExec" | "MemorySourceConfig"
        )
    {
        return Err(DataFusionError::Plan(format!(
            "unsupported distributed Oracle leaf: {name}"
        )));
    }
    for child in plan.children() {
        validate_supported(child.as_ref())?;
    }
    Ok(())
}

/// Replaces every leader result placeholder with the matching footer-validated Arrow batches.
///
/// # Errors
/// Returns a plan error for a missing/duplicate result or incompatible Arrow schema.
pub(super) fn substitute_remote_results(
    plan: Arc<dyn ExecutionPlan>,
    mut results: HashMap<String, Vec<RecordBatch>>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    let leader = substitute_node(plan, &mut results)?;
    if !results.is_empty() {
        return Err(DataFusionError::Plan(
            "distributed Oracle returned an unreferenced result".to_owned(),
        ));
    }
    Ok(leader)
}

/// Recursively consumes one exact remote result per leader placeholder.
fn substitute_node(
    plan: Arc<dyn ExecutionPlan>,
    results: &mut HashMap<String, Vec<RecordBatch>>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    if let Some(scan) = plan.as_any().downcast_ref::<RemoteScanExec>()
        && scan.scan_id().starts_with("oracle-result-")
    {
        let mut batches = results.remove(scan.scan_id()).ok_or_else(|| {
            DataFusionError::Plan(format!("missing remote result {}", scan.scan_id()))
        })?;
        if batches.is_empty() {
            batches.push(RecordBatch::new_empty(plan.schema()));
        }
        return Ok(MemorySourceConfig::try_new_exec(
            &[batches],
            plan.schema(),
            None,
        )?);
    }
    let children = plan.children();
    if children.is_empty() {
        return Ok(plan);
    }
    let replacements = children
        .into_iter()
        .map(|child| substitute_node(Arc::clone(child), results))
        .collect::<Result<Vec<_>, _>>()?;
    plan.with_new_children(replacements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{ArrayRef, Int64Array, StringArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
    use arrow::record_batch::RecordBatch;
    use async_trait::async_trait;
    use datafusion::catalog::Session;
    use datafusion::datasource::{MemTable, TableProvider, TableType};
    use datafusion::execution::context::SessionContext;
    use datafusion::logical_expr::Expr;

    /// Table provider whose physical leaf is one authenticated remote placeholder.
    #[derive(Debug)]
    struct RemoteTable {
        /// Exact common schema exposed to logical and physical planning.
        schema: SchemaRef,
    }

    #[async_trait]
    impl TableProvider for RemoteTable {
        /// Exposes the concrete fixture for DataFusion downcasts.
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        /// Returns the immutable fixture schema.
        fn schema(&self) -> SchemaRef {
            Arc::clone(&self.schema)
        }

        /// Models one ordinary base table.
        fn table_type(&self) -> TableType {
            TableType::Base
        }

        /// Produces the authenticated placeholder consumed by the splitter.
        async fn scan(
            &self,
            _state: &dyn Session,
            _projection: Option<&Vec<usize>>,
            _filters: &[Expr],
            _limit: Option<usize>,
        ) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
            Ok(Arc::new(RemoteScanExec::new(
                "scan".to_owned(),
                sealed_fragment_schema_fingerprint(self.schema.as_ref()),
                Arc::clone(&self.schema),
            )))
        }
    }

    /// Builds one partial aggregate batch for the actual follower schema.
    fn aggregate_partial(schema: SchemaRef, count: u64) -> RecordBatch {
        let columns = schema
            .fields()
            .iter()
            .map(|field| match field.data_type() {
                DataType::Utf8 => Arc::new(StringArray::from(vec!["oracle"])) as ArrayRef,
                DataType::Int64 => Arc::new(Int64Array::from(vec![
                    i64::try_from(count).expect("fixture count fits i64"),
                ])) as ArrayRef,
                DataType::UInt64 => Arc::new(UInt64Array::from(vec![count])) as ArrayRef,
                other => panic!("unexpected partial aggregate type {other:?}"),
            })
            .collect();
        RecordBatch::try_new(schema, columns).expect("partial aggregate matches follower schema")
    }

    /// Distributed splitting rejects joins before assigning any follower work.
    #[tokio::test]
    async fn distributed_split_rejects_unsafe_join_plan() {
        let session = SessionContext::new();
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::UInt64, false)]));
        for table in ["left_rows", "right_rows"] {
            session
                .register_table(
                    table,
                    Arc::new(
                        MemTable::try_new(Arc::clone(&schema), vec![Vec::new()])
                            .expect("schema-only join input is valid"),
                    ),
                )
                .expect("test join table registers");
        }
        let plan = session
            .sql("SELECT left_rows.id FROM left_rows JOIN right_rows USING (id)")
            .await
            .expect("join query plans")
            .create_physical_plan()
            .await
            .expect("join physical plan is constructible");
        let error = match split_physical_plan(plan, &HashMap::new()) {
            Ok(_) => panic!("distributed joins remain outside the approved split contract"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("unsupported distributed Oracle operator")
        );
    }

    /// Global aggregation, ordering, and limiting execute once after all remote partials.
    #[tokio::test]
    async fn distributed_split_retains_global_operators_on_leader() {
        let session = SessionContext::new();
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Utf8,
            false,
        )]));
        session
            .register_table("events", Arc::new(RemoteTable { schema }))
            .expect("remote fixture registers");
        let plan = session
            .sql(
                "SELECT value, COUNT(*) AS total FROM events \
                 GROUP BY value ORDER BY total DESC LIMIT 1",
            )
            .await
            .expect("aggregate query plans")
            .create_physical_plan()
            .await
            .expect("aggregate physical plan is constructible");
        let groups = HashMap::from([("scan".to_owned(), "events".to_owned())]);
        let split = split_physical_plan(plan, &groups).expect("safe lower boundary exists");
        assert_eq!(split.followers.len(), 1);
        let leader_shape = datafusion::physical_plan::displayable(split.leader.as_ref())
            .indent(true)
            .to_string();
        let follower_shape =
            datafusion::physical_plan::displayable(split.followers[0].plan.as_ref())
                .indent(true)
                .to_string();
        assert!(
            leader_shape.contains("fetch=1"),
            "leader plan:\n{leader_shape}"
        );
        assert!(
            leader_shape.contains("SortPreservingMergeExec"),
            "leader plan:\n{leader_shape}"
        );
        assert!(
            leader_shape.contains("AggregateExec: mode=FinalPartitioned"),
            "leader plan:\n{leader_shape}"
        );
        assert!(!follower_shape.contains("GlobalLimitExec"));
        assert!(!follower_shape.contains("SortPreservingMergeExec"));
        assert!(follower_shape.contains("AggregateExec: mode=Partial"));

        let follower = &split.followers[0];
        let partials = vec![
            aggregate_partial(follower.plan.schema(), 3),
            aggregate_partial(follower.plan.schema(), 2),
        ];
        let leader = substitute_remote_results(
            split.leader,
            HashMap::from([(follower.result_scan_id.clone(), partials)]),
        )
        .expect("footer-validated partials substitute once");
        let batches = datafusion::physical_plan::collect(leader, session.task_ctx())
            .await
            .expect("leader final plan executes");
        let total = batches
            .iter()
            .find_map(|batch| batch.column_by_name("total"))
            .and_then(|column| {
                column
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .map(|values| values.value(0))
                    .or_else(|| {
                        column
                            .as_any()
                            .downcast_ref::<Int64Array>()
                            .map(|values| u64::try_from(values.value(0)).expect("positive count"))
                    })
            });
        assert_eq!(total, Some(5));
    }

    /// A final operator without a lower exchange is never replayed as a follower plan.
    #[test]
    fn distributed_split_rejects_final_plan_without_safe_boundary() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Utf8,
            false,
        )]));
        let remote = Arc::new(RemoteScanExec::new(
            "scan".to_owned(),
            sealed_fragment_schema_fingerprint(schema.as_ref()),
            schema,
        ));
        let plan = Arc::new(GlobalLimitExec::new(remote, 0, Some(1)));
        let groups = HashMap::from([("scan".to_owned(), "events".to_owned())]);
        let error = match split_physical_plan(plan, &groups) {
            Ok(_) => panic!("a leader-only plan must not use whole-plan follower fallback"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("no safe follower boundary"));
    }
}
