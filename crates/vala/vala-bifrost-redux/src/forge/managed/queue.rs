//! Worker-local FIFO admission queue for planned compaction runners.
//!
redacted
//! `origin/main` `6f8fbbfd06d25d195bdff9a4f1cb246cf4363903`, file
//! `src/storage/src/hummock/compactor/iceberg_compaction/mod.rs`
//! (lines 40-385). Licensed Apache-2.0, Copyright `RisingWave` Labs.
//!
//! The accounting rules are upstream's exactly, because they are the reason a
//! worker cannot over-commit itself: waiting plans charge parallelism only,
//! running plans charge parallelism *and* estimated memory, the head of the
//! queue is the only candidate ever considered, and a plan that cannot fit the
//! worker at all is refused rather than parked forever.
//!
//! Two adaptations are Wyrd-local and deliberate. The key is
//! `(TaskId, plan_index)` where upstream uses its own task id newtype, and the
//! runner payload is a type parameter because the runner belongs to
//! [`ForgeWorker`](crate::forge::worker::ForgeWorker), not to the accounting.
//! Upstream's `Notify` is not ported: Wyrd has exactly one event loop, which
//! pops after every push and every completion, so a second wake-up channel
//! would have no reader.

use std::collections::{HashMap, VecDeque};

use uuid::Uuid;

/// Queue key for one plan of one durable task.
///
/// One claimed task fans out into several independent plans, so the task id
/// alone cannot identify an admission. `plan_index` is the planner's ordinal
/// and is process-local: it is never durable, and it never appears in an
/// operation row.
pub(crate) type ForgePlanKey = (Uuid, usize);

/// Admission terms for one planned rewrite.
///
/// Both figures are validated by [`ForgeCompactionQueue::push`] rather than by
/// the producer, so an implausible estimate is refused at one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForgePlanAdmission {
    /// Durable task this plan belongs to.
    pub task_id: Uuid,
    /// Planner ordinal of this plan within the attempt.
    pub plan_index: usize,
    /// Execution parallelism the plan recommends; must be `1..=max_parallelism`.
    pub required_parallelism: u32,
    /// Estimated heap peak charged while the plan runs; must be
    /// `1..=total_memory_budget_bytes`.
    pub memory_reservation_bytes: usize,
}

impl ForgePlanAdmission {
    /// Returns this admission's queue key.
    fn key(&self) -> ForgePlanKey {
        (self.task_id, self.plan_index)
    }
}

/// One popped plan and the runner payload the caller attached to it.
#[derive(Debug)]
pub(crate) struct PoppedForgePlan<R> {
    /// Admission terms now charged against the running budgets.
    pub admission: ForgePlanAdmission,
    /// Runner payload, present when the caller attached one at push time.
    pub runner: Option<R>,
}

/// Resources one admitted plan holds while it waits and while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForgePlanResources {
    /// Parallelism units charged to waiting, then to running.
    required_parallelism: u32,
    /// Estimated heap peak charged only once the plan is running.
    memory_reservation_bytes: usize,
}

/// Why [`ForgeCompactionQueue::push`] refused, or that it accepted.
///
/// The distinction between the three refusals matters to settlement: capacity
/// is a retryable worker-local condition that consumes no attempt, while an
/// invalid parallelism or a duplicate key is an internal invariant violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForgePushResult {
    /// The plan is queued in FIFO position.
    Added,
    /// Admitting it would exceed the waiting-parallelism budget.
    RejectedCapacity,
    /// Its parallelism or estimated memory exceeds the whole worker.
    RejectedTooLarge,
    /// Its `required_parallelism` is zero.
    RejectedInvalidParallelism,
    /// A plan with the same `(task_id, plan_index)` is already tracked.
    RejectedDuplicate,
}

