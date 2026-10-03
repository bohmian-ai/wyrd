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
//!
//! Key rewrap runs beside the claim loop, never inside it, under its own
//! cancellation and elapsed-time budgets, so a slow key provider cannot hold
//! back another tenant's due dispatch. HTTP credentials are attached only to
//! the request built from a client already screened and pinned to that
//! effective URL.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::{Client, Error as ReqwestError};
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use url::Url;
use wyrd_auth_oidc::{ScreenError, ScreenedHttp};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::SecretBearer;
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
use wyrd_sql::queries::operator_dispatches::{ClaimedDispatch, OperatorDispatchQueue};
use wyrd_sql::queries::verifier_runs::RetryOutcome;
use wyrd_sql::{OperatorPool, SqlError, TenantConn, WyrdPostgres};

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
use super::RuntimeLimits;
use super::claims::{ClaimLoop, LeasedWork, settled};
use super::health::RuntimeCapability;
use super::permits::OperatorPermits;
use crate::components::operators::keys::{KeyError, OperatorKeys};

mod pager_duty;
mod slack;

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

/// Largest provider response body read, in bytes.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
/// Same-origin redirects an HTTP Operator may follow in one attempt.
const MAX_REDIRECTS: usize = 3;

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
    /// Shared claim loop owning Operator execution capacity (separate from
    /// the Verifier pool), admission, and drain.
    claims: ClaimLoop,
    /// Tenant key-encryption keys for opening credentials.
    keys: Arc<OperatorKeys>,
    /// Provider wire adapters.
    delivery: OperatorDelivery,
    /// Runtime bounds.
    limits: RuntimeLimits,
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
            claims: ClaimLoop::new(
                postgres.clone(),
                Some(OperatorPermits::new(
                    limits.operator_global_permits,
                    limits.operator_tenant_permits,
                )),
                &limits,
            ),
            postgres,
            operator,
            queue: OperatorDispatchQueue::new(
                limits.operator_attempts,
                limits.operator_deadline,
                limits.operator_lease,
            ),
            keys,
            delivery,
            limits,
        }
    }

    /// Let `crash` panic this worker's loop.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_crash(mut self, crash: CapabilityCrash) -> Self {
        self.claims.arm_crash(crash);
        self
    }

    /// Claim and deliver dispatches until `stop` is cancelled, then drain,
    /// while rewrapping stale keys beside the claim loop.
    ///
    /// Delivery runs on the shared [`ClaimLoop`] exactly like the Verifier
    /// runner: each round claims one dispatch per due tenant with capacity;
    /// on `stop` no claim is admitted, in-flight attempts get the drain grace,
    /// and the rest are cancelled and released with their attempt refunded.
    /// Rewrap is polled concurrently in the same task, so a slow key provider
    /// delays only rewrap, and `stop` cancels it with the loop.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed an Operator-worker crash.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        tokio::join!(self.claims.run(&self, stop.clone()), self.rewrap(&stop));
    }

    /// Rewrap rows still on an older key version every rewrap interval until
    /// `stop`, each pass bounded by the runtime's pass and tenant budgets.
    ///
    /// Cancellation drops the pass in progress, rolling back its open tenant
    /// transaction; the next pass (here or on another replica) retries it.
    async fn rewrap(&self, stop: &CancellationToken) {
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => return,
                pass = self.keys.rewrap_pass(
                    &self.postgres,
                    &self.operator,
                    self.limits.rewrap_pass_budget,
                    self.limits.rewrap_tenant_budget,
                ) => {
                    if let Err(error) = pass {
                        tracing::warn!(%error, "operator key rewrap pass failed");
                    }
                }
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.limits.rewrap_interval) => {}
            }
        }
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
                KeyError::Unavailable { .. } | KeyError::Client | KeyError::VersionOutOfRange => {
                    store_down()
                }
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
            Attempt::Delivered => settled(self.queue.deliver(&mut conn, lease).await?, "delivered"),
            Attempt::Terminal(error) => {
                settled(self.queue.fail(&mut conn, lease, &error).await?, "failed")
            }
            Attempt::Release => settled(self.queue.release(&mut conn, lease).await?, "released"),
            Attempt::Retry { error, retry_after } => {
                let backoff = self.limits.operator_backoff(dispatch.attempt);
                let delay = retry_after.map_or(backoff, |after| after.max(backoff));
                match self.queue.retry(&mut conn, lease, &error, delay).await? {
                    RetryOutcome::Scheduled(_) => "retrying",
                    RetryOutcome::Exhausted => "failed",
                    RetryOutcome::StaleLease => "stale_lease",
                }
            }
        };
        conn.commit().await?;
        Ok(outcome)
    }
}

