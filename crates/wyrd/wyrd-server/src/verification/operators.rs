//! The generic Operator worker: claim, resolve, deliver, settle.
//!
//! [`OperatorWorker`] takes an execution permit before it claims a due
//! `wyrd.operator_dispatches` row, resolves the frozen Operator (a Card UID or
//! an inline body digest on the owner Card), re-checks its connection
//! authority, decrypts the latest credential for this attempt only, and hands
//! the action to [`OperatorDelivery`]. Each dispatch settles on its own fenced
//! lease — delivered, retried with the server backoff, failed, or released on
//! shutdown — so one Operator's failure never blocks a sibling or touches the
//! Verifier result. Claims and settlements are engine mechanics, not
//! authorization decisions, so none of them writes audit.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::{Method, Response, StatusCode};
use tokio::task::{JoinError, JoinSet};
use tokio_util::sync::CancellationToken;
use url::Url;
use wyrd_auth_oidc::{ScreenError, ScreenedHttp};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::operator::{
    HttpAuth, HttpMethod, MAX_ATTEMPT_SECONDS, NotifyChannel, OperatorAction,
    OperatorFailureContext, OperatorSpec, operator_url_origin,
};
use wyrd_spec::card::verifier::VerificationBinding;
use wyrd_spec::envelope::Spec;
use wyrd_spec::operator_connection::{ConnectionSecret, HttpsOrigin, OperatorConnectionStatus};
use wyrd_spec::reference::InlineableRef;
use wyrd_spec::verification::{FrozenTarget, VerificationError};
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::operator_connections::find_connection;
use wyrd_sql::queries::operator_dispatches::{
    ClaimedDispatch, DispatchRetry, OperatorDispatchQueue,
};
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
#[cfg(feature = "test-support")]
use super::health::RuntimeCapability;
use super::permits::{VerifierPermit, VerifierPermits};
use crate::components::operators::keys::{KeyError, OperatorKeys};

/// The frozen Operator no longer resolves to a supported Operator body.
pub const OPERATOR_UNAVAILABLE: &str = "operator_unavailable";
/// The named connection is missing, disabled, of another provider, or of
/// another authority; deliberately one code for all four.
pub const CONNECTION_UNAVAILABLE: &str = "connection_unavailable";
/// The key provider or Postgres could not supply the credential this attempt.
pub const CREDENTIAL_STORE_UNAVAILABLE: &str = "credential_store_unavailable";
/// The stored credential failed authentication or does not fit its provider.
pub const CREDENTIAL_INVALID: &str = "credential_invalid";
/// A template, URL, or header could not be rendered into a valid request.
pub const INVALID_REQUEST: &str = "invalid_request";
/// The destination resolves to an address this deployment must not reach, or
/// redirected to another origin.
pub const DESTINATION_REJECTED: &str = "destination_rejected";
/// The destination could not be reached or did not answer in time.
pub const DESTINATION_UNREACHABLE: &str = "destination_unreachable";
/// The attempt exceeded its timeout.
pub const ATTEMPT_TIMED_OUT: &str = "attempt_timed_out";
/// The provider answered with a transient failure (408, 429, 5xx, or a
/// provider-declared transient error).
pub const PROVIDER_TRANSIENT: &str = "provider_transient";
/// The provider refused the request as a permission, configuration, or
/// payload error.
pub const PROVIDER_REJECTED: &str = "provider_rejected";

/// Tenants examined per claim round, most overdue first.
const TENANTS_PER_ROUND: i64 = 64;
/// Largest provider response body read, in bytes.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// Same-origin redirects an HTTP Operator may follow in one attempt.
const MAX_REDIRECTS: usize = 3;
/// Interval between key-rewrap passes of this worker.
const REWRAP_INTERVAL: Duration = Duration::from_secs(300);
/// Slack errors that are transient rather than configuration failures.
const SLACK_TRANSIENT: [&str; 5] = [
    "ratelimited",
    "internal_error",
    "service_unavailable",
    "request_timeout",
    "fatal_error",
];

/// Where the fixed-provider Operators send.
///
/// Production uses the public Slack and PagerDuty endpoints; local journeys
/// point these at mock providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEndpoints {
    /// Slack `chat.postMessage`.
    pub slack: Url,
    /// PagerDuty Events API v2 enqueue.
    pub pager_duty: Url,
}