/// Strict-FIFO admission queue bounded by parallelism and running memory.
///
/// Invariants the type maintains, and which its callers rely on:
///
/// * every waiting plan's parallelism is in `waiting_parallelism_sum`, and
///   nothing else is;
/// * every running plan's parallelism and memory are in the running sums, and
///   nothing else is — a waiting plan's memory is deliberately uncharged, so a
///   large queued plan cannot starve smaller running ones;
/// * `resource_map` holds exactly the keys that are waiting or running, which
///   is what makes a duplicate push and a double finish both detectable;
/// * only the head is ever examined by [`Self::pop`], so admission order is
///   submission order and no plan is skipped for being small.
pub(crate) struct ForgeCompactionQueue<R> {
    /// FIFO of waiting admissions.
    deque: VecDeque<ForgePlanAdmission>,
    /// Resources of every waiting or running plan, keyed by plan key.
    resource_map: HashMap<ForgePlanKey, ForgePlanResources>,
    /// Runner payloads of waiting plans, removed on pop or cancel.
    runners: HashMap<ForgePlanKey, R>,
    /// Sum of `required_parallelism` over waiting plans.
    waiting_parallelism_sum: u32,
    /// Sum of `required_parallelism` over running plans.
    running_parallelism_sum: u32,
    /// Sum of `memory_reservation_bytes` over running plans.
    running_memory_reservation_bytes: usize,
    /// Maximum concurrent parallelism across running plans.
    max_parallelism: u32,
    /// Maximum total parallelism across waiting plans.
    pending_parallelism_budget: u32,
    /// Immutable worker memory budget running plans are charged against.
    total_memory_budget_bytes: usize,
}

impl<R> std::fmt::Debug for ForgeCompactionQueue<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgeCompactionQueue")
            .field("waiting", &self.deque.len())
            .field("waiting_parallelism_sum", &self.waiting_parallelism_sum)
            .field("running_parallelism_sum", &self.running_parallelism_sum)
            .field(
                "running_memory_reservation_bytes",
                &self.running_memory_reservation_bytes,
            )
            .field("max_parallelism", &self.max_parallelism)
            .field(
                "pending_parallelism_budget",
                &self.pending_parallelism_budget,
            )
            .field("total_memory_budget_bytes", &self.total_memory_budget_bytes)
            .field("resource_map", &self.resource_map.len())
            .field("runners", &self.runners.len())
            .finish_non_exhaustive()
    }
}

impl<R> ForgeCompactionQueue<R> {
    /// Creates an empty queue with the worker's immutable budgets.
    ///
    /// # Panics
    ///
    /// Panics when `max_parallelism` is zero, when
    /// `pending_parallelism_budget` cannot hold one maximally parallel plan, or
    /// when `total_memory_budget_bytes` is zero. Each is a composition
    /// invariant resolved once at boot, so a violation is a programming error
    /// in resource planning rather than a runtime condition.
    pub(crate) fn new(
        max_parallelism: u32,
        pending_parallelism_budget: u32,
        total_memory_budget_bytes: usize,
    ) -> Self {
        assert!(max_parallelism > 0, "max_parallelism must be > 0");
        assert!(
            pending_parallelism_budget >= max_parallelism,
            "pending budget should allow at least one task"
        );
        assert!(
            total_memory_budget_bytes > 0,
            "total memory budget must be > 0"
        );
        Self {
            deque: VecDeque::new(),
            resource_map: HashMap::new(),
            runners: HashMap::new(),
            waiting_parallelism_sum: 0,
            running_parallelism_sum: 0,
            running_memory_reservation_bytes: 0,
            max_parallelism,
            pending_parallelism_budget,
            total_memory_budget_bytes,
        }
    }

    /// Returns the parallelism currently charged to running plans.
    pub(crate) fn running_parallelism_sum(&self) -> u32 {
        self.running_parallelism_sum
    }

    /// Returns the parallelism currently charged to waiting plans.
    pub(crate) fn waiting_parallelism_sum(&self) -> u32 {
        self.waiting_parallelism_sum
    }

    /// Returns the estimated memory currently charged to running plans.
    pub(crate) fn running_memory_reservation_bytes(&self) -> usize {
        self.running_memory_reservation_bytes
    }

