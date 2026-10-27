//! Formatted-log output that never blocks the thread that logs.
//!
//! The `fmt` layer formats each event on the emitting thread and hands the
//! line to a bounded queue; one dedicated thread writes queued lines to the
//! output. A slow or blocked stdout (a stalled disk behind a redirect, a
//! back-pressured log pipe) therefore never stalls a request or runtime
//! worker. When the queue is full, a new line is dropped and counted rather
//! than waited on.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};

/// Most formatted lines queued for the writer thread before new lines drop.
pub(crate) const QUEUED_LINES: usize = 128_000;

/// Owner of the writer thread: dropping it drains every line already queued
/// and joins the thread.
#[derive(Debug)]
pub(crate) struct LogWriter {
    /// Sender the drop uses to queue the stop marker after the last line.
    stop: SyncSender<Option<Vec<u8>>>,
    /// The writer thread, joined on drop.
    worker: Option<std::thread::JoinHandle<()>>,
    /// Lines dropped because the queue was full or the thread had stopped.
    dropped: Arc<AtomicU64>,
}

impl LogWriter {
    /// Starts the writer thread over `output` with a queue of `lines` lines
    /// and returns the writer the `fmt` layer formats into.
    ///
    /// # Errors
    ///
    /// Returns the operating system's error when the thread cannot spawn.
    pub(crate) fn start<W: Write + Send + 'static>(
        mut output: W,
        lines: usize,
    ) -> std::io::Result<(QueuedLines, Self)> {
        let (sender, receiver): (_, Receiver<Option<Vec<u8>>>) =
            std::sync::mpsc::sync_channel(lines);
        let worker = std::thread::Builder::new()
            .name("wyrd-log-writer".to_owned())
            .spawn(move || {
                // A failed write has nowhere to be reported but the output
                // that just failed, so it is skipped.
                while let Ok(Some(line)) = receiver.recv() {
                    let _ = output.write_all(&line);
                }
                let _ = output.flush();
            })?;
        let dropped = Arc::new(AtomicU64::new(0));
        Ok((
            QueuedLines {
                sender: sender.clone(),
                dropped: Arc::clone(&dropped),
            },
            Self {
                stop: sender,
                worker: Some(worker),
                dropped,
            },
        ))
    }

    /// Returns how many formatted lines were dropped so far.
    pub(crate) fn dropped_lines(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl Drop for LogWriter {
    /// Queues the stop marker behind every line already queued, then joins
    /// the writer thread once it has written them.
    fn drop(&mut self) {
        if self.stop.send(None).is_ok()
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }
}

/// The `fmt` layer's writer: queues each formatted line for the writer
/// thread without waiting.
#[derive(Debug, Clone)]
pub(crate) struct QueuedLines {
    /// Bounded queue to the writer thread.
    sender: SyncSender<Option<Vec<u8>>>,
    /// Shared with [`LogWriter`], which reports it.
    dropped: Arc<AtomicU64>,
}

impl Write for QueuedLines {
    /// Queues `buf`, one formatted event, or drops and counts it when the
    /// queue is full or the writer thread has stopped; never fails or waits.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.sender.try_send(Some(buf.to_vec())) {
            Ok(()) => {}
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(buf.len())
    }

    /// Has nothing of its own to flush; the writer thread flushes on stop.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for QueuedLines {
    type Writer = Self;

    /// Hands the `fmt` layer a handle onto the same queue.
    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};

    use super::LogWriter;

    /// A stdout stand-in that accepts nothing until its sender is dropped.
    struct Stalled(std::sync::mpsc::Receiver<()>);

    impl std::io::Write for Stalled {
        /// Blocks until released, then accepts the whole buffer.
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let _ = self.0.recv();
            Ok(buf.len())
        }

        /// Has nothing buffered to flush.
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// A stdout stand-in that records every byte it accepts.
    #[derive(Clone, Default)]
    struct Recorded(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Recorded {
        /// Appends the whole buffer.
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("recorded output")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        /// Has nothing buffered to flush.
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Logging onto a stdout that never accepts a byte returns at once and
    /// drops the lines past the queue instead of blocking the logging
    /// thread; a blocking writer would hang here on the first line.
    ///
    /// # Panics
    ///
    /// Panics when a write fails or no line is dropped.
    #[test]
    fn a_stalled_stdout_drops_lines_instead_of_blocking() {
        let (release, stalled) = std::sync::mpsc::channel();
        let (mut writer, log_writer) = LogWriter::start(Stalled(stalled), 4).expect("writer");
        for _ in 0..16 {
            writer
                .write_all(b"line\n")
                .expect("a queued write never fails");
        }
        assert!(log_writer.dropped_lines() > 0);
        drop(release);
    }

    /// Dropping the owner writes every line queued before it, in order.
    ///
    /// # Panics
    ///
    /// Panics when a queued line is lost or reordered.
    #[test]
    fn dropping_the_owner_drains_queued_lines() {
        let output = Recorded::default();
        let (mut writer, log_writer) = LogWriter::start(output.clone(), 8).expect("writer");
        for line in [b"one\n", b"two\n"] {
            writer.write_all(line).expect("queued");
        }
        drop(log_writer);
        assert_eq!(
            output.0.lock().expect("recorded output").as_slice(),
            b"one\ntwo\n"
        );
    }
}
