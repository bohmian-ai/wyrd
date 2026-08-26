//! Integration coverage for deterministic Bifrost interleavings.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wyrd_testing::interleaving::Scheduler;

#[test]
fn test_matrix_finds_deliberate_race() {
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let outcomes_for_factory = Rc::clone(&outcomes);
    let scheduler = Scheduler::new(42).with_max_permutations(32);
    let reports = scheduler
        .run_permutations(move |scheduler| {
            let counter = Rc::new(Cell::new(0_u32));
            let completed = Rc::new(Cell::new(0_u32));
            for _ in 0..2 {
                let counter = Rc::clone(&counter);
                let completed = Rc::clone(&completed);
                let outcomes = Rc::clone(&outcomes_for_factory);
                let task_scheduler = scheduler.clone();
                let future_scheduler = task_scheduler.clone();
                task_scheduler.spawn(async move {
                    let observed = counter.get();
                    future_scheduler.yield_now().await;
                    counter.set(observed + 1);
                    let done = completed.get() + 1;
                    completed.set(done);
                    if done == 2 {
                        outcomes.borrow_mut().push(counter.get());
                    }
                });
            }
        })
        .expect("race fixture schedules complete");

    assert!(!reports.is_empty());
    assert!(
        outcomes.borrow().contains(&1),
        "scheduler did not discover the lost-update interleaving: {:?}",
        outcomes.borrow()
    );
}