impl Default for ProviderEndpoints {
    /// The public provider endpoints.
    ///
    /// # Panics
    /// Never: both literals are valid URLs.
    fn default() -> Self {
        Self {
            slack: Url::parse("https://slack.com/api/chat.postMessage")
                .expect("invariant: the Slack endpoint literal parses"),
            pager_duty: Url::parse("https://events.pagerduty.com/v2/enqueue")
                .expect("invariant: the PagerDuty endpoint literal parses"),
        }
    }
}

/// How one delivery attempt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attempt {
    /// The provider accepted the request.
    Delivered,
    /// Try again after the server backoff, or later when the provider asked.
    Retry {
        /// Why this attempt failed.
        error: VerificationError,
        /// A provider-requested delay, clipped to the deadline by Postgres.
        retry_after: Option<Duration>,
    },
    /// Fail the dispatch without another attempt.
    Terminal(VerificationError),
    /// Shutdown abandoned the attempt; return it with its attempt refunded.
    Release,
}

impl Attempt {
    /// A retry with no provider-requested delay.
    fn retry(code: &str, message: &str) -> Self {
        Self::Retry {
            error: failure(code, message),
            retry_after: None,
        }
    }

    /// A terminal failure.
    fn terminal(code: &str, message: &str) -> Self {
        Self::Terminal(failure(code, message))
    }
}

