//! Verifier step-graph planning. Pure function; no IO.
//!
//! Uses `petgraph::algo::tarjan_scc` for cycle-witness recovery so the
//! validation path can return the smallest deterministic cycle witness.

use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::algo::tarjan_scc;
use petgraph::graphmap::DiGraphMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::ids::TaskId;

/// Topologically sorted task DAG.
///
/// Stages are produced in dependency-layer order. Stage 0 contains all tasks
/// with no dependencies. Each later stage contains tasks whose dependencies
/// were all emitted by earlier stages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlan {
    /// Topological execution layers in dependency order.
    pub stages: Vec<Stage>,
    /// Total task count across all stages. Equals `tasks.len()` from the
    /// input map.
    pub task_count: usize,
}

/// One topological execution layer.
///
/// Tasks within a stage are independent and may execute in parallel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Zero-based topological layer index.
    pub index: u32,
    /// Task ids in this stage, sorted lexicographically for determinism.
    pub tasks: Vec<TaskId>,
}

/// DAG-validation failures.
#[derive(Debug, Clone, PartialEq, Error, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DagError {
    /// One or more cycles were detected.
    ///
    /// `cycle` is the smallest closed walk reachable from the offending SCC,
    /// sorted starting from the lexicographically smallest id and wrapping
    /// back to it for clarity.
    #[error("dag has a cycle: {cycle:?}")]
    Cycle {
        /// The deterministic closed-walk witness.
        cycle: Vec<TaskId>,
    },
    /// A `depends_on` entry references a task id absent from the supplied map.
    #[error("task {task:?} depends on missing task {dep:?}")]
    MissingDependency {
        /// The task declaring the dependency.
        task: TaskId,
        /// The missing dependency id.
        dep: TaskId,
    },
    /// A task lists itself in `depends_on`.
    #[error("task {task:?} depends on itself")]
    SelfLoop {
        /// The task with the self-dependency.
        task: TaskId,
    },
}

/// Plans dependency stages for any graph of uniquely named nodes.
///
/// Each item names one node and the nodes it depends on. Nodes are emitted
/// in dependency-layer order with each stage sorted lexicographically. The
/// caller guarantees node ids are unique; a repeated id replaces the earlier
/// node's position in the graph.
///
/// # Errors
/// Returns [`DagError::MissingDependency`] when a node references an absent
/// dependency, [`DagError::SelfLoop`] when a node depends on itself, and
/// [`DagError::Cycle`] when the remaining dependency graph has a cycle.
pub fn plan_stages<'a, I, D>(nodes: I) -> Result<ExecutionPlan, DagError>
where
    I: IntoIterator<Item = (&'a TaskId, D)>,
    D: IntoIterator<Item = &'a TaskId>,
{
    let nodes: Vec<(&TaskId, Vec<&TaskId>)> = nodes
        .into_iter()
        .map(|(id, deps)| (id, deps.into_iter().collect()))
        .collect();
    let mut in_degree: HashMap<&TaskId, u32> = HashMap::with_capacity(nodes.len());
    let mut adj: HashMap<&TaskId, Vec<&TaskId>> = HashMap::with_capacity(nodes.len());
    for (id, _) in &nodes {
        in_degree.insert(id, 0);
        adj.insert(id, Vec::new());
    }

    for &(id, ref deps) in &nodes {
        for &dep in deps {
            if dep == id {
                return Err(DagError::SelfLoop { task: id.clone() });
            }
            let Some(downstream_tasks) = adj.get_mut(dep) else {
                return Err(DagError::MissingDependency {
                    task: id.clone(),
                    dep: dep.clone(),
                });
            };

            downstream_tasks.push(id);
            if let Some(degree) = in_degree.get_mut(id) {
                *degree += 1;
            }
        }
    }

    let mut plan = Vec::new();
    let mut emitted: usize = 0;
    let mut ready: Vec<&TaskId> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    ready.sort();

    let mut stage_idx: u32 = 0;
    while !ready.is_empty() {
        let stage_tasks: Vec<TaskId> = ready.iter().map(|task| (*task).clone()).collect();
        emitted = emitted.saturating_add(stage_tasks.len());
        plan.push(Stage {
            index: stage_idx,
            tasks: stage_tasks,
        });
        stage_idx = stage_idx.saturating_add(1);

        let mut next_ready = Vec::new();
        for id in ready {
            if let Some(downstreams) = adj.get(id) {
                for &downstream in downstreams {
                    if let Some(degree) = in_degree.get_mut(downstream) {
                        *degree -= 1;
                        if *degree == 0 {
                            next_ready.push(downstream);
                        }
                    }
                }
            }
        }
        next_ready.sort();
        ready = next_ready;
    }

    if emitted == in_degree.len() {
        return Ok(ExecutionPlan {
            stages: plan,
            task_count: emitted,
        });
    }

    let remaining: HashSet<&TaskId> = in_degree
        .iter()
        .filter(|(_, degree)| **degree > 0)
        .map(|(id, _)| *id)
        .collect();
    let cycle = smallest_cycle(&adj, &remaining);
    Err(DagError::Cycle { cycle })
}

/// Cycle-witness recovery via `petgraph::algo::tarjan_scc`.
fn smallest_cycle<'a>(
    adj: &HashMap<&'a TaskId, Vec<&'a TaskId>>,
    remaining: &HashSet<&'a TaskId>,
) -> Vec<TaskId> {
    let mut graph: DiGraphMap<&TaskId, ()> = DiGraphMap::new();
    for vertex in remaining {
        graph.add_node(*vertex);
    }
    for vertex in remaining {
        for &edge in &adj[vertex] {
            if remaining.contains(edge) {
                graph.add_edge(*vertex, edge, ());
            }
        }
    }

    let mut sccs: Vec<Vec<&TaskId>> = tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() >= 2 || (scc.len() == 1 && self_loops_to(adj, scc[0])))
        .collect();
    sccs.sort_by(|left, right| {
        left.len()
            .cmp(&right.len())
            .then_with(|| left.iter().min().cmp(&right.iter().min()))
    });
    let Some(scc) = sccs.first() else {
        return Vec::new();
    };

    let scc_set: HashSet<&TaskId> = scc.iter().copied().collect();
    let Some(&start) = scc.iter().min() else {
        return Vec::new();
    };
    let mut parent: HashMap<&TaskId, &TaskId> = HashMap::new();
    let mut queue: VecDeque<&TaskId> = VecDeque::new();
    queue.push_back(start);
    parent.insert(start, start);

    let mut closing: Option<&TaskId> = None;
    'bfs: while let Some(vertex) = queue.pop_front() {
        for &edge in &adj[vertex] {
            if !scc_set.contains(edge) {
                continue;
            }
            if edge == start {
                closing = Some(vertex);
                break 'bfs;
            }
            if !parent.contains_key(edge) {
                parent.insert(edge, vertex);
                queue.push_back(edge);
            }
        }
    }

    let mut walk_rev = Vec::new();
    if let Some(last) = closing {
        let mut current = last;
        while current != start {
            walk_rev.push(current);
            current = parent[current];
        }
    }
    let mut walk: Vec<&TaskId> = vec![start];
    walk.extend(walk_rev.into_iter().rev());
    walk.push(start);

    walk.into_iter().cloned().collect()
}

