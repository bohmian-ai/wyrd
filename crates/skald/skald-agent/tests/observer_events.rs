//! Integration tests for observer event firing.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use skald_agent::Observer;

struct Capture(Arc<Mutex<Vec<String>>>);

impl Capture {
    fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
        let store = Arc::new(Mutex::new(Vec::new()));
        (Self(Arc::clone(&store)), store)
    }

    fn push(&self, event: String) {
        self.0.lock().expect("lock").push(event);
    }
}

#[async_trait]
impl Observer for Capture {
    async fn on_agent_start(
        &self,
        run_id: &str,
        _parent_run_id: Option<&str>,
        _agent_id: &str,
        _input: &str,
        _session_id: Option<&str>,
    ) {
        self.push(format!("start:{run_id}"));
    }

    async fn on_agent_finish(
        &self,
        run_id: &str,
        _agent_id: &str,
        _finish_reason: &str,
        _iterations: u32,
        _duration: Duration,
    ) {
        self.push(format!("finish:{run_id}"));
    }
}

#[test]
fn observer_trait_compiles_with_new_signature() {
    let _: Box<dyn Observer> = Box::new(skald_agent::NoopObserver);
}

#[test]
fn capture_observer_stores_events() {
    let (obs, store) = Capture::new();
    let run_id = "test-run-id";
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async move {
            obs.on_agent_start(run_id, None, "agent-1", "hello", None)
                .await;
            obs.on_agent_finish(
                run_id,
                "agent-1",
                "model_stopped",
                1,
                Duration::from_millis(100),
            )
            .await;
        });
    let events = store.lock().expect("lock");
    assert_eq!(events[0], format!("start:{run_id}"));
    assert_eq!(events[1], format!("finish:{run_id}"));
}

#[test]
fn composite_observer_fans_out() {
    use skald_observer::CompositeObserver;

    let (obs_a, store_a) = Capture::new();
    let (obs_b, store_b) = Capture::new();
    let composite = CompositeObserver::new(vec![Arc::new(obs_a), Arc::new(obs_b)]);
    let run_id = "fan-out-run";
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async move {
            composite.on_agent_start(run_id, None, "a", "x", None).await;
        });
    assert_eq!(store_a.lock().expect("lock").len(), 1);
    assert_eq!(store_b.lock().expect("lock").len(), 1);
}