    /// Returns parallelism available to a plan that would start now.
    fn available_parallelism(&self) -> u32 {
        self.max_parallelism
            .saturating_sub(self.running_parallelism_sum)
    }

    /// Returns estimated memory available to a plan that would start now.
    fn available_memory_reservation_bytes(&self) -> usize {
        self.total_memory_budget_bytes
            .saturating_sub(self.running_memory_reservation_bytes)
    }

    /// Appends one plan to the FIFO, or refuses it with a typed reason.
    ///
    /// Refusals never mutate accounting, so a caller that offers several plans
    /// keeps every earlier admission when a later one is refused. The queue
    /// neither reorders nor merges: a plan that does not fit the head position
    /// waits behind the plans offered before it.
    pub(crate) fn push(&mut self, admission: ForgePlanAdmission, runner: R) -> ForgePushResult {
        if admission.required_parallelism == 0 {
            return ForgePushResult::RejectedInvalidParallelism;
        }
        if admission.required_parallelism > self.max_parallelism
            || admission.memory_reservation_bytes > self.total_memory_budget_bytes
        {
            return ForgePushResult::RejectedTooLarge;
        }
        let key = admission.key();
        if self.resource_map.contains_key(&key) {
            return ForgePushResult::RejectedDuplicate;
        }
        let Some(new_parallelism_total) = self
            .waiting_parallelism_sum
            .checked_add(admission.required_parallelism)
        else {
            return ForgePushResult::RejectedCapacity;
        };
        if new_parallelism_total > self.pending_parallelism_budget {
            return ForgePushResult::RejectedCapacity;
        }
        self.resource_map.insert(
            key,
            ForgePlanResources {
                required_parallelism: admission.required_parallelism,
                memory_reservation_bytes: admission.memory_reservation_bytes,
            },
        );
        self.waiting_parallelism_sum = new_parallelism_total;
        self.runners.insert(key, runner);
        self.deque.push_back(admission);
        ForgePushResult::Added
    }

    /// Moves the head to running when both running budgets admit it.
    ///
    /// Returns `None` when the queue is empty or the head does not fit. Only
    /// the head is ever considered, which is what makes head-of-line blocking
    /// the queue's advertised behavior rather than an accident.
    ///
    /// # Panics
    ///
    /// Panics when the running memory sum would overflow `usize`, which cannot
    /// happen while every admission is bounded by the worker budget.
    pub(crate) fn pop(&mut self) -> Option<PoppedForgePlan<R>> {
        let front = self.deque.front()?;
        if front.required_parallelism > self.available_parallelism() {
            return None;
        }
        if front.memory_reservation_bytes > self.available_memory_reservation_bytes() {
            return None;
        }
        let admission = self.deque.pop_front()?;
        self.waiting_parallelism_sum = self
            .waiting_parallelism_sum
            .saturating_sub(admission.required_parallelism);
        self.running_parallelism_sum = self
            .running_parallelism_sum
            .saturating_add(admission.required_parallelism);
        self.running_memory_reservation_bytes = self
            .running_memory_reservation_bytes
            .checked_add(admission.memory_reservation_bytes)
            .expect("running memory reservation sum overflowed");
        let runner = self.runners.remove(&admission.key());
        Some(PoppedForgePlan { admission, runner })
    }

    /// Releases one running plan's exact accounting.
    ///
    /// Returns `false` for an unknown key, which is how a double finish is
    /// detected rather than silently double-crediting the budgets.
    ///
    /// # Panics
    ///
    /// Panics when the running memory sum would underflow, which would mean a
    /// plan was finished with resources it never held.
    pub(crate) fn finish_running(&mut self, key: ForgePlanKey) -> bool {
        let Some(resources) = self.resource_map.remove(&key) else {
            tracing::warn!(
                task_id = %key.0,
                plan_index = key.1,
                "Forge compaction queue finish for an unknown plan key"
            );
            return false;
        };
        self.running_parallelism_sum = self
            .running_parallelism_sum
            .saturating_sub(resources.required_parallelism);
        self.running_memory_reservation_bytes = self
            .running_memory_reservation_bytes
            .checked_sub(resources.memory_reservation_bytes)
            .expect("running memory reservation bookkeeping underflowed");
        self.runners.remove(&key);
        true
    }