/// Owner of claiming, resolving, delivering, and settling Operator dispatches.
pub struct OperatorWorker {
    /// Wyrd Postgres owner that opens every tenant-scoped claim, read, and
    /// settlement transaction.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant due list and rewrap passes.
    operator: OperatorPool,
    /// Delivery ceilings and fenced transitions.
    queue: OperatorDispatchQueue,
    /// Global and per-tenant Operator execution capacity, separate from the
    /// Verifier pool.
    permits: VerifierPermits,
    /// Tenant key-encryption keys for opening credentials.
    keys: Arc<OperatorKeys>,
    /// Provider wire adapters.
    delivery: OperatorDelivery,
    /// Runtime bounds.
    limits: RuntimeLimits,
    /// Test-only crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl OperatorWorker {
    /// Build a worker over the Wyrd Postgres owner and the operator pool.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        keys: Arc<OperatorKeys>,
        delivery: OperatorDelivery,
        limits: RuntimeLimits,
    ) -> Self {
        Self {
            postgres,
            operator,
            queue: OperatorDispatchQueue::new(
                limits.operator_attempts,
                limits.operator_deadline,
                limits.operator_lease,
            ),
            permits: VerifierPermits::new(limits.global_permits, limits.tenant_permits),
            keys,
            delivery,
            limits,
            #[cfg(feature = "test-support")]
            crash: None,
        }
    }

    /// Let `crash` panic this worker's loop.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_crash(mut self, crash: CapabilityCrash) -> Self {
        self.crash = Some(crash);
        self
    }

    /// Claim and deliver dispatches until `stop` is cancelled, then drain.
    ///
    /// Mirrors the Verifier runner: each round claims one dispatch per due
    /// tenant with capacity; on `stop` no claim is admitted, in-flight
    /// attempts get the drain grace, and the rest are cancelled and released
    /// with their attempt refunded. Between rounds the worker rewraps rows
    /// still on an older key version, bounded per tenant.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed an Operator-worker crash.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        let abandon = CancellationToken::new();
        let mut work = JoinSet::new();
        let mut last_rewrap: Option<Instant> = None;
        while !stop.is_cancelled() {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(RuntimeCapability::OperatorWorker);
            }
            if last_rewrap.is_none_or(|at| at.elapsed() >= REWRAP_INTERVAL) {
                last_rewrap = Some(Instant::now());
                if let Err(error) = self.keys.rewrap_pass(&self.postgres, &self.operator).await {
                    tracing::warn!(%error, "operator key rewrap pass failed");
                }
            }
            let claimed = match self.claim_round(&stop, &abandon, &mut work).await {
                Ok(claimed) => claimed,
                Err(error) => {
                    tracing::warn!(%error, "operator dispatch claim round failed");
                    0
                }
            };
            while let Some(finished) = work.try_join_next() {
                reap(finished);
            }
            self.record_active();
            if claimed > 0 {
                continue;
            }
            tokio::select! {
                () = stop.cancelled() => {}
                () = tokio::time::sleep(self.limits.poll_interval) => {}
                Some(finished) = work.join_next(), if !work.is_empty() => reap(finished),
            }
        }
        let drained = tokio::time::timeout(self.limits.drain_grace, async {
            while let Some(finished) = work.join_next().await {
                reap(finished);
            }
        })
        .await;
        if drained.is_err() {
            tracing::warn!(
                in_flight = work.len(),
                "operator drain grace elapsed; releasing in-flight dispatches"
            );
            abandon.cancel();
            while let Some(finished) = work.join_next().await {
                reap(finished);
            }
        }
        self.record_active();
    }

    /// Claim at most one dispatch for each due tenant that has capacity.
    ///
    /// A permit is taken before the claim and dropped unused when the tenant
    /// had nothing claimable. A claim committed after `stop` fired is released
    /// at once instead of being spawned.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the due-tenant list or a claim fails;
    /// dispatches claimed earlier in the round are already spawned.
    async fn claim_round(
        self: &Arc<Self>,
        stop: &CancellationToken,
        abandon: &CancellationToken,
        work: &mut JoinSet<()>,
    ) -> Result<usize, SqlError> {
        if self.permits.saturated() {
            return Ok(0);
        }
        let tenants = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(0),
            tenants = self.queue.due_tenants(&self.operator, TENANTS_PER_ROUND) => tenants?,
        };
        let mut claimed = 0;
        for tenant in tenants {
            if stop.is_cancelled() || self.permits.saturated() {
                break;
            }
            let Some(permit) = self.permits.try_acquire(tenant) else {
                continue;
            };
            let Some(dispatch) = self.claim(tenant, stop).await? else {
                continue;
            };
            if stop.is_cancelled() {
                if let Err(error) = self.settle(tenant, &dispatch, Attempt::Release).await {
                    tracing::error!(dispatch_id = %dispatch.lease.dispatch_id, %error, "releasing a late operator claim failed; the lease will expire");
                }
                break;
            }
            claimed += 1;
            work.spawn(Arc::clone(self).process(
                tenant,
                dispatch,
                permit,
                stop.clone(),
                abandon.clone(),
            ));
        }
        Ok(claimed)
    }

    /// Claim `tenant`'s next due dispatch in its own committed transaction.
    ///
    /// The uncommitted claim races `stop` and rolls back when `stop` wins; the
    /// commit is not raced, so a committed claim is always returned.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the transaction or claim fails.
    async fn claim(
        &self,
        tenant: DataTenantId,
        stop: &CancellationToken,
    ) -> Result<Option<ClaimedDispatch>, SqlError> {
        let uncommitted = async {
            let mut conn = self.postgres.tenant_conn(tenant).await?;
            let dispatch = self.queue.claim(&mut conn).await?;
            Ok::<_, SqlError>((conn, dispatch))
        };
        let (conn, dispatch) = tokio::select! {
            biased;
            () = stop.cancelled() => return Ok(None),
            claimed = uncommitted => claimed?,
        };
        conn.commit().await?;
        Ok(dispatch)
    }

    /// Attempt one claimed dispatch and apply its single settlement.
    ///
    /// Holds `permit` until settlement. `abandon` cancels the attempt and
    /// releases the lease; a retryable failure observed after `stop` is also
    /// released, since the process, not the dispatch, failed.
    async fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        dispatch: ClaimedDispatch,
        permit: VerifierPermit,
        stop: CancellationToken,
        abandon: CancellationToken,
    ) {
        let _permit = permit;
        let started = Instant::now();
        let timeout = self.limits.operator_attempt_timeout.min(dispatch.remaining);
        let attempt = tokio::select! {
            () = abandon.cancelled() => Attempt::Release,
            attempt = tokio::time::timeout(timeout, self.attempt(tenant, &dispatch, timeout)) => {
                attempt.unwrap_or_else(|_| Attempt::retry(ATTEMPT_TIMED_OUT, "the delivery attempt exceeded its timeout"))
            }
        };
        let attempt = match attempt {
            Attempt::Retry { .. } if stop.is_cancelled() => Attempt::Release,
            attempt => attempt,
        };
        let outcome = match self.settle(tenant, &dispatch, attempt).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::error!(dispatch_id = %dispatch.lease.dispatch_id, %error, "operator settlement failed; the lease will expire");
                "settlement_failed"
            }
        };
        tracing::info!(dispatch_id = %dispatch.lease.dispatch_id, attempt = dispatch.attempt, outcome, "operator dispatch attempt settled");
        metrics::counter!(
            crate::app::metrics::OPERATOR_DISPATCH_ATTEMPTS_TOTAL,
            "outcome" => outcome
        )
        .increment(1);
        metrics::histogram!(
            crate::app::metrics::OPERATOR_DISPATCH_DURATION_SECONDS,
            "outcome" => outcome
        )
        .record(started.elapsed().as_secs_f64());
    }

    /// Resolve, authorize, decrypt, and deliver one attempt.
    ///
    /// Never fails: every failure becomes the [`Attempt`] it maps to. The
    /// decrypted credential lives only inside this call.
    async fn attempt(
        &self,
        tenant: DataTenantId,
        dispatch: &ClaimedDispatch,
        timeout: Duration,
    ) -> Attempt {
        let Ok(context) =
            serde_json::from_value::<OperatorFailureContext>(dispatch.failure_context.clone())
        else {
            return Attempt::terminal(INVALID_REQUEST, "the frozen failure context is malformed");
        };
        let spec = match self.resolve(tenant, dispatch).await {
            Ok(spec) => spec,
            Err(attempt) => return attempt,
        };
        let secret = match self.credential(tenant, &spec).await {
            Ok(secret) => secret,
            Err(attempt) => return attempt,
        };
        self.delivery
            .send(&spec, secret.as_ref(), &context, timeout)
            .await
    }

    /// Load the frozen Operator body: a Card by UID, or the owner Card's
    /// inline `on_failure` body whose canonical digest was frozen.
    ///
    /// # Errors
    /// Returns a retry for a transient registry failure and a terminal
    /// [`OPERATOR_UNAVAILABLE`] when nothing matches.
    async fn resolve(
        &self,
        tenant: DataTenantId,
        dispatch: &ClaimedDispatch,
    ) -> Result<OperatorSpec, Attempt> {
        let unavailable = || {
            Attempt::terminal(
                OPERATOR_UNAVAILABLE,
                "the frozen Operator no longer resolves",
            )
        };
        let card_uid = match &dispatch.operator {
            FrozenTarget::Uid(uid) => uid,
            FrozenTarget::Digest(_) => dispatch.owner_card_uid.as_ref().ok_or_else(unavailable)?,
        };
        let mut conn = self.postgres.tenant_conn(tenant).await.map_err(|_| {
            Attempt::retry(CREDENTIAL_STORE_UNAVAILABLE, "the registry is unavailable")
        })?;
        let card = get_card_by_uid(&mut conn, card_uid)
            .await
            .map_err(|error| {
                if error.status() >= 500 {
                    Attempt::retry(CREDENTIAL_STORE_UNAVAILABLE, "the registry is unavailable")
                } else {
                    unavailable()
                }
            })?;
        drop(conn);
        match (&dispatch.operator, card.spec) {
            (FrozenTarget::Uid(_), Spec::Operator(spec)) => Ok(spec),
            (FrozenTarget::Digest(digest), owner) => inline_operators(&owner)
                .find(|body| {
                    Spec::Operator((*body).clone())
                        .canonical_hash()
                        .is_ok_and(|hash| hash.to_string() == *digest)
                })
                .cloned()
                .ok_or_else(unavailable),
            _ => Err(unavailable()),
        }
    }

    /// Re-check the connection authority and decrypt the latest credential.
    ///
    /// Returns `None` for an Operator that names no connection. The same
    /// predicate registration used runs before decryption, so a disabled,
    /// renamed-provider, or re-pointed connection fails closed here.
    ///
    /// # Errors
    /// Returns a terminal [`CONNECTION_UNAVAILABLE`] or [`CREDENTIAL_INVALID`],
    /// or a retry [`CREDENTIAL_STORE_UNAVAILABLE`] when Postgres or the key
    /// provider is unavailable.
    async fn credential(
        &self,
        tenant: DataTenantId,
        spec: &OperatorSpec,
    ) -> Result<Option<ConnectionSecret>, Attempt> {
        let Some((provider, name)) = spec.connection() else {
            return Ok(None);
        };
        let store_down = || {
            Attempt::retry(
                CREDENTIAL_STORE_UNAVAILABLE,
                "the credential store is unavailable",
            )
        };
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|_| store_down())?;
        let stored = find_connection(&mut conn, provider, name)
            .await
            .map_err(|_| store_down())?;
        drop(conn);
        let Some(stored) = stored.filter(|stored| {
            stored.view.status == OperatorConnectionStatus::Active
                && spec.matches_authority(&stored.view.config)
        }) else {
            return Err(Attempt::terminal(
                CONNECTION_UNAVAILABLE,
                &format!("no active {provider} connection named {name} with matching authority"),
            ));
        };
        let plaintext = self
            .keys
            .open(tenant, &stored)
            .await
            .map_err(|error| match error {
                KeyError::Unavailable { .. } => store_down(),
                KeyError::Authentication { .. } => Attempt::terminal(
                    CREDENTIAL_INVALID,
                    "the stored credential failed authentication",
                ),
            })?;
        serde_json::from_slice(&plaintext).map(Some).map_err(|_| {
            Attempt::terminal(CREDENTIAL_INVALID, "the stored credential is malformed")
        })
    }

    /// Apply one attempt's fenced settlement and return its telemetry label.
    ///
    /// A retry waits the server backoff for this attempt number, or longer
    /// when the provider asked; Postgres clips the next attempt to the
    /// dispatch deadline and fails the row once the budget or deadline is
    /// spent.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the tenant transaction or settlement fails.
    async fn settle(
        &self,
        tenant: DataTenantId,
        dispatch: &ClaimedDispatch,
        attempt: Attempt,
    ) -> Result<&'static str, SqlError> {
        let lease = dispatch.lease;
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let outcome = match attempt {
            Attempt::Delivered => fenced(self.queue.deliver(&mut conn, lease).await?, "delivered"),
            Attempt::Terminal(error) => {
                fenced(self.queue.fail(&mut conn, lease, &error).await?, "failed")
            }
            Attempt::Release => fenced(self.queue.release(&mut conn, lease).await?, "released"),
            Attempt::Retry { error, retry_after } => {
                let backoff = self.limits.operator_backoff(dispatch.attempt);
                let delay = retry_after.map_or(backoff, |after| after.max(backoff));
                match self.queue.retry(&mut conn, lease, &error, delay).await? {
                    DispatchRetry::Scheduled(_) => "retrying",
                    DispatchRetry::Exhausted => "failed",
                    DispatchRetry::StaleLease => "stale_lease",
                }
            }
        };
        conn.commit().await?;
        Ok(outcome)
    }

    /// Publish the active-dispatch gauge.
    fn record_active(&self) {
        metrics::gauge!(crate::app::metrics::OPERATOR_ACTIVE_DISPATCHES).set(f64::from(
            u32::try_from(self.permits.active()).unwrap_or(u32::MAX),
        ));
    }
}

