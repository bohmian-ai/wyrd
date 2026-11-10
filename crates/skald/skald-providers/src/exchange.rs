//! Transient monotonic timing of provider HTTP exchanges.
//!
//! A caller that must separate time spent waiting on a provider from its own
//! preparation and decode work runs its provider calls inside [`scope`]. Every
//! HTTP exchange this crate sends inside the scope is logged as one
//! [`Exchange`]: when the request was sent, when its response headers
//! arrived, and when its body was fully read or the exchange failed. A
//! streamed answer whose body is read elsewhere is logged open, with no end.
//! The caller drains the log with [`take`]. Outside a scope nothing is
//! recorded, so ordinary callers pay only a failed task-local lookup. A
//! scope may also name headers every exchange inside it forwards, such as a
//! caller's request classification.
//!
//! The log is per task: an exchange sent by a task spawned inside the scope
//! is not recorded. Timings are process-local monotonic instants, never
//! persisted or sent on the wire.

use std::cell::RefCell;
use std::future::Future;
use std::time::Instant;

use reqwest::header::HeaderMap;

tokio::task_local! {
    /// The current task's [`scope`].
    static SCOPE: Scope;
}

/// One [`scope`]'s state.
struct Scope {
    /// Exchanges sent inside it, in send order.
    exchanges: RefCell<Vec<Exchange>>,
    /// Headers every exchange inside it forwards.
    forwarded: HeaderMap,
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

/// Runs `future` with an empty exchange log for its task, every exchange it
/// sends forwarding `forwarded` beside its own headers.
///
/// Scopes nest: an inner scope shadows the outer one until it ends.
pub async fn scope<F: Future>(forwarded: HeaderMap, future: F) -> F::Output {
    let scope = Scope {
        exchanges: RefCell::new(Vec::new()),
        forwarded,
    };
    SCOPE.scope(scope, future).await
}

/// Drains the current scope's logged exchanges in send order; empty outside
/// a scope.
#[must_use]
pub fn take() -> Vec<Exchange> {
    SCOPE
        .try_with(|scope| std::mem::take(&mut *scope.exchanges.borrow_mut()))
        .unwrap_or_default()
}

/// Adds the current [`scope`]'s forwarded headers to an exchange's
/// `headers`; nothing outside a scope.
pub(crate) fn forward(headers: &mut HeaderMap) {
    SCOPE
        .try_with(|scope| {
            for (name, value) in &scope.forwarded {
                headers.insert(name, value.clone());
            }
        })
        .ok();
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
        SCOPE
            .try_with(|scope| scope.exchanges.borrow_mut().push(exchange))
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use reqwest::header::{HeaderMap, HeaderValue};

    use super::{ExchangeTimer, forward, scope, take};

    /// An exchange forwards its scope's headers, and nothing outside a
    /// scope; its own header of the same name is replaced.
    ///
    /// # Panics
    ///
    /// Panics when a header is added outside a scope or missing inside one.
    #[tokio::test]
    async fn exchanges_forward_only_their_scope_headers() {
        let mut forwarded = HeaderMap::new();
        forwarded.insert("x-forwarded-case", HeaderValue::from_static("case"));
        let mut outside = HeaderMap::new();
        forward(&mut outside);
        assert!(outside.is_empty(), "nothing is forwarded outside a scope");
        let inside = scope(forwarded.clone(), async {
            let mut headers = HeaderMap::new();
            headers.insert("x-forwarded-case", HeaderValue::from_static("own"));
            forward(&mut headers);
            headers
        })
        .await;
        assert_eq!(inside, forwarded);
    }

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
        let logged = scope(HeaderMap::new(), async {
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
