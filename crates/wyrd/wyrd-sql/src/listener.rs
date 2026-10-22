//! Postgres notification listening for node-local caches.
//!
//! A node-local cache may serve only while it hears every invalidation for
//! what it holds. [`NotificationListener::watch`] holds one pool connection
//! subscribed with `LISTEN` to one channel and tells its cache, through
//! `reset`, exactly when that is true: `reset(true)` after a subscription
//! succeeds, from an empty cache, and `reset(false)` as soon as listening may
//! have stopped. Each notification's payload goes to `notify`.
//!
//! A connection whose network path silently stops delivering bytes reports
//! nothing on its own, so the listener probes its own connection with
//! `SELECT 1` every [`HEARTBEAT`] on a fixed schedule that notification
//! traffic does not postpone, and treats a probe that does not answer
//! within [`PROBE_TIMEOUT`] as a lost connection. A silent stall therefore
//! stops cache service within [`HEARTBEAT`] plus [`PROBE_TIMEOUT`], plus
//! scheduling delay. This bounds staleness after a stall; it is not
//! immediate revocation and says nothing about work already in flight.
//!
//! SQLx's eager reconnection is disabled, so a reported disconnect returns
//! control here before any replacement connects: the cache is switched off
//! first, then the failed listener is discarded and never queried again, and
//! a new subscription is attempted after [`RECONNECT_DELAY`]. Every pending
//! acquisition, subscription, probe, receive, and retry also ends when the
//! caller's shutdown token is cancelled.
//!
//! Dropping a SQLx 0.9 `PgListener` that still holds its connection spawns a
//! task that sends `UNLISTEN *` and waits for the answer, which never comes
//! over a stalled path, so the task and its pool slot would be retained until
//! the operating system gives up on the socket. A failed listener's
//! connection is therefore swapped for a fresh connection opened outside the
//! pool, so the swap never waits on a free pool slot, and the stalled one is
//! closed without waiting on its peer; see [`NotificationListener::discard`].

use std::time::Duration;

use sqlx::postgres::{PgConnection, PgListener};
use sqlx::{Acquire as _, Connection as _, PgPool};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

/// How often a subscribed listener probes its own connection.
pub const HEARTBEAT: Duration = Duration::from_secs(30);

/// How long a probe, acquisition, subscription, or disposal may wait before
/// the listener treats its connection as lost.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Pause before subscribing again after a listener fails or cannot start.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(1);

/// Why one subscription stopped listening.
enum Ended {
    /// The shutdown token was cancelled.
    Shutdown,
    /// SQLx reported the connection lost and already closed it.
    Disconnected,
    /// A receive, probe, or subscription failed or timed out; the listener
    /// still holds a connection that must not be used again.
    Failed,
}

/// One cache's subscription to one Postgres notification channel.
///
/// Built by the owning SQL handle, which supplies its pool, so domain code
/// never holds a raw pool. Each cache owns its own listener and worker, and
/// each listener holds its own connection while subscribed.
pub struct NotificationListener {
    /// Pool the subscription and its replacements are acquired from.
    pool: PgPool,
    /// The channel to `LISTEN` on.
    channel: String,
    /// Interval between liveness probes; [`HEARTBEAT`] in production.
    heartbeat: Duration,
    /// Bound on each probe and pending stage; [`PROBE_TIMEOUT`] in
    /// production.
    probe_timeout: Duration,
}

impl NotificationListener {
    /// A listener on `channel` over `pool` with the production heartbeat.
    ///
    /// SQL owners call this from their handle's `listener` method so the
    /// pool stays behind the handle; domain code asks the handle instead.
    #[must_use]
    pub fn new(pool: PgPool, channel: &str) -> Self {
        Self {
            pool,
            channel: channel.to_owned(),
            heartbeat: HEARTBEAT,
            probe_timeout: PROBE_TIMEOUT,
        }
    }

