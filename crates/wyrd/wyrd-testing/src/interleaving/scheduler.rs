//! A small cooperative executor for reproducible async interleavings.

use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use futures_util::task::noop_waker_ref;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng, rngs::StdRng};
use thiserror::Error;

/// Default upper bound for a matrix of replayed schedules.
pub const DEFAULT_MAX_PERMUTATIONS: usize = 32;

/// Identifier assigned to a task when it is spawned.
pub type TaskId = usize;

type BoxFuture = Pin<Box<dyn Future<Output = ()> + 'static>>;

struct Task {
    future: Option<BoxFuture>,
}

struct SchedulerState {
    seed: u64,
    max_permutations: usize,
    max_steps: usize,
    forced_order: Option<Vec<TaskId>>,
    tasks: Vec<Task>,
}

/// Error returned when a cooperative schedule cannot complete.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SchedulerError {
    /// The schedule was asked to run without any spawned tasks.
    #[error("interleaving scheduler has no spawned tasks")]
    NoTasks,
    /// A task remained pending after the executor's safety bound.
    #[error("interleaving scheduler exceeded {steps} polling steps for seed {seed}")]
    StepLimitExceeded { seed: u64, steps: usize },
    /// The scheduler selected a task whose future was already completed.
    #[error("interleaving scheduler selected missing task {task_id}")]
    MissingTask { task_id: TaskId },
}

/// The result of one deterministic scheduler run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunReport {
    seed: u64,
    ordering: Vec<TaskId>,
    completed_tasks: usize,
}

impl RunReport {
    /// Return the seed that produced this schedule.
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Return task IDs in the order in which the executor polled them.
    #[must_use]
    pub fn ordering(&self) -> &[TaskId] {
        &self.ordering
    }

    /// Return the number of tasks that reached `Poll::Ready`.
    #[must_use]
    pub const fn completed_tasks(&self) -> usize {
        self.completed_tasks
    }
}

/// Seeded cooperative scheduler used by interleaving tests.
///
/// Futures are polled on the current thread and do not need to be `Send`. A
/// test task creates a yield point with [`Scheduler::yield_now`]. The
/// scheduler chooses a different active task between yield points whenever
/// one is available, making a race fixture deterministic without depending on
/// Tokio's runtime scheduling.
#[derive(Clone)]
pub struct Scheduler {
    state: Rc<RefCell<SchedulerState>>,
}

impl Scheduler {
    /// Create a scheduler whose random choices are derived only from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: Rc::new(RefCell::new(SchedulerState {
                seed,
                max_permutations: DEFAULT_MAX_PERMUTATIONS,
                max_steps: 1_000_000,
                forced_order: None,
                tasks: Vec::new(),
            })),
        }
    }

    /// Set the maximum number of schedules used by [`Self::run_permutations`].
    #[must_use]
    pub fn with_max_permutations(self, max_permutations: usize) -> Self {
        self.state.borrow_mut().max_permutations = max_permutations.max(1);
        self
    }

    /// Set the polling safety bound for one schedule.
    #[must_use]
    pub fn with_max_steps(self, max_steps: usize) -> Self {
        self.state.borrow_mut().max_steps = max_steps.max(1);
        self
    }

    /// Return the seed configured for this scheduler.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.state.borrow().seed
    }

    /// Return the configured schedule replay cap.
    #[must_use]
    pub fn max_permutations(&self) -> usize {
        self.state.borrow().max_permutations
    }

    /// Register a future for cooperative execution.
    pub fn spawn<F>(&self, future: F) -> TaskId
    where
        F: Future<Output = ()> + 'static,
    {
        let mut state = self.state.borrow_mut();
        let task_id = state.tasks.len();
        state.tasks.push(Task {
            future: Some(Box::pin(future)),
        });
        task_id
    }

    /// Create a future that yields exactly once when polled.
    #[must_use]
    pub const fn yield_now(&self) -> YieldNow {
        YieldNow { yielded: false }
    }

    /// Run all futures using one seeded schedule.
    ///
    /// The futures must use cooperative yield points. External futures that
    /// wait for a runtime event are not advanced by this executor and will
    /// eventually return [`SchedulerError::StepLimitExceeded`].
    pub fn run(&self) -> Result<RunReport, SchedulerError> {
        let (seed, max_steps, forced_order, task_count) = {
            let state = self.state.borrow();
            (
                state.seed,
                state.max_steps,
                state.forced_order.clone(),
                state.tasks.len(),
            )
        };

        if task_count == 0 {
            return Err(SchedulerError::NoTasks);
        }

        let mut rng = StdRng::seed_from_u64(seed);
        let mut ordering = Vec::new();
        let mut completed_tasks = 0;
        let waker = noop_waker_ref();
        let mut cx = Context::from_waker(waker);

        for step in 0..max_steps {
            let Some(task_id) = self.select_task(step, forced_order.as_deref(), &mut rng) else {
                return Ok(RunReport {
                    seed,
                    ordering,
                    completed_tasks,
                });
            };

            let mut future = self
                .state
                .borrow_mut()
                .tasks
                .get_mut(task_id)
                .and_then(|task| task.future.take())
                .ok_or(SchedulerError::MissingTask { task_id })?;

            ordering.push(task_id);
            let poll = future.as_mut().poll(&mut cx);
            let mut state = self.state.borrow_mut();
            match poll {
                Poll::Ready(()) => {
                    completed_tasks += 1;
                }
                Poll::Pending => {
                    state.tasks[task_id].future = Some(future);
                }
            }
        }

        Err(SchedulerError::StepLimitExceeded {
            seed,
            steps: max_steps,
        })
    }

    /// Rebuild and run the spawned task set for bounded task-order
    /// permutations. The factory is called once for each schedule and receives
    /// the fresh scheduler on which it must spawn the tasks.
    pub fn run_permutations<F>(&self, factory: F) -> Result<Vec<RunReport>, SchedulerError>
    where
        F: Fn(&Scheduler),
    {
        let probe = Scheduler::new(self.seed()).with_max_steps(self.max_steps());
        factory(&probe);
        let task_count = probe.task_count();
        if task_count == 0 {
            return Err(SchedulerError::NoTasks);
        }

        let mut rng = StdRng::seed_from_u64(self.seed());
        let permutations = bounded_permutations(task_count, self.max_permutations(), &mut rng);
        permutations
            .into_iter()
            .map(|order| {
                let scheduler = Scheduler::new(self.seed())
                    .with_max_steps(self.max_steps())
                    .with_forced_order(order);
                factory(&scheduler);
                scheduler.run()
            })
            .collect()
    }

    fn with_forced_order(self, order: Vec<TaskId>) -> Self {
        self.state.borrow_mut().forced_order = Some(order);
        self
    }

    fn task_count(&self) -> usize {
        self.state.borrow().tasks.len()
    }

    fn max_steps(&self) -> usize {
        self.state.borrow().max_steps
    }

    fn select_task(
        &self,
        step: usize,
        forced_order: Option<&[TaskId]>,
        rng: &mut StdRng,
    ) -> Option<TaskId> {
        let state = self.state.borrow();
        let active = |task_id: TaskId| {
            state
                .tasks
                .get(task_id)
                .is_some_and(|task| task.future.is_some())
        };

        if let Some(order) = forced_order
            && !order.is_empty()
        {
            let offset = step % order.len();
            for index in 0..order.len() {
                let task_id = order[(offset + index) % order.len()];
                if active(task_id) {
                    return Some(task_id);
                }
            }
        }

        let active_tasks: Vec<_> = (0..state.tasks.len()).filter(|&id| active(id)).collect();
        if active_tasks.is_empty() {
            None
        } else {
            Some(active_tasks[rng.random_range(0..active_tasks.len())])
        }
    }
}