impl LeasedWork for OperatorWorker {
    type Claim = ClaimedDispatch;

    const CAPABILITY: RuntimeCapability = RuntimeCapability::OperatorWorker;
    const ACTIVE_GAUGE: Option<&'static str> =
        Some(crate::app::metrics::OPERATOR_ACTIVE_DISPATCHES);

    /// Every tenant with due dispatches, most overdue first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant read fails.
    async fn due_tenants(&self) -> Result<Vec<DataTenantId>, SqlError> {
        Ok(self.queue.due_tenants(&self.operator).await?)
    }

    /// Claim the tenant's next due dispatch under a fresh lease.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim fails.
    async fn claim(&self, conn: &mut TenantConn<'_>) -> Result<Option<ClaimedDispatch>, SqlError> {
        Ok(self.queue.claim(conn).await?)
    }

    /// Attempt one claimed dispatch and apply its single settlement.
    ///
    /// The claim loop holds its permit until settlement. `abandon` cancels the attempt and
    /// releases the lease; a retryable failure observed after `stop` is also
    /// released, since the process, not the dispatch, failed. The attempt
    /// runs in one `operator.dispatch` span naming the scrubbed dispatch and
    /// failed run identities, which relates it to that run's attempt trace.
    /// Delivery is bounded by its own permits, so it never reports a
    /// shared-resource refusal and always resolves to `false`.
    #[tracing::instrument(
        name = "operator.dispatch",
        skip_all,
        fields(dispatch_id = %dispatch.lease.dispatch_id, run_id = %dispatch.run_id, attempt = dispatch.attempt)
    )]
    async fn process(
        self: Arc<Self>,
        tenant: DataTenantId,
        dispatch: ClaimedDispatch,
        stop: CancellationToken,
        abandon: CancellationToken,
        _spawned_at: Instant,
    ) -> bool {
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
        false
    }

    /// Release a dispatch claimed after shutdown began, attempt refunded.
    async fn release_late(&self, tenant: DataTenantId, dispatch: &ClaimedDispatch) {
        if let Err(error) = self.settle(tenant, dispatch, Attempt::Release).await {
            tracing::error!(dispatch_id = %dispatch.lease.dispatch_id, %error, "releasing a late operator claim failed; the lease will expire");
        }
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
            ) => match slack::message(channel_id, text, context) {
                Ok(body) => self.slack(value, &body, timeout).await,
                Err(attempt) => attempt,
            },
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
                match pager_duty::event(
                    value,
                    route,
                    *severity,
                    summary,
                    dedup_key.as_deref(),
                    context,
                ) {
                    Ok(body) => self.pager_duty(&body, timeout).await,
                    Err(attempt) => attempt,
                }
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
                let request =
                    match HttpRequest::render(*method, url, headers, body.as_ref(), context) {
                        Ok(request) => request,
                        Err(reason) => return Attempt::terminal(INVALID_REQUEST, &reason),
                    };
                let credential = auth
                    .as_ref()
                    .zip(secret)
                    .map(|(auth, secret)| Credential { auth, secret });
                self.http(request, credential, *expect_status, timeout)
                    .await
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

    /// `chat.postMessage` with the bot token, attached after the endpoint is
    /// screened; a 2xx reply is classified by [`slack::outcome`].
    async fn slack(&self, token: &SecretBearer, body: &Value, timeout: Duration) -> Attempt {
        let response = match self
            .post_json(&self.endpoints.slack, Some(token), body, timeout)
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
        slack::outcome(&bytes)
    }

    /// Events API v2 enqueue; the routing key rides in the body.
    async fn pager_duty(&self, body: &Value, timeout: Duration) -> Attempt {
        match self
            .post_json(&self.endpoints.pager_duty, None, body, timeout)
            .await
        {
            Ok(response) => status_failure(&response).unwrap_or(Attempt::Delivered),
            Err(attempt) => attempt,
        }
    }

    /// Send an authored HTTP request, following at most [`MAX_REDIRECTS`]
    /// same-origin redirects.
    ///
    /// Each effective URL, initial or redirected, is resolved, screened, and
    /// pinned first; only then is the already-authorized `credential`
    /// rendered onto the request built from that screened client. A blocked,
    /// unresolved, or cross-origin destination therefore never sees a
    /// credential header built.
    async fn http(
        &self,
        mut request: HttpRequest,
        credential: Option<Credential<'_>>,
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
            if let Some(credential) = &credential {
                builder = match credential.attach(builder) {
                    Ok(builder) => builder,
                    Err(attempt) => return attempt,
                };
            }
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

    /// POST `body` as JSON to `url` through a screened client, attaching
    /// `bearer` as a sensitive `Authorization` header only after screening.
    ///
    /// # Errors
    /// Returns the classified attempt when screening, the bearer header, or
    /// the send fails.
    async fn post_json(
        &self,
        url: &Url,
        bearer_token: Option<&SecretBearer>,
        body: &Value,
        timeout: Duration,
    ) -> Result<Response, Attempt> {
        let mut builder = self.client(url, timeout).await?.post(url.clone());
        if let Some(token) = bearer_token {
            builder = builder.header(reqwest::header::AUTHORIZATION, bearer(token.expose())?);
            credential_attached();
        }
        builder
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
    async fn client(&self, url: &Url, timeout: Duration) -> Result<Client, Attempt> {
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

/// One rendered HTTP Operator request without its credential.
///
/// Holds only nonsecret state; the credential is added per screened send by
/// [`Credential::attach`].
struct HttpRequest {
    /// Method.
    method: Method,
    /// Effective URL; its origin equals the template's literal origin.
    url: Url,
    /// Rendered authored headers and `Idempotency-Key`.
    headers: HeaderMap,
    /// Rendered JSON body.
    body: Option<Value>,
}

impl HttpRequest {
    /// Render the authored request against `context`.
    ///
    /// The effective URL must keep the template's literal origin, so a
    /// rendered value can never re-point a credential. No credential is
    /// touched here.
    ///
    /// # Errors
    /// Returns a reason for an invalid template, URL, or header.
    fn render(
        method: HttpMethod,
        url: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&Value>,
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

/// An already-authorized HTTP credential not yet rendered into a header.
struct Credential<'a> {
    /// Authored auth scheme.
    auth: &'a HttpAuth,
    /// Decrypted secret of this attempt.
    secret: &'a ConnectionSecret,
}

impl Credential<'_> {
    /// Render the credential as a sensitive header onto `builder`, which
    /// must come from a client screened and pinned to its URL.
    ///
    /// # Errors
    /// Returns a terminal [`CREDENTIAL_INVALID`] when the secret does not fit
    /// the scheme or is not a valid header value.
    fn attach(&self, builder: RequestBuilder) -> Result<RequestBuilder, Attempt> {
        let invalid = |message: &str| Attempt::terminal(CREDENTIAL_INVALID, message);
        let (name, value) = match (self.auth, self.secret) {
            (HttpAuth::Bearer { .. }, ConnectionSecret::Token { value }) => {
                (reqwest::header::AUTHORIZATION, bearer(value.expose())?)
            }
            (HttpAuth::Basic { .. }, ConnectionSecret::Basic { username, password }) => {
                let pair = base64::engine::general_purpose::STANDARD.encode(format!(
                    "{}:{}",
                    username.expose(),
                    password.expose()
                ));
                (
                    reqwest::header::AUTHORIZATION,
                    sensitive(&format!("Basic {pair}"))
                        .map_err(|()| invalid("the basic credential is not a header value"))?,
                )
            }
            (HttpAuth::Header { name, .. }, ConnectionSecret::Token { value }) => (
                HeaderName::try_from(name.as_str())
                    .map_err(|_| invalid("the credential header name is invalid"))?,
                sensitive(value.expose())
                    .map_err(|()| invalid("the header credential is invalid"))?,
            ),
            _ => {
                return Err(invalid(
                    "the stored credential does not fit the auth scheme",
                ));
            }
        };
        credential_attached();
        Ok(builder.header(name, value))
    }
}

/// Count one credential attachment in unit tests; nothing in production.
#[cfg(test)]
fn credential_attached() {
    tests::ATTACHED.with(|attached| attached.set(attached.get() + 1));
}

/// Count one credential attachment in unit tests; nothing in production.
#[cfg(not(test))]
const fn credential_attached() {}

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
fn render_json(value: &Value, context: &OperatorFailureContext) -> Result<Value, String> {
    Ok(match value {
        Value::String(text) => Value::String(context.render(text)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| render_json(item, context))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
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
fn transport_failure(error: &ReqwestError) -> Attempt {
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
async fn bounded_body(mut response: Response) -> Result<Vec<u8>, ReqwestError> {
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

/// A dispatch failure with a stable `code` and diagnostic `message`.
fn failure(code: &str, message: &str) -> VerificationError {
    VerificationError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    //! HTTP credentials are rendered only onto requests built from a client
    //! already screened and pinned to that effective URL.

    use std::cell::Cell;

    use chrono::Utc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth_oidc::AddressPolicy;
    use wyrd_spec::card::operator::VerifierCounts;
    use wyrd_spec::ids::{
        BindingId, CardUid, OperatorDispatchId, VerificationResultId, VerificationRunId,
    };
    use wyrd_spec::verification::VerificationVerdict;

    use super::*;

    thread_local! {
        /// Credential headers built on this test thread.
        pub(super) static ATTACHED: Cell<usize> = const { Cell::new(0) };
    }

    /// Credential headers built on this thread so far.
    fn attached() -> usize {
        ATTACHED.with(Cell::get)
    }

    /// A frozen failure context fixture.
    fn context() -> OperatorFailureContext {
        OperatorFailureContext {
            dispatch_id: OperatorDispatchId::new_v7(),
            run_id: VerificationRunId::new_v7(),
            result_id: VerificationResultId::new_v7(),
            binding_id: BindingId::new_v7(),
            verifier_uid: CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 card uid"),
            verifier_ref: "verifier/drift@1.0.0".to_owned(),
            subject_uid: CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 card uid"),
            subject_ref: "service/owner@1.0.0".to_owned(),
            verdict: VerificationVerdict::Failed,
            completed_at: Utc::now(),
            summary: "failed".to_owned(),
            verifier: VerifierCounts::eval(7, 10),
        }
    }

    /// An HTTP JSON body renders every `eval.*` count as a string value from
    /// the frozen context, with the partial pass rate rounded down.
    #[test]
    fn eval_counts_render_into_a_json_body() {
        let body = serde_json::json!({
            "passed": "{{eval.passed_tasks}}",
            "counts": ["{{eval.failed_tasks}}", "{{eval.total_tasks}}"],
            "rate": "{{ eval.pass_rate_percent }}%",
        });
        assert_eq!(
            render_json(&body, &context()).expect("eval fields render"),
            serde_json::json!({ "passed": "7", "counts": ["3", "10"], "rate": "70%" })
        );
        let drift = serde_json::json!({ "n": "{{drift.total_features}}" });
        assert!(render_json(&drift, &context()).is_err());
    }

    /// An HTTP Operator posting to `url` with a header credential.
    ///
    /// # Panics
    /// Panics when the fixture spec does not parse.
    fn hook(url: &str) -> OperatorSpec {
        serde_json::from_value(serde_json::json!({
            "kind": "http", "method": "post", "url": url,
            "auth": { "scheme": "header", "name": "x-api-key", "connection": "hooks" },
        }))
        .expect("hook spec parses")
    }

    /// The header credential fixture.
    fn secret() -> ConnectionSecret {
        ConnectionSecret::Token {
            value: SecretBearer::new("hook-key".to_owned()),
        }
    }

    /// Deliver `url` under `policy` and return the attempt with the number
    /// of credential headers built for it.
    async fn deliver(policy: AddressPolicy, url: &str) -> (Attempt, usize) {
        let delivery =
            OperatorDelivery::new(ScreenedHttp::new(policy), ProviderEndpoints::default());
        let before = attached();
        let attempt = delivery
            .send(
                &hook(url),
                Some(&secret()),
                &context(),
                Duration::from_secs(5),
            )
            .await;
        (attempt, attached() - before)
    }

    /// A blocked or unresolved initial URL fails before any credential header
    /// is built or any request sent.
    ///
    /// # Panics
    /// Panics when a credential is attached or a request reaches the mock.
    #[tokio::test]
    async fn blocked_and_unresolved_destinations_never_get_a_credential() {
        let mock = MockServer::start().await;
        let (blocked, built) = deliver(
            AddressPolicy::BlockInternal,
            &format!("{}/hook", mock.uri()),
        )
        .await;
        assert!(
            matches!(&blocked, Attempt::Terminal(error) if error.code == DESTINATION_REJECTED),
            "{blocked:?}"
        );
        assert_eq!(built, 0, "a blocked destination gets no credential");
        let (unresolved, built) = deliver(
            AddressPolicy::AllowInternal,
            "https://wyrd-unresolvable.invalid/hook",
        )
        .await;
        assert!(
            matches!(&unresolved, Attempt::Retry { error, .. } if error.code == DESTINATION_UNREACHABLE),
            "{unresolved:?}"
        );
        assert_eq!(built, 0, "an unresolved destination gets no credential");
        assert!(mock.received_requests().await.expect("recorded").is_empty());
    }

    /// An allowed destination and each same-origin redirect are screened
    /// before their own credential attachment and receive the header; a
    /// cross-origin redirect is refused without attaching or sending.
    ///
    /// # Panics
    /// Panics when a credential count, header, or outcome differs.
    #[tokio::test]
    async fn each_screened_hop_attaches_its_own_credential() {
        let mock = MockServer::start().await;
        let other = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(307).insert_header("location", "/next"))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/next"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/away"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/stolen", other.uri())),
            )
            .mount(&mock)
            .await;
        let (followed, built) = deliver(
            AddressPolicy::AllowInternal,
            &format!("{}/start", mock.uri()),
        )
        .await;
        assert_eq!(followed, Attempt::Delivered);
        assert_eq!(built, 2, "each hop attaches after its own screen");
        let keys: Vec<_> = mock
            .received_requests()
            .await
            .expect("recorded")
            .iter()
            .map(|request| request.headers.get("x-api-key").cloned())
            .collect();
        assert_eq!(keys.len(), 2);
        assert!(
            keys.iter()
                .all(|key| key.as_ref().is_some_and(|key| key == "hook-key"))
        );

        let (refused, built) = deliver(
            AddressPolicy::AllowInternal,
            &format!("{}/away", mock.uri()),
        )
        .await;
        assert!(
            matches!(&refused, Attempt::Terminal(error) if error.code == DESTINATION_REJECTED),
            "{refused:?}"
        );
        assert_eq!(built, 1, "the refused redirect target gets no credential");
        assert!(
            other
                .received_requests()
                .await
                .expect("recorded")
                .is_empty()
        );
    }
}