/// The provider wire of every supported Operator action.
///
/// Owns the deployment's SSRF address policy and the provider endpoints.
/// Every request — fixed provider or authored URL — gets a client screened
/// and pinned to the destination's resolved addresses, built with this
/// attempt's timeout, and never follows a redirect on its own.
#[derive(Debug, Clone)]
pub struct OperatorDelivery {
    /// Address policy every destination is screened under.
    http: ScreenedHttp,
    /// Slack and PagerDuty endpoints.
    endpoints: ProviderEndpoints,
}

impl OperatorDelivery {
    /// Deliver through `endpoints` under `http`'s address policy.
    #[must_use]
    pub const fn new(http: ScreenedHttp, endpoints: ProviderEndpoints) -> Self {
        Self { http, endpoints }
    }

    /// Render and send one Operator action with its optional credential.
    ///
    /// Never fails: every failure is classified into an [`Attempt`].
    pub async fn send(
        &self,
        spec: &OperatorSpec,
        secret: Option<&ConnectionSecret>,
        context: &OperatorFailureContext,
        timeout: Duration,
    ) -> Attempt {
        match (&spec.action, secret) {
            (
                OperatorAction::Notify {
                    channel:
                        NotifyChannel::Slack {
                            channel_id, text, ..
                        },
                },
                Some(ConnectionSecret::Token { value }),
            ) => {
                let Ok(text) = context.render(text) else {
                    return Attempt::terminal(
                        INVALID_REQUEST,
                        "the Slack text template is invalid",
                    );
                };
                let body = serde_json::json!({ "channel": channel_id, "text": text });
                self.slack(value.expose(), &body, timeout).await
            }
            (
                OperatorAction::Notify {
                    channel:
                        NotifyChannel::PagerDuty {
                            route,
                            severity,
                            summary,
                            dedup_key,
                            ..
                        },
                },
                Some(ConnectionSecret::Token { value }),
            ) => {
                let dedup = dedup_key.as_deref().map_or_else(
                    || Ok(context.dispatch_id.to_string()),
                    |key| context.render(key),
                );
                let (Ok(summary), Ok(dedup)) = (context.render(summary), dedup) else {
                    return Attempt::terminal(INVALID_REQUEST, "a PagerDuty template is invalid");
                };
                let body = serde_json::json!({
                    "routing_key": value.expose(),
                    "event_action": "trigger",
                    "dedup_key": dedup,
                    "payload": {
                        "summary": summary,
                        "source": context.subject_ref,
                        "severity": severity,
                        "custom_details": {
                            "wyrd_route": route,
                            "run_id": context.run_id,
                            "result_id": context.result_id,
                            "binding_id": context.binding_id,
                            "verifier_ref": context.verifier_ref,
                        },
                    },
                });
                self.pager_duty(&body, timeout).await
            }
            (
                OperatorAction::Http {
                    method,
                    url,
                    headers,
                    body,
                    auth,
                    timeout_seconds,
                    expect_status,
                },
                secret,
            ) if auth.is_some() == secret.is_some() => {
                let timeout = timeout_seconds.map_or(timeout, |seconds| {
                    timeout.min(Duration::from_secs(u64::from(
                        seconds.min(MAX_ATTEMPT_SECONDS),
                    )))
                });
                let request = match HttpRequest::render(
                    *method,
                    url,
                    headers,
                    body.as_ref(),
                    auth.as_ref().zip(secret),
                    context,
                ) {
                    Ok(request) => request,
                    Err(reason) => return Attempt::terminal(INVALID_REQUEST, &reason),
                };
                self.http(request, *expect_status, timeout).await
            }
            (OperatorAction::Workflow { .. }, _) => {
                Attempt::terminal(OPERATOR_UNAVAILABLE, "the workflow action is not invocable")
            }
            _ => Attempt::terminal(
                CREDENTIAL_INVALID,
                "the stored credential does not fit the Operator's provider",
            ),
        }
    }

