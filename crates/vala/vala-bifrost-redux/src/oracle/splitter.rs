//! Native physical-plan exchange splitting for Oracle follower execution.

use datafusion::physical_plan::execution_plan::{ChildrenPropertiesMode, ReplaceChildrenOptions};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[cfg(test)]
use arrow::record_batch::RecordBatch;
use datafusion::config::ConfigOptions;
#[cfg(test)]
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::error::DataFusionError;
use datafusion::physical_expr::{PhysicalExpr, expressions::Column as PhysicalColumn};
use datafusion::physical_optimizer::PhysicalOptimizerRule;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::ExecutionPlanProperties;
use datafusion::physical_plan::aggregates::{AggregateExec, AggregateMode, PhysicalGroupBy};
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
#[cfg(test)]
use datafusion::physical_plan::limit::GlobalLimitExec;
use datafusion::physical_plan::repartition::RepartitionExec;
use datafusion::physical_plan::sorts::sort::SortExec;
use datafusion::physical_plan::sorts::sort_preserving_merge::SortPreservingMergeExec;

use super::assignment_schema_fingerprint;
use super::codec::RemoteSourcePlaceholderExec;

/// One native follower subtree and the leader placeholder that receives its Arrow output.
#[derive(Debug)]
pub(super) struct FollowerSubtree {
    /// Stable leader-side result identity.
    pub(super) result_scan_id: String,
    /// Native child retained below the selected exchange boundary.
    pub(super) plan: Arc<dyn ExecutionPlan>,
    /// Exact authenticated provider identities referenced by this subtree.
    pub(super) source_scan_ids: Vec<String>,
}

/// Leader plan plus every disjoint native follower subtree.
#[derive(Debug)]
pub(super) struct SplitPhysicalPlan {
    /// Native leader plan retaining final/global operators.
    pub(super) leader: Arc<dyn ExecutionPlan>,
    /// Disjoint follower children retaining local/partial operators.
    pub(super) followers: Vec<FollowerSubtree>,
}

/// Request-bound physical optimizer that owns the one distributed-plan rewrite.
#[derive(Debug)]
pub(super) struct RemoteScanRule {
    /// Immutable source-to-table identities captured before physical planning.
    source_groups: Arc<HashMap<String, String>>,
    /// Exact authenticated participant count used by the single-node predicate.
    selected_participants: usize,
    /// One-shot follower subtrees captured by the optimizer for network specialization.
    followers: std::sync::Mutex<Option<Vec<FollowerSubtree>>>,
}

impl RemoteScanRule {
    /// Creates one request-scoped rule over immutable assignment and membership inputs.
    pub(super) fn new(
        source_groups: HashMap<String, String>,
        selected_participants: usize,
    ) -> Self {
        Self {
            source_groups: Arc::new(source_groups),
            selected_participants,
            followers: std::sync::Mutex::new(None),
        }
    }

    /// Consumes the exact follower subtrees produced during physical optimization.
    pub(super) fn take_followers(&self) -> Result<Vec<FollowerSubtree>, DataFusionError> {
        self.followers
            .lock()
            .map_err(|_| {
                DataFusionError::Internal("remote scan rule state is poisoned".to_owned())
            })?
            .take()
            .ok_or_else(|| DataFusionError::Internal("remote scan rule did not run".to_owned()))
    }
}

impl PhysicalOptimizerRule for RemoteScanRule {
    /// Applies the request-bound distributed rewrite exactly once.
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
        if self
            .followers
            .lock()
            .map_err(|_| {
                DataFusionError::Internal("remote scan rule state is poisoned".to_owned())
            })?
            .is_some()
        {
            return Ok(plan);
        }
        let split = split_physical_plan_with_context(
            plan,
            &self.source_groups,
            self.selected_participants,
        )?;
        let leader = split.leader;
        *self.followers.lock().map_err(|_| {
            DataFusionError::Internal("remote scan rule state is poisoned".to_owned())
        })? = Some(split.followers);
        Ok(leader)
    }

    fn name(&self) -> &'static str {
        "RemoteScanRule"
    }

    fn schema_check(&self) -> bool {
        true
    }
}