    /// Removes every still-waiting plan of one task and returns how many.
    ///
    /// Running siblings are untouched: they hold real resources and must drain
    /// through their own completion, so cancellation signals them rather than
    /// forgetting them.
    pub(crate) fn cancel_waiting_task(&mut self, task_id: Uuid) -> usize {
        let mut retained = VecDeque::with_capacity(self.deque.len());
        let mut cancelled_parallelism = 0;
        let mut cancelled_count = 0;
        while let Some(admission) = self.deque.pop_front() {
            if admission.task_id == task_id {
                cancelled_parallelism += admission.required_parallelism;
                cancelled_count += 1;
                self.resource_map.remove(&admission.key());
                self.runners.remove(&admission.key());
            } else {
                retained.push_back(admission);
            }
        }
        self.deque = retained;
        self.waiting_parallelism_sum = self
            .waiting_parallelism_sum
            .saturating_sub(cancelled_parallelism);
        cancelled_count
    }
}

#[cfg(test)]
mod tests {
    use super::{ForgeCompactionQueue, ForgePlanAdmission, ForgePushResult};
    use uuid::Uuid;

    /// Builds a stable task UUID from a small ordinal so assertions can name
    /// tasks by number without depending on random identity.
    fn task(ordinal: u8) -> Uuid {
        let mut bytes = [0_u8; 16];
        bytes[15] = ordinal;
        Uuid::from_bytes(bytes)
    }

    /// Builds an admission with a nominal one-byte memory reservation, so
    /// parallelism assertions are not perturbed by the memory budget.
    fn admission(ordinal: u8, plan_index: usize, parallelism: u32) -> ForgePlanAdmission {
        ForgePlanAdmission {
            task_id: task(ordinal),
            plan_index,
            required_parallelism: parallelism,
            memory_reservation_bytes: 1,
        }
    }

