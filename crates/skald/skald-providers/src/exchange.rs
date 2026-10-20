//! Transient monotonic timing of provider HTTP exchanges.
//!
//! A caller that must separate time spent waiting on a provider from its own
//! preparation and decode work runs its provider calls inside [`scope`]. Every
//! HTTP exchange this crate sends inside the scope is logged as one
//! [`Exchange`]: when the request was sent, when its response headers
//! arrived, and when its body was fully read or the exchange failed. A
//! streamed answer whose body is read elsewhere is logged open, with no end.
//! The caller drains the log with [`take`]. Outside a scope nothing is
//! recorded, so ordinary callers pay only a failed task-local lookup.
//!
//! The log is per task: an exchange sent by a task spawned inside the scope
//! is not recorded. Timings are process-local monotonic instants, never
//! persisted or sent on the wire.

use std::cell::RefCell;
use std::future::Future;
use std::time::Instant;

tokio::task_local! {
    /// Exchanges sent by the current task inside a [`scope`], in send order.
    static EXCHANGES: RefCell<Vec<Exchange>>;
}

/// One provider HTTP exchange's monotonic timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exchange {
    /// When the request was handed to the HTTP client.
    pub sent: Instant,
    /// When the response headers arrived; `None` when the exchange failed
    /// before any response.
    pub headers: Option<Instant>,
    /// When the body was fully read or the exchange failed; `None` for a
    /// streamed answer left open for incremental reading.
    pub ended: Option<Instant>,
}

/// Runs `future` with an empty exchange log for its task.
///
/// Scopes nest: an inner scope shadows the outer one until it ends.
pub async fn scope<F: Future>(future: F) -> F::Output {
    EXCHANGES.scope(RefCell::new(Vec::new()), future).await
}

/// Drains the current scope's logged exchanges in send order; empty outside
/// a scope.
#[must_use]
pub fn take() -> Vec<Exchange> {
    EXCHANGES
        .try_with(|log| std::mem::take(&mut *log.borrow_mut()))
        .unwrap_or_default()
}

/// Times one exchange from its send and logs it when dropped.
///
/// Dropping it, including on an early error return or cancellation, logs the
/// exchange as ended at that moment; [`Self::open`] logs it without an end
/// for a body read elsewhere.
pub(crate) struct ExchangeTimer {
    /// The exchange so far.
    exchange: Exchange,
    /// Whether the body is read elsewhere, so the drop logs no end.
    open: bool,
}

impl ExchangeTimer {
    /// Starts timing an exchange sent now.
    pub(crate) fn start() -> Self {
        Self {
            exchange: Exchange {
                sent: Instant::now(),
                headers: None,
                ended: None,
            },
            open: false,
        }
    }

    /// Marks the response headers as arrived now.
    pub(crate) fn headers(&mut self) {
        self.exchange.headers = Some(Instant::now());
    }

    /// Logs the exchange without an end: its body is read incrementally by
    /// the caller.
    pub(crate) fn open(mut self) {
        self.open = true;
    }
}

impl Drop for ExchangeTimer {
    /// Logs the exchange in the current scope, ended now unless left open.
    fn drop(&mut self) {
        if !self.open {
            self.exchange.ended = Some(Instant::now());
        }
        let exchange = self.exchange;
        EXCHANGES
            .try_with(|log| log.borrow_mut().push(exchange))
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::{ExchangeTimer, scope, take};

    /// Exchanges are logged in a scope, in send order, with an open stream
    /// left unended, and nothing is logged outside a scope.
    ///
    /// # Panics
    ///
    /// Panics when the log is wrong.
    #[tokio::test]
    async fn exchanges_are_logged_only_inside_a_scope() {
        drop(ExchangeTimer::start());
        assert!(take().is_empty(), "nothing is logged outside a scope");
        let logged = scope(async {
            let mut buffered = ExchangeTimer::start();
            buffered.headers();
            drop(buffered);
            let mut streamed = ExchangeTimer::start();
            streamed.headers();
            streamed.open();
            take()
        })
        .await;
        assert_eq!(logged.len(), 2);
        let (buffered, streamed) = (logged[0], logged[1]);
        assert!(buffered.headers.is_some() && buffered.ended >= buffered.headers);
        assert!(streamed.headers.is_some() && streamed.ended.is_none());
        assert!(streamed.sent >= buffered.sent);
    }
}