/// Explicit post-remote optimizer position reserved for leader indexing.
#[derive(Debug, Default)]
pub(super) struct PostRemoteOptimizerRule;

impl PhysicalOptimizerRule for PostRemoteOptimizerRule {
    fn optimize(
        &self,
        plan: Arc<dyn ExecutionPlan>,
        _config: &ConfigOptions,
    ) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
        Ok(plan)
    }

    fn name(&self) -> &'static str {
        "PostRemoteOptimizerRule"
    }

    fn schema_check(&self) -> bool {
        true
    }
}

/// Splits at native repartition/coalesce/merge exchanges and fails closed on unsupported shapes.
///
/// # Errors
/// Returns a plan error for joins, windows, unknown Wyrd extensions, or a plan with no remote scan.
#[cfg(test)]
pub(super) fn split_physical_plan(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
) -> Result<SplitPhysicalPlan, DataFusionError> {
    split_physical_plan_with_context(plan, source_groups, usize::MAX)
}

/// Applies the exact rewrite with the authenticated selected-participant count.
fn split_physical_plan_with_context(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
    selected_participants: usize,
) -> Result<SplitPhysicalPlan, DataFusionError> {
    validate_supported(plan.as_ref())?;
    let mut followers = Vec::new();
    let source_count = collect_source_scan_ids(plan.as_ref()).len();
    if selected_participants == 1 && source_count <= 1 && source_count == 1 {
        let schema = plan.schema();
        let source_scan_ids = collect_source_scan_ids(plan.as_ref());
        followers.push(FollowerSubtree {
            result_scan_id: "oracle-result-0".to_owned(),
            plan,
            source_scan_ids,
        });
        return Ok(SplitPhysicalPlan {
            leader: Arc::new(RemoteSourcePlaceholderExec::new(
                "oracle-result-0".to_owned(),
                assignment_schema_fingerprint(schema.as_ref()),
                schema,
            )),
            followers,
        });
    }
    let leader = split_node(plan, source_groups, &mut followers)?;
    if followers.is_empty() {
        let source_scan_ids = collect_source_scan_ids(leader.as_ref());
        if source_scan_ids.is_empty() {
            return Ok(SplitPhysicalPlan { leader, followers });
        }
        let result_scan_id = "oracle-result-0".to_owned();
        let schema = leader.schema();
        followers.push(FollowerSubtree {
            result_scan_id: result_scan_id.clone(),
            plan: leader,
            source_scan_ids,
        });
        return Ok(SplitPhysicalPlan {
            leader: Arc::new(RemoteSourcePlaceholderExec::new(
                result_scan_id,
                assignment_schema_fingerprint(schema.as_ref()),
                schema,
            )),
            followers,
        });
    }
    Ok(SplitPhysicalPlan { leader, followers })
}

/// Rewrites one plan bottom-up so the innermost exchange becomes the split boundary.
///
/// The traversal is strictly bottom-up: every child is rewritten before the node
/// itself is considered. That ordering is what keeps final and global operators
/// on the leader. Once an inner exchange has been replaced, the subtree carries a
/// leader placeholder, and [`rewrite_node`] refuses to split any ancestor above
/// it. A top-down traversal would instead match the outermost exchange or
/// sort-preserving merge first and capture the final aggregate, sort, and fetch
/// into every follower, so each participant would return its own finished answer
/// and the leader would merely concatenate them.
///
/// # Errors
///
/// Returns a plan error when a rewritten child cannot be reattached to its
/// parent or when [`rewrite_node`] cannot build the replacement exchange.
fn split_node(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
    followers: &mut Vec<FollowerSubtree>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    let children = plan.children();
    if children.is_empty() {
        return Ok(plan);
    }
    let rewritten = children
        .into_iter()
        .map(|child| split_node(Arc::clone(child), source_groups, followers))
        .collect::<Result<Vec<_>, DataFusionError>>()?;
    let plan = plan.replace_children(
        rewritten,
        ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
    )?;
    rewrite_node(plan, source_groups, followers)
}

