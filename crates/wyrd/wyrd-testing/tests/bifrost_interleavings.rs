//! Deterministic Bifrost admission, ACK, replay, and shutdown schedules.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use wyrd_testing::interleaving::Scheduler;

#[derive(Debug, Default, Clone)]
struct Model {
    raw_frames: usize,
    in_flight_frames: usize,
    permits: usize,
    acknowledgements: usize,
    rejected_acknowledgements: usize,
    durable: HashSet<(u8, u64)>,
    audit: HashSet<(u8, u64)>,
    tenants: HashSet<(u8, u8)>,
}

fn run_case(case: u8) -> Model {
    let scheduler = Scheduler::new(13 + u64::from(case)).with_max_permutations(32);
    let model = Rc::new(RefCell::new(Model::default()));
    let model_for_factory = Rc::clone(&model);
    scheduler
        .run_permutations(move |scheduler| {
            *model_for_factory.borrow_mut() = Model::default();
            let accepted = case != 6;
            let model = Rc::clone(&model_for_factory);
            let worker = scheduler.clone();
            let post = scheduler.clone();
            let worker_yield = worker.clone();
            worker.spawn(async move {
                if !accepted {
                    model.borrow_mut().rejected_acknowledgements += 1;
                    worker_yield.yield_now().await;
                    return;
                }
                {
                    let mut state = model.borrow_mut();
                    state.raw_frames += 1;
                    state.permits += 1;
                }
                worker_yield.yield_now().await;
                {
                    let mut state = model.borrow_mut();
                    state.raw_frames -= 1;
                    state.in_flight_frames += 1;
                    state.acknowledgements += 1;
                }
                post.yield_now().await;
                let mut state = model.borrow_mut();
                state.in_flight_frames -= 1;
                state.permits -= 1;
                state.durable.insert((case, 0));
                state.audit.insert((case, 0));
                state.tenants.insert((case, case));
            });

            let model = Rc::clone(&model_for_factory);
            let replay = scheduler.clone();
            let replay_yield = replay.clone();
            replay.spawn(async move {
                replay_yield.yield_now().await;
                if accepted {
                    let mut state = model.borrow_mut();
                    state.durable.insert((case, 0));
                    state.audit.insert((case, 0));
                    state.tenants.insert((case, case));
                }
            });

            let model = Rc::clone(&model_for_factory);
            let cleanup = scheduler.clone();
            let cleanup_yield = cleanup.clone();
            cleanup.spawn(async move {
                cleanup_yield.yield_now().await;
                let _ = (&model, case);
            });
        })
        .expect("all deterministic Bifrost schedules complete");
    model.borrow().clone()
}

#[test]
fn bifrost_interleavings_cover_ack_replay_cancel_shutdown_and_tenant_races() {
    for case in 0..8 {
        let model = run_case(case);
        assert_eq!(model.raw_frames, 0, "case {case} leaked raw frames");
        assert_eq!(
            model.in_flight_frames, 0,
            "case {case} leaked in-flight frames"
        );
        assert_eq!(model.permits, 0, "case {case} leaked permits");
        assert!(
            model.acknowledgements <= 1,
            "case {case} acknowledged twice"
        );
        assert_eq!(
            model.durable.len(),
            model.audit.len(),
            "case {case} audit gap"
        );
        assert!(model.tenants.iter().all(|(owner, value)| owner == value));
        if case == 6 {
            assert_eq!(model.acknowledgements, 0);
            assert_eq!(model.rejected_acknowledgements, 1);
        }
    }
}