    /// `chat.postMessage` with the bot token; success is Slack's JSON `ok`.
    async fn slack(&self, token: &str, body: &serde_json::Value, timeout: Duration) -> Attempt {
        let mut headers = HeaderMap::new();
        match bearer(token) {
            Ok(value) => headers.insert(reqwest::header::AUTHORIZATION, value),
            Err(attempt) => return attempt,
        };
        let response = match self
            .post_json(&self.endpoints.slack, headers, body, timeout)
            .await
        {
            Ok(response) => response,
            Err(attempt) => return attempt,
        };
        if let Some(attempt) = status_failure(&response) {
            return attempt;
        }
        let Ok(bytes) = bounded_body(response).await else {
            return Attempt::retry(DESTINATION_UNREACHABLE, "the Slack response was cut short");
        };
        let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        if reply["ok"] == serde_json::Value::Bool(true) {
            return Attempt::Delivered;
        }
        let code = reply["error"].as_str().unwrap_or("unknown_error");
        let message = format!("Slack refused the message: {code}");
        if SLACK_TRANSIENT.contains(&code) {
            Attempt::retry(PROVIDER_TRANSIENT, &message)
        } else {
            Attempt::terminal(PROVIDER_REJECTED, &message)
        }
    }

    /// Events API v2 enqueue; the routing key rides in the body.
    async fn pager_duty(&self, body: &serde_json::Value, timeout: Duration) -> Attempt {
        match self
            .post_json(&self.endpoints.pager_duty, HeaderMap::new(), body, timeout)
            .await
        {
            Ok(response) => status_failure(&response).unwrap_or(Attempt::Delivered),
            Err(attempt) => attempt,
        }
    }