fn self_loops_to(adj: &HashMap<&TaskId, Vec<&TaskId>>, vertex: &TaskId) -> bool {
    adj.get(vertex).is_some_and(|edges| edges.contains(&vertex))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plans `nodes`, each named with the ids it depends on.
    fn plan(nodes: &[(&str, &[&str])]) -> Result<ExecutionPlan, DagError> {
        let id = |value: &str| TaskId::new(value).expect("static task id is valid");
        let nodes: Vec<(TaskId, Vec<TaskId>)> = nodes
            .iter()
            .map(|(node, deps)| (id(node), deps.iter().map(|dep| id(dep)).collect()))
            .collect();
        plan_stages(nodes.iter().map(|(node, deps)| (node, deps.iter())))
    }

    /// Stage task ids as strings.
    fn names(stage: &Stage) -> Vec<&str> {
        stage.tasks.iter().map(TaskId::as_str).collect()
    }

    /// A three-node cycle yields a closed witness starting and ending at the
    /// smallest id.
    #[test]
    fn triangle_cycle_returns_witness() {
        match plan(&[("a", &["c"]), ("b", &["a"]), ("c", &["b"])]) {
            Err(DagError::Cycle { cycle }) => {
                let names: Vec<&str> = cycle.iter().map(TaskId::as_str).collect();
                assert_eq!(names.first(), Some(&"a"));
                assert_eq!(names.last(), Some(&"a"));
                assert_eq!(cycle.len() - 1, 3);
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    /// With two cycles, the witness is the smaller one.
    #[test]
    fn smallest_cycle_witness_picks_3_over_5() {
        let nodes: [(&str, &[&str]); 8] = [
            ("a", &["c"]),
            ("b", &["a"]),
            ("c", &["b"]),
            ("v", &["w"]),
            ("w", &["z"]),
            ("x", &["v"]),
            ("y", &["x"]),
            ("z", &["y"]),
        ];
        match plan(&nodes) {
            Err(DagError::Cycle { cycle }) => {
                assert_eq!(cycle.len() - 1, 3, "smallest cycle is 3, got {cycle:?}");
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    /// A chain plans one node per stage in dependency order.
    #[test]
    fn linear_three_stages() {
        let plan = plan(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]).expect("plans");
        assert_eq!(plan.task_count, 3);
        let stages: Vec<Vec<&str>> = plan.stages.iter().map(names).collect();
        assert_eq!(stages, vec![vec!["a"], vec!["b"], vec!["c"]]);
    }

    /// Independent middle nodes share one sorted stage.
    #[test]
    fn diamond_three_stages_parallel_middle() {
        let plan = plan(&[("a", &[]), ("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])])
            .expect("plans");
        assert_eq!(plan.stages.len(), 3);
        assert_eq!(names(&plan.stages[1]), vec!["b", "c"]);
    }

    /// Disconnected roots share stage zero.
    #[test]
    fn disconnected_components_share_stage_zero() {
        let plan = plan(&[("a", &[]), ("b", &[])]).expect("plans");
        assert_eq!(plan.stages.len(), 1);
        assert_eq!(names(&plan.stages[0]), vec!["a", "b"]);
    }

    /// A dependency on an absent node is refused with both ids.
    #[test]
    fn unknown_dependency_rejected() {
        match plan(&[("a", &["ghost"])]) {
            Err(DagError::MissingDependency { task, dep }) => {
                assert_eq!(task.as_str(), "a");
                assert_eq!(dep.as_str(), "ghost");
            }
            other => panic!("expected MissingDependency, got {other:?}"),
        }
    }

    /// A node depending on itself is refused.
    #[test]
    fn task_depending_on_itself_rejected() {
        match plan(&[("loop", &["loop"])]) {
            Err(DagError::SelfLoop { task }) => assert_eq!(task.as_str(), "loop"),
            other => panic!("expected SelfLoop, got {other:?}"),
        }
    }
}
