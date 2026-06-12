//! Spec-load DAG validation. Pure function; no IO.
//!
//! Uses `petgraph::algo::tarjan_scc` for cycle-witness recovery so the
//! validation path can return the smallest deterministic cycle witness.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use petgraph::algo::tarjan_scc;
use petgraph::graphmap::DiGraphMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::WyrdError;

use super::ids::TaskId;
use super::task::EvalTask;

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

impl From<DagError> for WyrdError {
    fn from(error: DagError) -> Self {
        let details = match &error {
            DagError::Cycle { cycle } => serde_json::json!({
                "kind": "cycle",
                "cycle": cycle.iter().map(TaskId::as_str).collect::<Vec<_>>(),
            }),
            DagError::MissingDependency { task, dep } => serde_json::json!({
                "kind": "missing_dependency",
                "task": task.as_str(),
                "dep": dep.as_str(),
            }),
            DagError::SelfLoop { task } => serde_json::json!({
                "kind": "self_loop",
                "task": task.as_str(),
            }),
        };

        WyrdError::ValaTaskDagInvalid {
            message: error.to_string(),
            details,
        }
    }
}

/// Validate the task map as a DAG.
///
/// Returns the topologically sorted execution plan or a typed DAG error.
/// Complexity is O(V + E). The cycle-witness phase only runs on the unfinished
/// subgraph and allocates one `DiGraphMap` over that subset.
///
/// # Errors
/// Returns [`DagError::MissingDependency`] when a task references an absent
/// dependency, [`DagError::SelfLoop`] when a task depends on itself, and
/// [`DagError::Cycle`] when the remaining dependency graph has a cycle.
pub fn validate_dag(tasks: &BTreeMap<TaskId, EvalTask>) -> Result<ExecutionPlan, DagError> {
    let mut in_degree: HashMap<&TaskId, u32> = HashMap::with_capacity(tasks.len());
    let mut adj: HashMap<&TaskId, Vec<&TaskId>> = HashMap::with_capacity(tasks.len());
    for id in tasks.keys() {
        in_degree.insert(id, 0);
        adj.insert(id, Vec::new());
    }

    for (id, task) in tasks {
        for dep in task.depends_on() {
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

    if emitted == tasks.len() {
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
    adj.get(vertex)
        .map(|edges| edges.contains(&vertex))
        .unwrap_or(false)
}