    /// Shortens the heartbeat interval and probe timeout so a test can
    /// observe stall detection without waiting the production bound.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub const fn with_heartbeat_for_test(mut self, heartbeat: Duration, timeout: Duration) -> Self {
        self.heartbeat = heartbeat;
        self.probe_timeout = timeout;
        self
    }

    /// Listens until `shutdown`, keeping the caller's cache serving only
    /// while every notification on the channel is being heard.
    ///
    /// Starts with `reset(false)`. Each successful subscription calls
    /// `reset(true)` and then hands every notification's payload, including
    /// ones buffered while a probe ran, to `notify` in arrival order. Any
    /// loss, failed probe, or shutdown calls `reset(false)` before the failed
    /// listener is discarded or a replacement is attempted, and a failed start
    /// leaves the cache off and retries after [`RECONNECT_DELAY`]. While a
    /// failed listener cannot be released, no new subscription is attempted,
    /// so at most one failed connection is ever retained. Returns
    /// after `shutdown` with the cache off; cancellation is observed at every
    /// pending stage, and discarding the last listener is bounded by the probe
    /// timeout.
    pub async fn watch(
        &self,
        shutdown: CancellationToken,
        reset: impl Fn(bool),
        notify: impl Fn(&str),
    ) {
        reset(false);
        let mut unreleased = None;
        while !shutdown.is_cancelled() {
            if let Some(failed) = unreleased.take() {
                unreleased = self.discard(failed).await;
            }
            if unreleased.is_none() {
                unreleased = self.subscription(&shutdown, &reset, &notify).await;
            }
            tokio::select! {
                () = shutdown.cancelled() => {}
                () = tokio::time::sleep(RECONNECT_DELAY) => {}
            }
        }
        if let Some(failed) = unreleased {
            // One last bounded release attempt; if it fails too, the
            // stalled connection is retained only until its socket fails.
            drop(self.discard(failed).await);
        }
        reset(false);
    }

    /// Runs one subscription from acquisition to its end and returns its
    /// listener when it could not be released.
    ///
    /// The cache is switched off before this returns or discards anything.
    async fn subscription(
        &self,
        shutdown: &CancellationToken,
        reset: &impl Fn(bool),
        notify: &impl Fn(&str),
    ) -> Option<PgListener> {
        let acquired = tokio::select! {
            () = shutdown.cancelled() => return None,
            acquired = tokio::time::timeout(self.probe_timeout, PgListener::connect_with(&self.pool)) => acquired,
        };
        let mut listener = match acquired {
            Ok(Ok(listener)) => listener,
            Ok(Err(error)) => {
                tracing::warn!(%error, channel = %self.channel, "cache listener could not connect; reads go to Postgres");
                return None;
            }
            Err(_) => {
                tracing::warn!(channel = %self.channel, "cache listener connect timed out; reads go to Postgres");
                return None;
            }
        };
        listener.eager_reconnect(false);
        let ended = match self.subscribe(&mut listener, shutdown).await {
            Ok(()) => {
                reset(true);
                self.receive(&mut listener, shutdown, notify).await
            }
            Err(ended) => ended,
        };
        reset(false);
        match ended {
            Ended::Disconnected => None,
            Ended::Shutdown | Ended::Failed => self.discard(listener).await,
        }
    }

    /// Sends `LISTEN` on `listener`'s connection.
    ///
    /// # Errors
    /// Returns [`Ended::Failed`] when the statement fails or times out, and
    /// [`Ended::Shutdown`] when `shutdown` is cancelled first.
    async fn subscribe(
        &self,
        listener: &mut PgListener,
        shutdown: &CancellationToken,
    ) -> Result<(), Ended> {
        let listened = tokio::select! {
            () = shutdown.cancelled() => return Err(Ended::Shutdown),
            listened = tokio::time::timeout(self.probe_timeout, listener.listen(&self.channel)) => listened,
        };
        match listened {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                tracing::warn!(%error, channel = %self.channel, "cache listener could not subscribe; reads go to Postgres");
                Err(Ended::Failed)
            }
            Err(_) => {
                tracing::warn!(channel = %self.channel, "cache listener subscription timed out; reads go to Postgres");
                Err(Ended::Failed)
            }
        }
    }

    /// Delivers notifications and probes the connection on a fixed schedule
    /// until the subscription ends.
    ///
    /// The probe deadline is computed from the subscription start, so
    /// notification traffic never postpones it. A probe runs on the
    /// listener's own connection; notifications that arrive meanwhile are
    /// buffered by SQLx and returned by the next receive. Cancelling an
    /// in-progress receive loses nothing, because SQLx consumes a message
    /// only once it is complete.
    async fn receive(
        &self,
        listener: &mut PgListener,
        shutdown: &CancellationToken,
        notify: &impl Fn(&str),
    ) -> Ended {
        let mut heartbeat =
            tokio::time::interval_at(Instant::now() + self.heartbeat, self.heartbeat);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let received = tokio::select! {
                () = shutdown.cancelled() => return Ended::Shutdown,
                _ = heartbeat.tick() => None,
                received = listener.try_recv() => Some(received),
            };
            match received {
                None => {
                    if let Err(ended) = self.probe(listener, shutdown).await {
                        return ended;
                    }
                }
                Some(Ok(Some(notification))) => notify(notification.payload()),
                Some(Ok(None)) => {
                    tracing::warn!(channel = %self.channel, "cache listener connection lost; reads go to Postgres");
                    return Ended::Disconnected;
                }
                Some(Err(error)) => {
                    tracing::warn!(%error, channel = %self.channel, "cache listener failed; reads go to Postgres");
                    return Ended::Failed;
                }
            }
        }
    }

    /// Runs one `SELECT 1` liveness probe on `listener`'s own connection.
    ///
    /// # Errors
    /// Returns [`Ended::Failed`] when the probe fails or does not answer
    /// within the probe timeout, and [`Ended::Shutdown`] on cancellation.
    async fn probe(
        &self,
        listener: &mut PgListener,
        shutdown: &CancellationToken,
    ) -> Result<(), Ended> {
        let probed = tokio::select! {
            () = shutdown.cancelled() => return Err(Ended::Shutdown),
            probed = tokio::time::timeout(self.probe_timeout, sqlx::query("SELECT 1").execute(&mut *listener)) => probed,
        };
        match probed {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => {
                tracing::warn!(%error, channel = %self.channel, "cache listener heartbeat failed; reads go to Postgres");
                Err(Ended::Failed)
            }
            Err(_) => {
                tracing::warn!(channel = %self.channel, "cache listener heartbeat timed out; reads go to Postgres");
                Err(Ended::Failed)
            }
        }
    }

    /// Releases a failed listener without waiting on its connection's peer,
    /// returning it when it could not be released yet.
    ///
    /// `listener` must still hold its connection, so borrowing it cannot
    /// reconnect. The connection is swapped for a fresh one opened with the
    /// pool's connect options but outside the pool, so the swap needs no free
    /// pool slot, and the failed one is closed without a goodbye; dropping
    /// the listener then sends its `UNLISTEN *` over the healthy connection
    /// and returns it to the pool in the failed one's place, so no pool slot
    /// or cleanup task stays behind. When the database accepts no fresh
    /// connection within the probe timeout the listener is returned for a
    /// later attempt, so at most one failed connection is retained per
    /// listener.
    async fn discard(&self, mut listener: PgListener) -> Option<PgListener> {
        let replaced = tokio::time::timeout(self.probe_timeout, async {
            let fresh = PgConnection::connect_with(&self.pool.connect_options()).await?;
            let held = (&mut listener).acquire().await?;
            Ok::<_, sqlx::Error>(std::mem::replace(held, fresh))
        })
        .await;
        match replaced {
            Ok(Ok(failed)) => {
                // Closing only shuts the socket down; dropping it on timeout
                // closes the socket all the same.
                let _ = tokio::time::timeout(self.probe_timeout, failed.close_hard()).await;
                drop(listener);
                None
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, channel = %self.channel, "failed cache listener not yet released");
                Some(listener)
            }
            Err(_) => {
                tracing::warn!(channel = %self.channel, "failed cache listener release timed out");
                Some(listener)
            }
        }
    }
}
