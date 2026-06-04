use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use async_trait::async_trait;
use skald_agent::observer::Observer;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static GLOBAL_RECORDER: OnceLock<Arc<RecordingObserver>> = OnceLock::new();

#[tokio::test]
async fn with_observer_overrides_in_current_task() {
    let _guard = TEST_LOCK.lock().await;
    let global = global_recorder();
    let scoped = Arc::new(RecordingObserver::default());

    wyrd_observe::with_observer(scoped.clone(), async {
        wyrd_observe::current()
            .on_agent_start("run-id", None, "agent-id", "input", None)
            .await;
    })
    .await;

    assert_eq!(scoped.count(), 1);
    assert_eq!(global.count(), 0);
}

#[tokio::test]
async fn with_observer_restores_on_exit() {
    let _guard = TEST_LOCK.lock().await;
    let global = global_recorder();
    let scoped = Arc::new(RecordingObserver::default());

    wyrd_observe::with_observer(scoped.clone(), async {}).await;
    wyrd_observe::current()
        .on_agent_start("run-id", None, "agent-id", "input", None)
        .await;

    assert_eq!(scoped.count(), 0);
    assert_eq!(global.count(), 1);
}

#[tokio::test]
async fn with_observer_does_not_propagate_to_detached_spawn() {
    let _guard = TEST_LOCK.lock().await;
    let global = global_recorder();
    let scoped = Arc::new(RecordingObserver::default());

    wyrd_observe::with_observer(scoped.clone(), async {
        tokio::spawn(async {
            wyrd_observe::current()
                .on_agent_start("run-id", None, "agent-id", "input", None)
                .await;
        })
        .await
        .expect("spawned observer task joins");
    })
    .await;

    assert_eq!(scoped.count(), 0);
    assert_eq!(global.count(), 1);
}

fn global_recorder() -> Arc<RecordingObserver> {
    let recorder = GLOBAL_RECORDER
        .get_or_init(|| {
            let recorder = Arc::new(RecordingObserver::default());
            let observer: Arc<dyn Observer> = recorder.clone();
            wyrd_observe::set_global(observer);
            recorder
        })
        .clone();
    recorder.clear();
    recorder
}

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<String>>,
}

impl RecordingObserver {
    fn count(&self) -> usize {
        self.lock().len()
    }

    fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> MutexGuard<'_, Vec<String>> {
        match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl Observer for RecordingObserver {
    async fn on_agent_start(
        &self,
        _run_id: &str,
        _parent_run_id: Option<&str>,
        agent_id: &str,
        input: &str,
        session_id: Option<&str>,
    ) {
        self.lock()
            .push(format!("{agent_id}:{input}:{}", session_id.unwrap_or("")));
    }
}