/// Applies this node's own split, assuming every descendant is already rewritten.
///
/// Each arm mirrors one pinned upstream rewrite. A repartition or coalesce
/// exchange pushes its input to the followers beneath a round-robin repartition;
/// a sort-preserving merge keeps the merge on the leader above the remote result;
/// a union splits every child independently, promoting a sorted child to a
/// sort-preserving merge so ordering survives the network; and a hash join splits
/// each child that is not already remote. A node whose subtree already holds a
/// leader placeholder is returned untouched, which is what stops an ancestor from
/// re-capturing work an inner exchange already placed on the followers.
///
/// # Errors
///
/// Returns a plan error when the partial-reduce rebuild fails, when a
/// replacement repartition cannot be constructed, or when the rewritten children
/// cannot be reattached.
fn rewrite_node(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
    followers: &mut Vec<FollowerSubtree>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    if plan.is::<RepartitionExec>() || plan.is::<CoalescePartitionsExec>() {
        return rewrite_exchange(plan, source_groups, followers);
    }
    if plan.is::<SortPreservingMergeExec>() {
        if contains_remote_result(plan.as_ref()) {
            return Ok(plan);
        }
        let source_scan_ids = collect_source_scan_ids(plan.as_ref());
        if source_scan_ids.is_empty() {
            return Ok(plan);
        }
        let remote = capture_follower(Arc::clone(&plan), source_scan_ids, followers);
        return plan.replace_children(
            vec![remote],
            ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
        );
    }
    if plan.name() == "UnionExec" {
        if contains_remote_result(plan.as_ref()) {
            return Ok(plan);
        }
        // Only a union that actually spans several authenticated tables is a
        // split boundary. Wyrd also unions one table's own source classes --
        // Iceberg, hot, and each Scribe live tail -- and that union is the
        // follower's internal source shape, not a leader boundary: every
        // participant decodes the whole union and resolves the placeholders it
        // owns. Splitting it here would put one remote scan under each source
        // class, multiply the peer RPCs by the number of classes, and strip the
        // partial aggregate that the exchange above it would otherwise push
        // down. Leaving it whole lets that ancestor exchange own the split.
        if table_group_count(plan.as_ref(), source_groups) <= 1 {
            return Ok(plan);
        }
        let mut replacements = Vec::new();
        for child in plan.children() {
            let source_scan_ids = collect_source_scan_ids(child.as_ref());
            if source_scan_ids.is_empty() {
                replacements.push(Arc::clone(child));
                continue;
            }
            let follower_plan: Arc<dyn ExecutionPlan> =
                if let Some(sort) = child.downcast_ref::<SortExec>() {
                    Arc::new(
                        SortPreservingMergeExec::new(sort.expr().clone(), Arc::clone(child))
                            .with_fetch(sort.fetch()),
                    )
                } else {
                    Arc::clone(child)
                };
            replacements.push(capture_follower(follower_plan, source_scan_ids, followers));
        }
        return plan.replace_children(
            replacements,
            ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
        );
    }
    if plan.name() == "HashJoinExec" {
        let mut replacements = Vec::new();
        for child in plan.children() {
            let source_scan_ids = collect_source_scan_ids(child.as_ref());
            if source_scan_ids.is_empty() || contains_remote_result(child.as_ref()) {
                replacements.push(Arc::clone(child));
                continue;
            }
            let partitions = child.output_partitioning().partition_count();
            let remote = capture_follower(Arc::clone(child), source_scan_ids, followers);
            replacements.push(Arc::new(RepartitionExec::try_new(
                remote,
                datafusion::physical_expr::Partitioning::RoundRobinBatch(partitions),
            )?) as Arc<dyn ExecutionPlan>);
        }
        return plan.replace_children(
            replacements,
            ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
        );
    }
    Ok(plan)
}

