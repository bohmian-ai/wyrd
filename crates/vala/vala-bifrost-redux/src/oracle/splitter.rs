//! Native physical-plan exchange splitting for Oracle follower execution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use datafusion::datasource::memory::MemorySourceConfig;
use datafusion::error::DataFusionError;
use datafusion::physical_plan::ExecutionPlan;
use datafusion::physical_plan::coalesce_partitions::CoalescePartitionsExec;
use datafusion::physical_plan::repartition::RepartitionExec;
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
        if (at_exchange || at_role_boundary) && !source_scan_ids.is_empty() {
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
        let batches = results.remove(scan.scan_id()).ok_or_else(|| {
            DataFusionError::Plan(format!("missing remote result {}", scan.scan_id()))
        })?;
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
