//! Interactive terminal demo for Card declaration progress rendering.

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use wyrd_cli::declaration::DeclarationProgressRenderer;
use wyrd_client::cards::{DeclarationPhase, DeclarationProgressEvent, DeclarationProgressSink};
use wyrd_spec::registry::RelativeArtifactPath;

fn main() {
    let should_fail = std::env::args().any(|argument| argument == "--fail");
    let renderer = DeclarationProgressRenderer::with_interactive(true);
    let sink = renderer.sink();

    sink(DeclarationProgressEvent::Phase(DeclarationPhase::Preparing));
    thread::sleep(Duration::from_millis(500));
    sink(DeclarationProgressEvent::Phase(
        DeclarationPhase::Submitting,
    ));
    thread::sleep(Duration::from_millis(700));
    sink(DeclarationProgressEvent::Phase(DeclarationPhase::Uploading));

    let semaphore = Arc::new(DemoSemaphore::new(4));
    let mut workers = Vec::new();
    for index in 1..=5 {
        let sink = Arc::clone(&sink);
        let semaphore = Arc::clone(&semaphore);
        workers.push(thread::spawn(move || {
            let _permit = semaphore.acquire();
            simulate_upload(sink, index, should_fail && index == 3)
        }));
    }

    let mut succeeded = true;
    for worker in workers {
        if !worker
            .join()
            .expect("invariant: demo worker must not panic")
        {
            succeeded = false;
        }
    }

    if succeeded {
        sink(DeclarationProgressEvent::Phase(
            DeclarationPhase::Completing,
        ));
        thread::sleep(Duration::from_millis(700));
        sink(DeclarationProgressEvent::Phase(DeclarationPhase::Verifying));
        thread::sleep(Duration::from_millis(500));
    } else {
        sink(DeclarationProgressEvent::Phase(
            DeclarationPhase::CleaningUp,
        ));
        thread::sleep(Duration::from_millis(900));
    }

    renderer.finish_and_clear();
    if succeeded {
        eprintln!("registration demo complete");
    } else {
        eprintln!("registration demo failed and cleaned up");
    }
}

fn simulate_upload(sink: DeclarationProgressSink, index: usize, should_fail: bool) -> bool {
    let artifact = RelativeArtifactPath::new(&format!("artifact-{index}.bin"))
        .expect("invariant: demo artifact path is valid");
    let total_bytes = 8 * 1024 * 1024;
    let chunk_bytes = total_bytes / 16;

    sink(DeclarationProgressEvent::ArtifactStarted {
        artifact: artifact.clone(),
        total_bytes: Some(total_bytes),
    });

    for chunk in 1..=16 {
        thread::sleep(Duration::from_millis(90 + (index as u64 * 25)));
        let uploaded_bytes = chunk * chunk_bytes;
        sink(DeclarationProgressEvent::ArtifactProgress {
            artifact: artifact.clone(),
            uploaded_bytes,
            total_bytes: Some(total_bytes),
        });
        if should_fail && chunk == 8 {
            sink(DeclarationProgressEvent::ArtifactFinished {
                artifact,
                success: false,
            });
            return false;
        }
    }

    sink(DeclarationProgressEvent::ArtifactFinished {
        artifact,
        success: true,
    });
    true
}

struct DemoSemaphore {
    available: Mutex<usize>,
    wakeup: Condvar,
}

impl DemoSemaphore {
    fn new(capacity: usize) -> Self {
        Self {
            available: Mutex::new(capacity),
            wakeup: Condvar::new(),
        }
    }

    fn acquire(self: &Arc<Self>) -> DemoPermit {
        let mut available = self
            .available
            .lock()
            .expect("invariant: demo semaphore mutex is not poisoned");
        while *available == 0 {
            available = self
                .wakeup
                .wait(available)
                .expect("invariant: demo semaphore mutex is not poisoned");
        }
        *available -= 1;
        DemoPermit {
            semaphore: Arc::clone(self),
        }
    }
}

struct DemoPermit {
    semaphore: Arc<DemoSemaphore>,
}

impl Drop for DemoPermit {
    fn drop(&mut self) {
        let mut available = self
            .semaphore
            .available
            .lock()
            .expect("invariant: demo semaphore mutex is not poisoned");
        *available += 1;
        self.semaphore.wakeup.notify_one();
    }
}
