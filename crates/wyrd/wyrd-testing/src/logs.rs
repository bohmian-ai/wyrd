//! Thread-default tracing capture for assertions over emitted log events.
//!
//! [`LogCapture`] installs a plain-text `fmt` subscriber as the calling
//! thread's default. A test on a current-thread Tokio runtime observes every
//! event its in-process server and spawned tasks emit, so it can assert a
//! structured event was written — or that a secret never was — without
//! replacing the process-global subscriber other tests share. A test whose
//! events come from threads it does not own, such as an in-process server's
//! shared runtime, installs it process-globally instead.

use std::io::{Result as IoResult, Write};
use std::sync::{Arc, Mutex};

use tracing::subscriber::{DefaultGuard, SetGlobalDefaultError};

/// Plain-text tracing output of the installing thread.
///
/// Captures events at `DEBUG` and above without ANSI escapes. Dropping a
/// thread-default capture restores the thread's previous default subscriber;
/// a process-global capture stays installed for the life of the process.
pub struct LogCapture {
    /// Bytes the subscriber wrote.
    sink: Arc<Mutex<Vec<u8>>>,
    /// Keeps a thread-default capture installed; `None` when process-global.
    _guard: Option<DefaultGuard>,
}

impl LogCapture {
    /// Install the capture as this thread's default subscriber.
    #[must_use]
    pub fn install() -> Self {
        let (sink, subscriber) = Self::subscriber();
        Self {
            sink,
            _guard: Some(tracing::subscriber::set_default(subscriber)),
        }
    }

    /// Install the capture as the process-global subscriber, so it observes
    /// every thread without its own default, including an in-process
    /// server's shared runtime. Install it before starting the server, whose
    /// telemetry then keeps this subscriber.
    ///
    /// # Errors
    /// Returns the refusal when the process already has a global subscriber.
    pub fn install_global() -> Result<Self, SetGlobalDefaultError> {
        let (sink, subscriber) = Self::subscriber();
        tracing::subscriber::set_global_default(subscriber)?;
        Ok(Self { sink, _guard: None })
    }

    /// A fresh sink and the plain-text subscriber writing into it.
    fn subscriber() -> (
        Arc<Mutex<Vec<u8>>>,
        impl tracing::Subscriber + Send + Sync + 'static,
    ) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::clone(&sink);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(move || CaptureWriter(Arc::clone(&writer)))
            .finish();
        (sink, subscriber)
    }

    /// Everything captured so far, lossily decoded as UTF-8.
    ///
    /// # Panics
    /// Panics when the sink lock was poisoned, which no writer can cause
    /// because a write never panics while holding it.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(
            &self
                .sink
                .lock()
                .expect("invariant: the log sink lock is never held across a panic"),
        )
        .into_owned()
    }
}

/// One subscriber write handle appending to a [`LogCapture`] sink.
struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    /// Append `buf` to the shared sink.
    ///
    /// # Errors
    /// Never fails; a poisoned sink drops the write rather than failing the
    /// subscriber the test observes through.
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        if let Ok(mut sink) = self.0.lock() {
            sink.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    /// Nothing is buffered beyond the sink, so flushing is a no-op.
    ///
    /// # Errors
    /// Never fails.
    fn flush(&mut self) -> IoResult<()> {
        Ok(())
    }
}