    /// Builds an admission that charges an exact memory reservation.
    fn sized(ordinal: u8, memory_reservation_bytes: usize) -> ForgePlanAdmission {
        ForgePlanAdmission {
            task_id: task(ordinal),
            plan_index: 0,
            required_parallelism: 1,
            memory_reservation_bytes,
        }
    }

redacted
    ///
    /// Every transition upstream's own tests pin is asserted here against the
    /// same expected values, because the queue's only justification is that a
redacted
    /// is synchronous and has no sleeps: admission is pure accounting, so any
    /// need to wait would itself be a defect.
    ///
    /// # Panics
    ///
    /// Panics when any admission, refusal, pop, finish, or cancellation
    /// deviates from the ported state machine.
    #[test]
redacted
        // Basic push, pop, finish.
        let mut queue = ForgeCompactionQueue::new(8, 32, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 4), ()), ForgePushResult::Added);
        assert_eq!(queue.waiting_parallelism_sum(), 4);
        let popped = queue.pop().expect("the head fits both running budgets");
        assert_eq!(popped.admission.task_id, task(1));
        assert_eq!(popped.runner, Some(()));
        assert_eq!(queue.waiting_parallelism_sum(), 0);
        assert_eq!(queue.running_parallelism_sum(), 4);
        assert!(queue.finish_running((task(1), 0)));
        assert_eq!(queue.running_parallelism_sum(), 0);
        // A second finish for the same key is refused rather than double-credited.
        assert!(!queue.finish_running((task(1), 0)));
        assert_eq!(queue.running_parallelism_sum(), 0);
        // An unknown key is refused the same way.
        assert!(!queue.finish_running((task(99), 0)));

        // Strict FIFO across tasks.
        let mut queue = ForgeCompactionQueue::new(8, 32, usize::MAX);
        for ordinal in 1..=3 {
            assert_eq!(
                queue.push(admission(ordinal, 0, 2), ()),
                ForgePushResult::Added
            );
        }
        for ordinal in 1..=3 {
            assert_eq!(
                queue.pop().expect("every plan fits").admission.task_id,
                task(ordinal)
            );
        }

        // Pending-parallelism capacity refuses without disturbing accounting.
        let mut queue = ForgeCompactionQueue::new(4, 6, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 3), ()), ForgePushResult::Added);
        assert_eq!(queue.push(admission(2, 0, 3), ()), ForgePushResult::Added);
        assert_eq!(
            queue.push(admission(3, 0, 1), ()),
            ForgePushResult::RejectedCapacity
        );
        assert_eq!(queue.waiting_parallelism_sum(), 6);

        // Zero parallelism, over-large parallelism, and over-large memory.
        let mut queue = ForgeCompactionQueue::new(4, 10, 100);
        assert_eq!(
            queue.push(admission(1, 0, 0), ()),
            ForgePushResult::RejectedInvalidParallelism
        );
        assert_eq!(
            queue.push(admission(2, 0, 5), ()),
            ForgePushResult::RejectedTooLarge
        );
        assert_eq!(
            queue.push(sized(3, 101), ()),
            ForgePushResult::RejectedTooLarge
        );
        assert_eq!(queue.waiting_parallelism_sum(), 0);

        // Duplicate keys refuse; a different plan index of the same task does not.
        let mut queue = ForgeCompactionQueue::new(8, 32, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 3), ()), ForgePushResult::Added);
        assert_eq!(
            queue.push(admission(1, 0, 5), ()),
            ForgePushResult::RejectedDuplicate
        );
        assert_eq!(queue.waiting_parallelism_sum(), 3);
        assert_eq!(queue.push(admission(1, 1, 2), ()), ForgePushResult::Added);
        assert_eq!(queue.waiting_parallelism_sum(), 5);
        let popped = queue.pop().expect("the head fits");
        assert_eq!(
            (popped.admission.task_id, popped.admission.plan_index),
            (task(1), 0)
        );
        assert!(queue.finish_running((task(1), 0)));
        assert_eq!(queue.push(admission(1, 0, 4), ()), ForgePushResult::Added);
    }

    /// Budget pressure blocks the head rather than letting a smaller plan pass.
    ///
    /// Running parallelism, a whole-worker plan, out-of-order finishes across
    /// one task's plans, cancellation of only the waiting siblings, and the
    /// deliberate exemption of waiting memory from the running bound are all
    /// admission-order properties: none of them may reorder the queue.
    ///
    /// # Panics
    ///
    /// Panics when any pop, finish, or cancellation deviates from the ported
    /// state machine.
    #[test]
    fn queue_capacity_pressure_never_reorders_admission() {
        // Head-of-line blocking on running parallelism.
        let mut queue = ForgeCompactionQueue::new(8, 32, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 6), ()), ForgePushResult::Added);
        assert_eq!(queue.push(admission(2, 0, 4), ()), ForgePushResult::Added);
        assert_eq!(queue.pop().expect("head fits").admission.task_id, task(1));
        assert!(queue.pop().is_none(), "only two parallelism units remain");
        assert!(queue.finish_running((task(1), 0)));
        assert_eq!(
            queue.pop().expect("head now fits").admission.task_id,
            task(2)
        );

        // A plan sized to the whole worker is admitted, and blocks the rest.
        let mut queue = ForgeCompactionQueue::new(4, 4, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 4), ()), ForgePushResult::Added);
        assert_eq!(
            queue.push(admission(2, 0, 1), ()),
            ForgePushResult::RejectedCapacity
        );
        assert_eq!(
            queue
                .pop()
                .expect("head fits")
                .admission
                .required_parallelism,
            4
        );
        assert_eq!(queue.push(admission(3, 0, 1), ()), ForgePushResult::Added);
        assert!(queue.pop().is_none(), "no running parallelism remains");

        // Several plans of one task are independent and finish out of order.
        let mut queue = ForgeCompactionQueue::new(10, 30, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 3), ()), ForgePushResult::Added);
        assert_eq!(queue.push(admission(1, 1, 4), ()), ForgePushResult::Added);
        assert_eq!(queue.push(admission(1, 2, 2), ()), ForgePushResult::Added);
        assert_eq!(queue.waiting_parallelism_sum(), 9);
        for plan_index in 0..3 {
            let popped = queue.pop().expect("every plan fits");
            assert_eq!(popped.admission.task_id, task(1));
            assert_eq!(popped.admission.plan_index, plan_index);
        }
        assert_eq!(queue.running_parallelism_sum(), 9);
        assert!(queue.finish_running((task(1), 1)));
        assert_eq!(queue.running_parallelism_sum(), 5);
        assert!(queue.finish_running((task(1), 0)));
        assert!(queue.finish_running((task(1), 2)));
        assert_eq!(queue.running_parallelism_sum(), 0);

        // Cancellation removes only waiting siblings.
        let mut queue = ForgeCompactionQueue::new(10, 30, usize::MAX);
        assert_eq!(queue.push(admission(1, 0, 3), ()), ForgePushResult::Added);
        assert_eq!(queue.push(admission(1, 1, 4), ()), ForgePushResult::Added);
        assert_eq!(queue.push(admission(2, 0, 2), ()), ForgePushResult::Added);
        let popped = queue.pop().expect("head fits");
        assert_eq!(
            (popped.admission.task_id, popped.admission.plan_index),
            (task(1), 0)
        );
        assert_eq!(queue.cancel_waiting_task(task(1)), 1);
        assert_eq!(queue.running_parallelism_sum(), 3);
        assert_eq!(queue.waiting_parallelism_sum(), 2);
        assert!(queue.finish_running((task(1), 0)));
        assert_eq!(queue.pop().expect("head fits").admission.task_id, task(2));

        // Waiting memory is uncharged; only running memory bounds the head.
        let mut queue = ForgeCompactionQueue::new(8, 32, 100);
        assert_eq!(queue.push(sized(1, 80), ()), ForgePushResult::Added);
        queue.pop().expect("the first plan fits the empty budget");
        assert_eq!(queue.running_memory_reservation_bytes(), 80);
        assert_eq!(
            queue.push(sized(2, 60), ()),
            ForgePushResult::Added,
            "a waiting plan is not charged memory, so it is admitted"
        );
        assert!(queue.pop().is_none(), "but it cannot run beside the first");
        assert!(queue.finish_running((task(1), 0)));
        assert_eq!(
            queue.pop().expect("memory freed").admission.task_id,
            task(2)
        );

        // Memory head-of-line blocking preserves FIFO rather than skipping ahead.
        let mut queue = ForgeCompactionQueue::new(8, 32, 100);
        assert_eq!(queue.push(sized(1, 60), ()), ForgePushResult::Added);
        queue.pop().expect("the first plan fits");
        assert_eq!(queue.push(sized(2, 50), ()), ForgePushResult::Added);
        assert_eq!(queue.push(sized(3, 40), ()), ForgePushResult::Added);
        assert!(
            queue.pop().is_none(),
            "the smaller third plan must not bypass the blocked head"
        );
        assert!(queue.finish_running((task(1), 0)));
        assert_eq!(queue.pop().expect("head fits").admission.task_id, task(2));
        assert_eq!(queue.pop().expect("tail fits").admission.task_id, task(3));

        // An empty queue is inert.
        let mut queue: ForgeCompactionQueue<()> = ForgeCompactionQueue::new(8, 32, usize::MAX);
        assert!(queue.pop().is_none());
        assert!(!queue.finish_running((task(1), 0)));
        assert_eq!(queue.waiting_parallelism_sum(), 0);
        assert_eq!(queue.running_parallelism_sum(), 0);
        assert_eq!(queue.cancel_waiting_task(task(1)), 0);
    }
}