    /// Send an authored HTTP request, following at most [`MAX_REDIRECTS`]
    /// same-origin redirects, each re-screened before the credential rides.
    async fn http(
        &self,
        mut request: HttpRequest,
        expect_status: Option<u16>,
        timeout: Duration,
    ) -> Attempt {
        for _ in 0..=MAX_REDIRECTS {
            let client = match self.client(&request.url, timeout).await {
                Ok(client) => client,
                Err(attempt) => return attempt,
            };
            let mut builder = client
                .request(request.method.clone(), request.url.clone())
                .headers(request.headers.clone());
            if let Some(body) = &request.body {
                builder = builder.json(body);
            }
            let response = match builder.send().await {
                Ok(response) => response,
                Err(error) => return transport_failure(&error),
            };
            let status = response.status();
            if status.is_redirection() {
                let next = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|location| location.to_str().ok())
                    .and_then(|location| request.url.join(location).ok());
                let Some(next) = next.filter(|next| next.origin() == request.url.origin()) else {
                    return Attempt::terminal(
                        DESTINATION_REJECTED,
                        "the destination redirected to another origin",
                    );
                };
                request.url = next;
                continue;
            }
            return match expect_status {
                Some(expected) if status.as_u16() == expected => Attempt::Delivered,
                Some(expected) if status.is_success() => Attempt::terminal(
                    PROVIDER_REJECTED,
                    &format!("the destination answered {status}, expected {expected}"),
                ),
                _ => status_failure(&response).unwrap_or(Attempt::Delivered),
            };
        }
        Attempt::terminal(DESTINATION_REJECTED, "the destination redirected too often")
    }

    /// POST `body` as JSON to `url` through a screened client.
    ///
    /// # Errors
    /// Returns the classified attempt when screening or the send fails.
    async fn post_json(
        &self,
        url: &Url,
        headers: HeaderMap,
        body: &serde_json::Value,
        timeout: Duration,
    ) -> Result<Response, Attempt> {
        self.client(url, timeout)
            .await?
            .post(url.clone())
            .headers(headers)
            .json(body)
            .send()
            .await
            .map_err(|error| transport_failure(&error))
    }

    /// A client screened and pinned to `url`'s addresses with `timeout`.
    ///
    /// # Errors
    /// Returns a terminal [`DESTINATION_REJECTED`] for a blocked address and a
    /// retry [`DESTINATION_UNREACHABLE`] when the name does not resolve.
    async fn client(&self, url: &Url, timeout: Duration) -> Result<reqwest::Client, Attempt> {
        self.http
            .with_timeout(timeout)
            .client_for(url)
            .await
            .map_err(|error| match error {
                ScreenError::Blocked => Attempt::terminal(
                    DESTINATION_REJECTED,
                    "the destination resolves to a blocked address",
                ),
                ScreenError::Unresolved | ScreenError::Client => Attempt::retry(
                    DESTINATION_UNREACHABLE,
                    "the destination could not be resolved",
                ),
            })
    }
}