/// A single cooperative yield point.
pub struct YieldNow {
    yielded: bool,
}

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            Poll::Pending
        }
    }
}

fn bounded_permutations(task_count: usize, cap: usize, rng: &mut StdRng) -> Vec<Vec<TaskId>> {
    let mut output = Vec::new();
    let mut remaining: Vec<TaskId> = (0..task_count).collect();
    build_permutations(&mut remaining, &mut Vec::new(), cap, rng, &mut output);
    output
}

fn build_permutations(
    remaining: &mut Vec<TaskId>,
    prefix: &mut Vec<TaskId>,
    cap: usize,
    rng: &mut StdRng,
    output: &mut Vec<Vec<TaskId>>,
) {
    if output.len() >= cap {
        return;
    }
    if remaining.is_empty() {
        output.push(prefix.clone());
        return;
    }

    let mut indices: Vec<_> = (0..remaining.len()).collect();
    indices.shuffle(rng);
    for index in indices {
        let task_id = remaining.remove(index);
        prefix.push(task_id);
        build_permutations(remaining, prefix, cap, rng, output);
        prefix.pop();
        remaining.insert(index, task_id);
        if output.len() >= cap {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::HashSet;
    use std::rc::Rc;

    use super::*;

    fn schedule(seed: u64) -> RunReport {
        let scheduler = Scheduler::new(seed);
        for _ in 0..3 {
            let task_scheduler = scheduler.clone();
            let future_scheduler = task_scheduler.clone();
            task_scheduler.spawn(async move {
                future_scheduler.yield_now().await;
                future_scheduler.yield_now().await;
            });
        }
        scheduler.run().expect("schedule completes")
    }

    #[test]
    fn test_scheduler_deterministic_from_seed() {
        assert_eq!(schedule(7), schedule(7));
        assert_ne!(schedule(7).ordering(), schedule(8).ordering());
    }

    #[test]
    fn test_scheduler_enumerates_bounded_permutations() {
        let scheduler = Scheduler::new(7).with_max_permutations(4);
        let reports = scheduler
            .run_permutations(|scheduler| {
                for _ in 0..3 {
                    let task_scheduler = scheduler.clone();
                    let future_scheduler = task_scheduler.clone();
                    task_scheduler.spawn(async move {
                        future_scheduler.yield_now().await;
                    });
                }
            })
            .expect("permutations complete");

        let orderings: HashSet<_> = reports
            .iter()
            .map(|report| report.ordering().to_vec())
            .collect();
        assert_eq!(reports.len(), 4);
        assert_eq!(orderings.len(), 4);
    }

    #[test]
    fn deterministic_scheduler_can_run_non_send_fixtures() {
        let scheduler = Scheduler::new(1);
        let value = Rc::new(Cell::new(0_u32));
        let value_a = Rc::clone(&value);
        let scheduler_a = scheduler.clone();
        scheduler.spawn(async move {
            let current = value_a.get();
            scheduler_a.yield_now().await;
            value_a.set(current + 1);
        });
        let value_b = Rc::clone(&value);
        let scheduler_b = scheduler.clone();
        scheduler.spawn(async move {
            let current = value_b.get();
            scheduler_b.yield_now().await;
            value_b.set(current + 1);
        });

        let report = scheduler.run().expect("race fixture completes");
        assert_eq!(report.completed_tasks(), 2);
        assert!(matches!(value.get(), 1 | 2));
    }
}