/// Counts the distinct authenticated table groups a subtree's sources belong to.
///
/// A scan id with no recorded group is ignored rather than counted, so an
/// unmapped source can never make a single-table subtree look multi-table and
/// trigger a split boundary that does not exist.
fn table_group_count(plan: &dyn ExecutionPlan, source_groups: &HashMap<String, String>) -> usize {
    collect_source_scan_ids(plan)
        .iter()
        .filter_map(|scan_id| source_groups.get(scan_id))
        .collect::<HashSet<_>>()
        .len()
}

/// Splits one repartition or coalesce exchange into a follower subtree.
///
/// The exchange's input becomes the follower plan after the partial-reduce
/// rebuild, and the leader keeps the same exchange over a round-robin
/// repartition of the remote result so downstream partitioning is unchanged. The
/// split is skipped when the subtree is already remote, carries no authenticated
/// source, or spans more than one table group, because a follower subtree must
/// address exactly one authenticated table binding.
///
/// # Errors
///
/// Returns a plan error when the partial-reduce rebuild fails, when the
/// round-robin repartition cannot be constructed, or when the replacement child
/// cannot be reattached to the exchange.
fn rewrite_exchange(
    plan: Arc<dyn ExecutionPlan>,
    source_groups: &HashMap<String, String>,
    followers: &mut Vec<FollowerSubtree>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    if contains_remote_result(plan.as_ref()) {
        return Ok(plan);
    }
    let Some(input) = plan.children().first().map(|child| Arc::clone(child)) else {
        return Ok(plan);
    };
    let source_scan_ids = collect_source_scan_ids(input.as_ref());
    if source_scan_ids.is_empty() {
        return Ok(plan);
    }
    let child_groups = source_scan_ids
        .iter()
        .filter_map(|scan_id| source_groups.get(scan_id))
        .collect::<HashSet<_>>();
    if child_groups.len() > 1 {
        return Ok(plan);
    }
    let partitions = input.output_partitioning().partition_count();
    let follower_plan = wrap_partial_reduce(true, input)?;
    let remote = capture_follower(follower_plan, source_scan_ids, followers);
    let repartition = Arc::new(RepartitionExec::try_new(
        remote,
        datafusion::physical_expr::Partitioning::RoundRobinBatch(partitions),
    )?) as Arc<dyn ExecutionPlan>;
    plan.replace_children(
        vec![repartition],
        ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
    )
}

/// Captures one follower subtree and returns its leader-side remote placeholder.
fn capture_follower(
    plan: Arc<dyn ExecutionPlan>,
    source_scan_ids: Vec<String>,
    followers: &mut Vec<FollowerSubtree>,
) -> Arc<dyn ExecutionPlan> {
    let result_scan_id = format!("oracle-result-{}", followers.len());
    let schema = plan.schema();
    followers.push(FollowerSubtree {
        result_scan_id: result_scan_id.clone(),
        plan,
        source_scan_ids,
    });
    Arc::new(RemoteSourcePlaceholderExec::new(
        result_scan_id,
        assignment_schema_fingerprint(schema.as_ref()),
        schema,
    ))
}

/// Stops rewrites below a leader placeholder already produced by this rule.
fn contains_remote_result(plan: &dyn ExecutionPlan) -> bool {
    plan.downcast_ref::<RemoteSourcePlaceholderExec>()
        .is_some_and(|scan| scan.scan_id().starts_with("oracle-result-"))
        || plan
            .children()
            .into_iter()
            .any(|child| contains_remote_result(child.as_ref()))
}