/// One rendered HTTP Operator request with its credential attached.
struct HttpRequest {
    /// Method.
    method: Method,
    /// Effective URL; its origin equals the template's literal origin.
    url: Url,
    /// Rendered authored headers, the credential, and `Idempotency-Key`.
    headers: HeaderMap,
    /// Rendered JSON body.
    body: Option<serde_json::Value>,
}

impl HttpRequest {
    /// Render the authored request against `context` and attach `credential`.
    ///
    /// The effective URL must keep the template's literal origin, so a
    /// rendered value can never re-point a credential.
    ///
    /// # Errors
    /// Returns a reason for an invalid template, URL, header, or credential.
    fn render(
        method: HttpMethod,
        url: &str,
        headers: &std::collections::BTreeMap<String, String>,
        body: Option<&serde_json::Value>,
        credential: Option<(&HttpAuth, &ConnectionSecret)>,
        context: &OperatorFailureContext,
    ) -> Result<Self, String> {
        let origin = operator_url_origin(url)?;
        let url = Url::parse(&context.render(url)?).map_err(|error| error.to_string())?;
        if HttpsOrigin::of_url(&url).ok().as_ref() != Some(&origin) {
            return Err("the rendered URL changed its origin".to_owned());
        }
        let mut rendered = HeaderMap::new();
        for (name, value) in headers {
            rendered.insert(
                HeaderName::try_from(name.as_str()).map_err(|error| error.to_string())?,
                HeaderValue::try_from(context.render(value)?).map_err(|error| error.to_string())?,
            );
        }
        match credential {
            None => {}
            Some((HttpAuth::Bearer { .. }, ConnectionSecret::Token { value })) => {
                rendered.insert(
                    reqwest::header::AUTHORIZATION,
                    bearer(value.expose()).map_err(|_| "the bearer token is not a header value")?,
                );
            }
            Some((HttpAuth::Basic { .. }, ConnectionSecret::Basic { username, password })) => {
                use base64::Engine as _;
                let pair = base64::engine::general_purpose::STANDARD.encode(format!(
                    "{}:{}",
                    username.expose(),
                    password.expose()
                ));
                rendered.insert(
                    reqwest::header::AUTHORIZATION,
                    sensitive(&format!("Basic {pair}"))
                        .map_err(|()| "the basic credential is not a header value")?,
                );
            }
            Some((HttpAuth::Header { name, .. }, ConnectionSecret::Token { value })) => {
                rendered.insert(
                    HeaderName::try_from(name.as_str()).map_err(|error| error.to_string())?,
                    sensitive(value.expose()).map_err(|()| "the header credential is invalid")?,
                );
            }
            Some(_) => return Err("the stored credential does not fit the auth scheme".to_owned()),
        }
        rendered.insert(
            HeaderName::from_static("idempotency-key"),
            HeaderValue::try_from(context.dispatch_id.to_string())
                .map_err(|error| error.to_string())?,
        );
        Ok(Self {
            method: match method {
                HttpMethod::Get => Method::GET,
                HttpMethod::Post => Method::POST,
                HttpMethod::Put => Method::PUT,
                HttpMethod::Patch => Method::PATCH,
                HttpMethod::Delete => Method::DELETE,
            },
            url,
            headers: rendered,
            body: body.map(|body| render_json(body, context)).transpose()?,
        })
    }
}