/// Rebuilds a partial aggregate against its partial-output schema before remote execution.
///
/// Disabled, non-aggregate, and non-partial inputs are returned unchanged.
///
/// # Errors
/// Returns a plan error when group columns or the partial-reduce aggregate cannot be rebuilt.
fn wrap_partial_reduce(
    enabled: bool,
    input: Arc<dyn ExecutionPlan>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    if !enabled {
        return Ok(input);
    }
    let Some(aggregate) = input.downcast_ref::<AggregateExec>() else {
        return Ok(input);
    };
    if !matches!(
        aggregate.mode(),
        AggregateMode::Partial | AggregateMode::PartialReduce
    ) {
        return Ok(input);
    }
    let partial_schema = input.schema();
    let group_exprs = aggregate
        .group_expr()
        .expr()
        .iter()
        .map(|(_, name)| {
            let index = partial_schema.index_of(name)?;
            Ok((
                Arc::new(PhysicalColumn::new(name, index)) as Arc<dyn PhysicalExpr>,
                name.clone(),
            ))
        })
        .collect::<Result<Vec<_>, DataFusionError>>()?;
    let hash_exprs = group_exprs
        .iter()
        .map(|(expression, _)| Arc::clone(expression))
        .collect::<Vec<_>>();
    let group_by = PhysicalGroupBy::new(
        group_exprs,
        aggregate.group_expr().null_expr().to_vec(),
        aggregate.group_expr().groups().to_vec(),
        !aggregate.group_expr().is_single(),
    );
    let reduced_input: Arc<dyn ExecutionPlan> = if hash_exprs.is_empty() {
        Arc::new(CoalescePartitionsExec::new(Arc::clone(&input)))
    } else {
        Arc::new(RepartitionExec::try_new(
            Arc::clone(&input),
            datafusion::physical_expr::Partitioning::Hash(
                hash_exprs,
                input.output_partitioning().partition_count(),
            ),
        )?)
    };
    Ok(Arc::new(AggregateExec::try_new(
        AggregateMode::PartialReduce,
        group_by,
        aggregate.aggr_expr().to_vec(),
        vec![None; aggregate.aggr_expr().len()],
        reduced_input,
        aggregate.input_schema(),
    )?))
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
    if let Some(scan) = plan.downcast_ref::<RemoteSourcePlaceholderExec>()
        && !scan.scan_id().starts_with("oracle-result-")
        && seen.insert(scan.scan_id().to_owned())
    {
        scans.push(scan.scan_id().to_owned());
    }
    for child in plan.children() {
        collect_scans(child.as_ref(), seen, scans);
    }
}

/// Collects the closed predicate/projection closure the provider attached to
/// every [`RemoteSourcePlaceholderExec`] placeholder in `plan` (see
/// `oracle::exec::OracleTableProvider::scan` and
/// [`RemoteSourcePlaceholderExec::with_closure`]).
///
/// This is the caller's only way to learn what the already-optimized,
/// already-planned physical tree actually pushed down: `oracle_assignments`
/// is built as a safe full-schema placeholder before `SessionContext::sql`
/// runs, and only `scan()` — invoked during `create_physical_plan()` — knows
/// the real requested projection and supported filters. A scan id with no
/// entry here means the placeholder was pruned out of the final plan or
/// never received a closure; the caller must keep that assignment's existing
/// safe default rather than treat the absence as "empty projection".
///
/// Both halves of the split are walked. A source placeholder survives in the
/// leader only when the split boundary landed above it; for an ordinary
/// projection/filter scan the whole leaf is pushed below the exchange, so the
/// placeholder that carries the closure lives in a follower subtree. Walking
/// the leader alone would silently leave every such assignment on its
/// unpruned default and lose closed-predicate pruning for the dispatched
/// leaf.
pub(super) fn collect_remote_scan_closures(
    split: &SplitPhysicalPlan,
) -> HashMap<
    String,
    (
        Vec<String>,
        Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    ),