/// Every inline Operator body in `owner`'s verification bindings.
fn inline_operators(owner: &Spec) -> impl Iterator<Item = &OperatorSpec> {
    let bindings: Vec<&VerificationBinding> = match owner {
        Spec::Agent(agent) => agent.verified_by.iter().collect(),
        Spec::Service(service) => service
            .verified_by
            .iter()
            .chain(
                service
                    .components
                    .iter()
                    .flat_map(|component| &component.verified_by),
            )
            .collect(),
        _ => Vec::new(),
    };
    bindings
        .into_iter()
        .flat_map(|binding| &binding.on_failure)
        .filter_map(|operator| match operator {
            InlineableRef::Inline(body) => Some(&**body),
            _ => None,
        })
}

/// Render every string template inside a JSON body.
///
/// # Errors
/// Returns the first template error.
fn render_json(
    value: &serde_json::Value,
    context: &OperatorFailureContext,
) -> Result<serde_json::Value, String> {
    Ok(match value {
        serde_json::Value::String(text) => serde_json::Value::String(context.render(text)?),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| render_json(item, context))
                .collect::<Result<_, _>>()?,
        ),
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, item)| Ok((key.clone(), render_json(item, context)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

/// A sensitive `Bearer` authorization value.
///
/// # Errors
/// Returns a terminal [`CREDENTIAL_INVALID`] when the token is not a header value.
fn bearer(token: &str) -> Result<HeaderValue, Attempt> {
    sensitive(&format!("Bearer {token}")).map_err(|()| {
        Attempt::terminal(CREDENTIAL_INVALID, "the stored token is not a header value")
    })
}

/// A header value marked sensitive so HTTP tracing never prints it.
///
/// # Errors
/// Returns `()` when `value` is not a valid header value.
fn sensitive(value: &str) -> Result<HeaderValue, ()> {
    let mut value = HeaderValue::try_from(value).map_err(drop)?;
    value.set_sensitive(true);
    Ok(value)
}

/// Classify a non-success status, or `None` for 2xx.
///
/// 408, 429, and 5xx retry, honoring an integer `Retry-After`; every other
/// status is a terminal provider refusal.
fn status_failure(response: &Response) -> Option<Attempt> {
    let status = response.status();
    if status.is_success() {
        return None;
    }
    let message = format!("the destination answered {status}");
    Some(
        if status == StatusCode::REQUEST_TIMEOUT
            || status == StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error()
        {
            Attempt::Retry {
                error: failure(PROVIDER_TRANSIENT, &message),
                retry_after: response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map(Duration::from_secs),
            }
        } else {
            Attempt::terminal(PROVIDER_REJECTED, &message)
        },
    )
}

/// Classify a send that produced no response; the request may have been
/// delivered, so every such failure retries (at-least-once).
fn transport_failure(error: &reqwest::Error) -> Attempt {
    if error.is_builder() {
        Attempt::terminal(INVALID_REQUEST, "the request could not be built")
    } else if error.is_timeout() {
        Attempt::retry(ATTEMPT_TIMED_OUT, "the destination did not answer in time")
    } else {
        Attempt::retry(
            DESTINATION_UNREACHABLE,
            "the destination could not be reached",
        )
    }
}

/// Read at most [`MAX_RESPONSE_BYTES`] of a response body.
///
/// # Errors
/// Returns the transport error when the body stream fails.
async fn bounded_body(mut response: Response) -> Result<Vec<u8>, reqwest::Error> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let take = (MAX_RESPONSE_BYTES - body.len()).min(chunk.len());
        body.extend_from_slice(&chunk[..take]);
        if body.len() >= MAX_RESPONSE_BYTES {
            break;
        }
    }
    Ok(body)
}

/// `label` when the fenced settlement applied, otherwise `stale_lease`.
const fn fenced(applied: bool, label: &'static str) -> &'static str {
    if applied { label } else { "stale_lease" }
}

/// Log a delivery task that panicked; its lease expires into a reclaim.
fn reap(finished: Result<(), JoinError>) {
    if let Err(error) = finished
        && error.is_panic()
    {
        tracing::error!(%error, "operator delivery panicked; its lease will expire");
    }
}

/// A dispatch failure with a stable `code` and diagnostic `message`.
fn failure(code: &str, message: &str) -> VerificationError {
    VerificationError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}