> {
    let mut closures = HashMap::new();
    collect_closures(split.leader.as_ref(), &mut closures);
    for follower in &split.followers {
        collect_closures(follower.plan.as_ref(), &mut closures);
    }
    closures
}

/// Depth-first closure collection mirroring [`collect_scans`].
fn collect_closures(
    plan: &dyn ExecutionPlan,
    closures: &mut HashMap<
        String,
        (
            Vec<String>,
            Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
        ),
    >,
) {
    if let Some(scan) = plan.downcast_ref::<RemoteSourcePlaceholderExec>()
        && !scan.scan_id().starts_with("oracle-result-")
        && !scan.required_columns().is_empty()
    {
        closures.insert(
            scan.scan_id().to_owned(),
            (scan.required_columns().to_vec(), scan.predicates().to_vec()),
        );
    }
    for child in plan.children() {
        collect_closures(child.as_ref(), closures);
    }
}

/// Rejects operators whose distributed semantics are not approved for this splitter.
fn validate_supported(plan: &dyn ExecutionPlan) -> Result<(), DataFusionError> {
    let name = plan.name();
    if (name.contains("Join") && name != "HashJoinExec") || name.contains("Window") {
        return Err(DataFusionError::Plan(format!(
            "unsupported distributed Oracle operator: {name}"
        )));
    }
    if name.ends_with("Exec")
        && plan.children().is_empty()
        && !matches!(
            name,
            "RemoteSourcePlaceholderExec" | "EmptyExec" | "DataSourceExec" | "MemorySourceConfig"
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
#[cfg(test)]
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

/// Replaces every leader result placeholder with its network-backed execution node.
///
/// # Errors
/// Returns a plan error for a missing, duplicate, or unreferenced remote result node.
pub(super) fn substitute_remote_plans(
    plan: Arc<dyn ExecutionPlan>,
    mut replacements: HashMap<String, Arc<dyn ExecutionPlan>>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    let leader = substitute_plan_node(plan, &mut replacements)?;
    if !replacements.is_empty() {
        return Err(DataFusionError::Plan(
            "distributed Oracle retained an unreferenced remote plan".to_owned(),
        ));
    }
    Ok(leader)
}

/// Recursively consumes one exact network plan per leader placeholder.
fn substitute_plan_node(
    plan: Arc<dyn ExecutionPlan>,
    replacements: &mut HashMap<String, Arc<dyn ExecutionPlan>>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    if let Some(scan) = plan.downcast_ref::<RemoteSourcePlaceholderExec>()
        && scan.scan_id().starts_with("oracle-result-")
    {
        return replacements.remove(scan.scan_id()).ok_or_else(|| {
            DataFusionError::Plan(format!("missing remote plan {}", scan.scan_id()))
        });
    }
    let children = plan.children();
    if children.is_empty() {
        return Ok(plan);
    }
    let children = children
        .into_iter()
        .map(|child| substitute_plan_node(Arc::clone(child), replacements))
        .collect::<Result<Vec<_>, _>>()?;
    plan.replace_children(
        children,
        ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
    )
}

/// Recursively consumes one exact remote result per leader placeholder.
#[cfg(test)]
fn substitute_node(
    plan: Arc<dyn ExecutionPlan>,
    results: &mut HashMap<String, Vec<RecordBatch>>,
) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
    if let Some(scan) = plan.downcast_ref::<RemoteSourcePlaceholderExec>()
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
    plan.replace_children(
        replacements,
        ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{ArrayRef, Int64Array, StringArray, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
    use arrow::record_batch::RecordBatch;
    use async_trait::async_trait;
    use datafusion::catalog::Session;
    use datafusion::datasource::{TableProvider, TableType};
    use datafusion::execution::context::SessionContext;
    use datafusion::logical_expr::Expr;

    /// Table provider whose physical leaf is one authenticated remote placeholder.
    #[derive(Debug)]
    struct RemoteTable {
        /// Exact common schema exposed to logical and physical planning.
        schema: SchemaRef,
        /// Authenticated source identity exposed by the physical placeholder.
        scan_id: String,
    }

    #[async_trait]
    impl TableProvider for RemoteTable {
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
            state: &dyn Session,
            _projection: Option<&Vec<usize>>,
            _filters: &[Expr],
            _limit: Option<usize>,
        ) -> Result<Arc<dyn ExecutionPlan>, DataFusionError> {
            // Mirrors the production provider: the placeholder advertises the
            // session's target partitions, so planning does not insert a
            // round-robin repartition above it and the split boundary lands on
            // the same exchange these tests assert against.
            Ok(Arc::new(
                RemoteSourcePlaceholderExec::new(
                    self.scan_id.clone(),
                    assignment_schema_fingerprint(self.schema.as_ref()),
                    Arc::clone(&self.schema),
                )
                .with_partitions(state.config().target_partitions()),
            ))
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

    /// Distributed hash joins wrap both authenticated children independently.
    #[tokio::test]
    async fn distributed_split_wraps_hash_join_children() {
        let session = SessionContext::new();
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::UInt64, false)]));
        for table in ["left_rows", "right_rows"] {
            session
                .register_table(
                    table,
                    Arc::new(RemoteTable {
                        schema: Arc::clone(&schema),
                        scan_id: table.to_owned(),
                    }),
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
        let groups = HashMap::from([
            ("left_rows".to_owned(), "left_rows".to_owned()),
            ("right_rows".to_owned(), "right_rows".to_owned()),
        ]);
        let split = split_physical_plan(plan, &groups).expect("hash join children split");
        assert_eq!(split.followers.len(), 2);
        assert_eq!(split.leader.name(), "HashJoinExec");
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
            .register_table(
                "events",
                Arc::new(RemoteTable {
                    schema,
                    scan_id: "scan".to_owned(),
                }),
            )
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
        assert!(leader_shape.contains("RemoteSourcePlaceholderExec"));
        // The follower reduces its own share and nothing more. If any global
        // operator crossed the boundary, every participant would return a
        // finished answer and the leader would concatenate rather than merge
        // them, so an aggregate would report one participant's value instead of
        // the total.
        assert!(
            !follower_shape.contains("GlobalLimitExec"),
            "follower plan:\n{follower_shape}"
        );
        assert!(
            !follower_shape.contains("SortPreservingMergeExec"),
            "follower plan:\n{follower_shape}"
        );
        assert!(
            !follower_shape.contains("AggregateExec: mode=FinalPartitioned"),
            "follower plan:\n{follower_shape}"
        );
        assert!(
            follower_shape.contains("AggregateExec: mode=Partial"),
            "follower plan:\n{follower_shape}"
        );

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
        // Two participants each counted part of one group; the leader's final
        // aggregate must sum them. Picking the larger partial instead is the
        // exact symptom of a global operator having been pushed to the
        // followers.
        assert_eq!(total, Some(5));
    }

    /// A final operator without a lower exchange uses the pinned whole-plan fallback.
    #[test]
    fn distributed_split_wraps_final_plan_at_top() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Utf8,
            false,
        )]));
        let remote = Arc::new(RemoteSourcePlaceholderExec::new(
            "scan".to_owned(),
            assignment_schema_fingerprint(schema.as_ref()),
            schema,
        ));
        let plan = Arc::new(GlobalLimitExec::new(remote, 0, Some(1)));
        let groups = HashMap::from([("scan".to_owned(), "events".to_owned())]);
        let split = split_physical_plan(plan, &groups).expect("whole-plan fallback");
        assert_eq!(split.followers.len(), 1);
        assert!(split.followers[0].plan.is::<GlobalLimitExec>());
        assert!(split.leader.is::<RemoteSourcePlaceholderExec>());
    }
}
