//! Tenant human OIDC connection authority.
//!
//! [`HumanConnections`] is the one owner of a tenant's human login trust. The
//! admin lifecycle (stage, test, activate, deactivate, remove) and the login
//! and callback read ([`HumanConnections::active_connection_for`]) all go
//! through it, and it reads only `wyrd.auth_human_connections`: workload trust
//! in `wyrd.auth_trusted_issuers` never authorizes a human login.
//!
//! Every mutation opens one tenant transaction, takes the tenant's connection
//! slot lock, appends the caller's already-evaluated canonical audit decision,
//! validates against the locked state, and writes. A refusal reached after the
//! decision commits the decision alone; a failed audit append aborts before any
//! write, so no change is ever durable without its audit row. Provider network
//! IO (the candidate probe) runs before that transaction and outside any lock.
//!
//! Human session issuance takes the same slot lock and requires the exact
//! connection revision a session is bound to to still be Active, so every
//! lifecycle mutation here also cuts off the sessions of the connection it
//! retires, on every replica.

use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::LOCATION;
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use url::Url;
use uuid::Uuid;
use wyrd_auth_oidc::{ClientAuth, OidcProvider, ScreenedHttp, TrustedIssuer, usable_jwks_keys};
use wyrd_crypt::SealingKeyring;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    ClaimMappingPayload, ConnectionActivate, ConnectionInput, HumanClientAuth,
    HumanConnectionState, HumanConnectionView, HumanConnectionsResponse, IssuerUrl,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_sql::queries::auth::{
    HumanConnectionWrite, deactivate_active_human_connection, human_candidate_test_is_current,
    human_connection_in_state, insert_human_candidate, list_service_account_roles,
    live_human_connections, lock_human_connection_slot, promote_tested_human_candidate,
    remove_human_connection, replace_human_candidate, stamp_human_candidate_tested,
};
use wyrd_sql::row_types::auth::{HumanConnectionBinding, HumanConnectionRow};
use wyrd_sql::{TenantConn, WyrdPostgres};

use crate::audit::append_auth_audit;
use crate::callback::{authorization_code_request, discover_provider};
use crate::error::{screen_error, store_error};
use crate::exchange_api_key::{ExchangeError, role_refs, verify_api_key};
use crate::issuance::resolve_permissions;
use crate::login::{auth_nonce, auth_state_key, build_authorization_url, pkce_verifier};
use crate::pg_resolvers::{client_auth_from_row, human_connection_trusted_issuer, seal_secret};

/// How long a successful candidate test authorizes activation.
pub const CONNECTION_TEST_VALIDITY: Duration = Duration::from_mins(15);

/// JWKS key-cache lifetime for a connection that inherits none.
const DEFAULT_JWKS_TTL_SECS: i64 = 300;

/// Path of the deployment's common provider callback on the public origin.
const CALLBACK_PATH: &str = "/auth/callback";

/// Provider error codes that mean the token endpoint refused Wyrd's client
/// authentication rather than the probe's deliberately bogus code.
const CLIENT_AUTH_REFUSALS: [&str; 2] = ["invalid_client", "unauthorized_client"];

/// The only token-endpoint error proving client authentication succeeded: the
/// provider authenticated the client and then rejected the bogus code.
const GRANT_REFUSAL: &str = "invalid_grant";

/// Standard OAuth 2.0 (RFC 6749 §4.1.2.1) and OpenID Connect Core (§3.1.2.6)
/// authorization error codes. A provider redirecting one of these to the
/// callback has accepted the callback as registered for the client, which is
/// what the non-interactive `prompt=none` probe needs to prove.
const AUTHORIZATION_ERRORS: [&str; 16] = [
    "invalid_request",
    "unauthorized_client",
    "access_denied",
    "unsupported_response_type",
    "invalid_scope",
    "server_error",
    "temporarily_unavailable",
    "interaction_required",
    "login_required",
    "account_selection_required",
    "consent_required",
    "invalid_request_uri",
    "invalid_request_object",
    "request_not_supported",
    "request_uri_not_supported",
    "registration_not_supported",
];

/// The tenant human-connection owner.
///
/// Holds the role-separated runtime store (every statement runs on an RLS
/// [`TenantConn`] acquired through [`WyrdPostgres`]), the deployment sealing
/// keyring, the screened provider HTTP capability, and the callback URL
/// derived from the configured public origin. Built once per server and
/// cheap to clone.
#[derive(Clone)]
pub struct HumanConnections {
    /// Runtime Postgres handle every tenant transaction is acquired from.
    postgres: WyrdPostgres,
    /// Sealing keyring for provider client secrets; `None` on a keyless
    /// deployment, which can then stage only public clients.
    keyring: Option<Arc<SealingKeyring>>,
    /// Screened, redirect-disabled, proxy-free HTTP capability every provider
    /// request goes through.
    http: ScreenedHttp,
    /// `{public_origin}/auth/callback`, or `None` without a public origin.
    callback_url: Option<Url>,
}

impl Debug for HumanConnections {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HumanConnections")
            .field("callback_url", &self.callback_url)
            .finish_non_exhaustive()
    }
}

/// A validated, sealed candidate ready for [`HumanConnections::put_candidate`].
#[derive(Debug)]
pub struct StagedCandidate {
    /// The candidate revision the caller expects to replace; `None` creates.
    expected_revision: Option<u64>,
    /// The sealed row write, its JWKS TTL filled in under the slot lock.
    write: HumanConnectionWrite,
}

/// The candidate a network probe runs against, with its opened secret.
struct TestTarget {
    /// Connection id of the probed candidate.
    connection_id: Uuid,
    /// Exact revision probed; the stamp lands only on this revision.
    revision: i64,
    /// Issuer the discovery document must name.
    issuer: IssuerUrl,
    /// OAuth client id presented to the provider.
    client_id: String,
    /// Opened client authentication, including any secret.
    client_auth: ClientAuth,
}

/// A candidate revision that passed every provider check and awaits its
/// stamp through [`HumanConnections::stamp_candidate`].
///
/// Only [`HumanConnections::probe_candidate`] produces one, so a stamp always
/// names the revision and JWKS URI a real probe proved.
#[derive(Debug)]
pub struct TestedCandidate {
    /// Connection id of the probed candidate.
    connection_id: Uuid,
    /// Exact revision the probe proved.
    revision: i64,
    /// JWKS URI discovered and proven usable during the probe.
    jwks_uri: Url,
}

/// The tenant's Active connection as human login and callback use it.
///
/// Carries both the verification-ready issuer and the exact connection id and
/// revision it was read at, so a login can bind its state to that revision and
/// the callback can require it to still be Active when it issues the session.
#[derive(Debug, Clone)]
pub struct ActiveHumanConnection {
    /// The issuer human ID tokens verify against.
    pub trusted: TrustedIssuer,
    /// The exact connection id and revision this issuer was read from.
    pub binding: HumanConnectionBinding,
}

impl HumanConnections {
    /// Build the owner over the runtime store, keyring, screened HTTP, and
    /// public origin.
    ///
    /// The callback URL is `{public_origin}/auth/callback`; `None` when the
    /// deployment configures no public origin, in which case staging, testing,
    /// and login refuse.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        keyring: Option<Arc<SealingKeyring>>,
        http: ScreenedHttp,
        public_origin: Option<&Url>,
    ) -> Self {
        Self {
            postgres,
            keyring,
            http,
            callback_url: public_origin.and_then(|origin| origin.join(CALLBACK_PATH).ok()),
        }
    }

    /// The exact redirect URI tenants register at their provider, if a public
    /// origin is configured.
    #[must_use]
    pub fn callback_url(&self) -> Option<&Url> {
        self.callback_url.as_ref()
    }

    /// The screened HTTP capability every provider request is made through.
    #[must_use]
    pub fn http(&self) -> ScreenedHttp {
        self.http
    }

    /// Read the tenant's Active and Candidate connections as redacted views.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuditUnavailable`] when the decision cannot be
    /// appended, [`WyrdError::AuthVerifyUnavailable`] when the store fails, and
    /// [`WyrdError::Internal`] when a stored row does not decode.
    pub async fn list(
        &self,
        tenant: DataTenantId,
        decision: &AuditEvent,
    ) -> Result<HumanConnectionsResponse, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        append_auth_audit(&mut conn, decision).await?;
        let rows = live_human_connections(&mut conn)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;

        let mut response = HumanConnectionsResponse {
            active: None,
            candidate: None,
            callback_url: self.callback_url.as_ref().map(Url::to_string),
        };
        for row in rows {
            let view = self.view(row)?;
            match view.state {
                HumanConnectionState::Active => response.active = Some(view),
                HumanConnectionState::Candidate => response.candidate = Some(view),
                HumanConnectionState::Inactive => {}
            }
        }
        Ok(response)
    }

    /// Prepare a candidate write without IO: require a public origin and seal
    /// the secret under the keyring's write key.
    ///
    /// `input` must come from [`ConnectionInput::from_json`], which is the one
    /// place its cross-field rules are checked; the durable table re-asserts
    /// the Public-has-no-secret invariant. Callers run this before
    /// authorizing, so every refusal it can produce is a deployment or input
    /// fact rather than an unaudited decision.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when no public origin is configured or
    /// a secret must be sealed without a keyring, and [`WyrdError::Internal`]
    /// when sealing or JSON encoding fails.
    pub fn stage(&self, input: ConnectionInput) -> Result<StagedCandidate, WyrdError> {
        self.require_callback()?;
        let client_secret_enc = match &input.client_secret {
            Some(secret) => {
                let keyring = self
                    .keyring
                    .as_deref()
                    .ok_or_else(|| WyrdError::Validation {
                        message: "a client secret was supplied but no sealing key is configured"
                            .to_owned(),
                        details: json!({ "reason": "sealing_key_missing" }),
                    })?;
                Some(seal_secret(keyring, secret.expose().as_bytes()).map_err(internal)?)
            }
            None => None,
        };
        Ok(StagedCandidate {
            expected_revision: input.expected_revision,
            write: HumanConnectionWrite {
                issuer_url: input.issuer.as_str().to_owned(),
                client_auth: input.client_auth.as_str().to_owned(),
                client_secret_enc,
                claim_mapping: serde_json::to_value(&input.claim_mapping).map_err(internal)?,
                group_role_map: serde_json::to_value(&input.group_role_map).map_err(internal)?,
                client_id: input.client_id,
                jwks_ttl_secs: DEFAULT_JWKS_TTL_SECS,
            },
        })
    }

    /// Stage a new candidate or replace the existing one at the next revision.
    ///
    /// A replaced candidate loses its test stamp. The JWKS TTL is inherited
    /// from the candidate being replaced, else from the Active connection.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when `expected_revision` does
    /// not match the current candidate (or is supplied when none exists), and
    /// the audit/store errors of [`Self::list`].
    pub async fn put_candidate(
        &self,
        tenant: DataTenantId,
        staged: StagedCandidate,
        decision: &AuditEvent,
    ) -> Result<HumanConnectionView, WyrdError> {
        let StagedCandidate {
            expected_revision,
            mut write,
        } = staged;
        let mut conn = self.begin_locked(tenant, decision).await?;
        let candidate =
            human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
                .await
                .map_err(store_error)?;
        let revision_matches = match (&candidate, expected_revision) {
            (Some(row), Some(expected)) => u64::try_from(row.revision).ok() == Some(expected),
            (None, None) => true,
            _ => false,
        };
        if !revision_matches {
            return commit_refusal(
                conn,
                conflict("expected_revision does not match the current candidate"),
            )
            .await;
        }
        write.jwks_ttl_secs = match &candidate {
            Some(row) => row.jwks_ttl_secs,
            None => human_connection_in_state(&mut conn, HumanConnectionState::Active.as_str())
                .await
                .map_err(store_error)?
                .map_or(DEFAULT_JWKS_TTL_SECS, |row| row.jwks_ttl_secs),
        };
        let row = match candidate {
            Some(existing) => replace_human_candidate(&mut conn, existing.connection_id, &write)
                .await
                .map_err(store_error)?
                .ok_or_else(|| internal("locked candidate vanished during replace"))?,
            None => insert_human_candidate(&mut conn, &write)
                .await
                .map_err(store_error)?,
        };
        conn.commit().await.map_err(store_error)?;
        self.view(row)
    }

    /// Prove the exact candidate revision against its provider, holding no
    /// transaction or lock while the screened network checks run.
    ///
    /// Checks, in order: discovery with issuer pinning; the JWKS document
    /// decoding to at least one key the verifier can use; a non-interactive
    /// (`prompt=none`) authorization request that the provider answers with a
    /// redirect to exactly the deployment callback, echoing the probe's state
    /// and carrying a code or a standard authorization error; and the token
    /// endpoint answering a deliberately invalid code with `invalid_grant`,
    /// which proves it authenticated the client first. Any other answer leaves
    /// the revision untested. The result feeds [`Self::stamp_candidate`] after
    /// the caller re-evaluates its decision.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when no candidate exists at
    /// `expected_revision`, [`WyrdError::ConnectionNotTested`] naming the
    /// failed check in `details.reason`, [`WyrdError::Validation`] when no
    /// public origin is configured, [`WyrdError::DiscoveryUnavailable`] when
    /// the provider is refused by address screening, unreachable, or failing,
    /// and the store errors of [`Self::list`]. Cancellation leaves nothing
    /// written.
    pub async fn probe_candidate(
        &self,
        tenant: DataTenantId,
        expected_revision: u64,
    ) -> Result<TestedCandidate, WyrdError> {
        let callback = self.require_callback()?;
        let target = self.test_target(tenant, expected_revision).await?;
        let provider = discover_provider(&target.issuer, self.http)
            .await
            .map_err(issuer_mismatch_untested)?;
        self.probe_jwks(&target, &provider).await?;
        self.probe_callback(&provider, &target, callback).await?;
        self.probe_client_auth(&provider, &target, callback).await?;
        Ok(TestedCandidate {
            connection_id: target.connection_id,
            revision: target.revision,
            jwks_uri: provider.metadata.jwks_uri,
        })
    }

    /// Stamp a probed candidate activatable for [`CONNECTION_TEST_VALIDITY`].
    ///
    /// `decision` is the caller's second, freshly evaluated allow decision,
    /// taken after the probe succeeded; it is appended in the stamp
    /// transaction under the slot lock, and the stamp lands only if the
    /// candidate is still at the probed revision.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when the candidate changed
    /// while it was probed, and the audit/store errors of [`Self::list`]. A
    /// failed audit append aborts before the stamp.
    pub async fn stamp_candidate(
        &self,
        tenant: DataTenantId,
        tested: TestedCandidate,
        decision: &AuditEvent,
    ) -> Result<HumanConnectionView, WyrdError> {
        let mut conn = self.begin_locked(tenant, decision).await?;
        let Some(row) = stamp_human_candidate_tested(
            &mut conn,
            tested.connection_id,
            tested.revision,
            tested.jwks_uri.as_str(),
            CONNECTION_TEST_VALIDITY,
        )
        .await
        .map_err(store_error)?
        else {
            return commit_refusal(
                conn,
                conflict("the candidate changed while it was being tested"),
            )
            .await;
        };
        conn.commit().await.map_err(store_error)?;
        self.view(row)
    }

    /// Swap the freshly tested candidate in as the tenant's Active connection.
    ///
    /// Under the slot lock: the candidate must exist at `expected_revision`
    /// with an unexpired stamp for that revision, and `recovery_api_key` must
    /// be a live API key of an active headless principal of this tenant that
    /// holds `identity_connections:write`. Only then is the previous Active
    /// retired and the candidate promoted, in the same transaction; sessions
    /// bound to the retired revision stop renewing once it commits.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] for a missing or stale
    /// candidate or an invalid recovery key, [`WyrdError::ConnectionNotTested`]
    /// when the stamp is missing or expired, and the audit/store errors of
    /// [`Self::list`].
    pub async fn activate(
        &self,
        tenant: DataTenantId,
        request: ConnectionActivate,
        decision: &AuditEvent,
    ) -> Result<HumanConnectionView, WyrdError> {
        let recovery_key = request.recovery_api_key.into_secret_string();
        let mut conn = self.begin_locked(tenant, decision).await?;
        let candidate =
            human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
                .await
                .map_err(store_error)?;
        let Some(candidate) = candidate
            .filter(|row| u64::try_from(row.revision).ok() == Some(request.expected_revision))
        else {
            return commit_refusal(conn, conflict("no candidate exists at expected_revision"))
                .await;
        };
        if !human_candidate_test_is_current(&mut conn, candidate.revision)
            .await
            .map_err(store_error)?
        {
            return commit_refusal(conn, not_tested("the candidate has no current test")).await;
        }
        if !recovery_key_authorizes(&mut conn, &recovery_key).await? {
            return commit_refusal(
                conn,
                conflict(
                    "recovery_api_key is not a live key of a headless principal of this tenant \
                     holding identity_connections:write",
                ),
            )
            .await;
        }
        let previous = deactivate_active_human_connection(&mut conn)
            .await
            .map_err(store_error)?;
        let Some(row) = promote_tested_human_candidate(&mut conn, candidate.revision)
            .await
            .map_err(store_error)?
        else {
            // Every promotion condition was checked under this lock; losing
            // one here means the store disagrees with itself. Dropping `conn`
            // rolls back the retirement and the decision together.
            return Err(internal("tested candidate could not be promoted"));
        };
        conn.commit().await.map_err(store_error)?;
        tracing::info!(
            tenant_id = %tenant,
            connection_id = %row.connection_id,
            retired = ?previous,
            revision = row.revision,
            "tenant human connection activated"
        );
        self.view(row)
    }

    /// Retire the Active connection; human login and every session bound to
    /// it stop on every replica once this commits.
    ///
    /// # Errors
    /// Returns [`WyrdError::NotFound`] when no Active connection exists, and
    /// the audit/store errors of [`Self::list`].
    pub async fn deactivate(
        &self,
        tenant: DataTenantId,
        decision: &AuditEvent,
    ) -> Result<Uuid, WyrdError> {
        let mut conn = self.begin_locked(tenant, decision).await?;
        let Some(retired) = deactivate_active_human_connection(&mut conn)
            .await
            .map_err(store_error)?
        else {
            return commit_refusal(conn, not_found("no active human connection")).await;
        };
        conn.commit().await.map_err(store_error)?;
        Ok(retired)
    }

    /// Tombstone one connection: it stops trusting logins and renewing its
    /// sessions, its secret is wiped, and its id remains for history.
    ///
    /// # Errors
    /// Returns [`WyrdError::NotFound`] when no live connection of this tenant
    /// has `connection_id`, and the audit/store errors of [`Self::list`].
    pub async fn remove(
        &self,
        tenant: DataTenantId,
        connection_id: Uuid,
        decision: &AuditEvent,
    ) -> Result<(), WyrdError> {
        let mut conn = self.begin_locked(tenant, decision).await?;
        if !remove_human_connection(&mut conn, connection_id)
            .await
            .map_err(store_error)?
        {
            return commit_refusal(conn, not_found("no live human connection has this id")).await;
        }
        conn.commit().await.map_err(store_error)
    }

    /// Resolve the tenant's Active connection, requiring it to be `issuer`.
    ///
    /// Read durably on every call, so every replica sees activation,
    /// replacement, and deactivation at once. Login state records the issuer
    /// the flow began against; if the tenant has since activated a different
    /// provider or deactivated login, the flow fails closed rather than
    /// trusting whichever provider is active now. The returned binding names
    /// the exact revision read, which session issuance re-checks under the
    /// slot lock.
    ///
    /// # Errors
    /// Returns [`WyrdError::InvalidToken`] when no Active connection exists or
    /// it names a different issuer, and [`WyrdError::AuthVerifyUnavailable`]
    /// when the store fails or the Active row cannot be opened (for example,
    /// its secret was sealed under a key this process does not hold).
    pub async fn active_connection_for(
        &self,
        tenant: DataTenantId,
        issuer: &IssuerUrl,
    ) -> Result<ActiveHumanConnection, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let row = human_connection_in_state(&mut conn, HumanConnectionState::Active.as_str())
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let not_active = || WyrdError::InvalidToken {
            message: "issuer is not the active login connection for the tenant".to_owned(),
            details: json!({}),
        };
        let row = row.ok_or_else(not_active)?;
        let binding = HumanConnectionBinding {
            connection_id: row.connection_id,
            connection_revision: row.revision,
        };
        let trusted = human_connection_trusted_issuer(tenant, row, self.keyring.as_deref())
            .map_err(|error| {
                tracing::error!(error = %error, tenant_id = %tenant, "active human connection is unusable");
                WyrdError::AuthVerifyUnavailable {
                    message: "tenant login connection is unavailable".to_owned(),
                    details: json!({ "retry_after_seconds": 1 }),
                }
            })?;
        if trusted.issuer != *issuer {
            return Err(not_active());
        }
        Ok(ActiveHumanConnection { trusted, binding })
    }

    /// Refuse when no public origin is configured.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] naming the missing public origin.
    pub fn require_callback(&self) -> Result<&Url, WyrdError> {
        self.callback_url
            .as_ref()
            .ok_or_else(|| WyrdError::Validation {
                message:
                    "the deployment has no public origin configured (WYRD_PUBLIC_ORIGIN), so no \
                      callback URL can be registered"
                        .to_owned(),
                details: json!({ "reason": "public_origin_missing" }),
            })
    }

    /// Open one RLS tenant transaction through the runtime store.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when no transaction can be
    /// acquired.
    async fn begin(&self, tenant: DataTenantId) -> Result<TenantConn<'_>, WyrdError> {
        self.postgres.tenant_conn(tenant).await.map_err(store_error)
    }

    /// Open a tenant transaction, take the slot lock, and append `decision`.
    ///
    /// # Errors
    /// Returns the store error when the transaction or lock fails, and
    /// [`WyrdError::AuditUnavailable`] when the append fails — before any
    /// mutation, so nothing becomes durable.
    async fn begin_locked(
        &self,
        tenant: DataTenantId,
        decision: &AuditEvent,
    ) -> Result<TenantConn<'_>, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        lock_human_connection_slot(&mut conn)
            .await
            .map_err(store_error)?;
        append_auth_audit(&mut conn, decision).await?;
        Ok(conn)
    }

    /// Read and open the candidate at `expected_revision` for a network probe.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when no candidate exists at
    /// that revision, the store errors of [`Self::begin`], and
    /// [`WyrdError::Internal`] when the stored row cannot be decoded or its
    /// secret opened.
    async fn test_target(
        &self,
        tenant: DataTenantId,
        expected_revision: u64,
    ) -> Result<TestTarget, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let row = human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let row = row
            .filter(|row| u64::try_from(row.revision).ok() == Some(expected_revision))
            .ok_or_else(|| conflict("no candidate exists at expected_revision"))?;
        let issuer = IssuerUrl::new(row.issuer_url.clone()).map_err(internal)?;
        let client_auth = client_auth_from_row(
            &row.client_auth,
            row.client_secret_enc.as_deref(),
            self.keyring.as_deref(),
        )
        .map_err(internal)?;
        Ok(TestTarget {
            connection_id: row.connection_id,
            revision: row.revision,
            issuer,
            client_id: row.client_id,
            client_auth,
        })
    }

    /// Require the discovered JWKS to decode to at least one usable key,
    /// through the same screened fetch and decoder the verifier uses.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionNotTested`] with reason `jwks_unusable`
    /// when the key set is unreachable, malformed, or holds no usable key.
    async fn probe_jwks(
        &self,
        target: &TestTarget,
        provider: &OidcProvider,
    ) -> Result<(), WyrdError> {
        match usable_jwks_keys(
            target.issuer.as_str(),
            &provider.metadata.jwks_uri,
            self.http,
        )
        .await
        {
            Ok(count) if count > 0 => Ok(()),
            outcome => {
                tracing::warn!(?outcome, "candidate provider JWKS is unusable");
                Err(not_tested_reason(
                    "jwks_unusable",
                    "the provider JWKS has no usable keys",
                ))
            }
        }
    }

    /// Ask the authorization endpoint to start a non-interactive login for
    /// this client and callback, and accept only an exact callback redirect.
    ///
    /// # Errors
    /// Returns [`WyrdError::DiscoveryUnavailable`] when the endpoint is
    /// refused by screening, unreachable, or answers 5xx, and
    /// [`WyrdError::ConnectionNotTested`] with reason `callback_rejected` for
    /// every other answer that is not a qualifying callback redirect.
    async fn probe_callback(
        &self,
        provider: &OidcProvider,
        target: &TestTarget,
        callback: &Url,
    ) -> Result<(), WyrdError> {
        let state = auth_state_key();
        let verifier = pkce_verifier();
        let mut url = build_authorization_url(
            &provider.metadata.authorization_endpoint,
            &target.client_id,
            callback.as_str(),
            &state,
            verifier.expose_secret(),
            &auth_nonce(),
        );
        url.query_pairs_mut().append_pair("prompt", "none");
        let client = self
            .http
            .client_for(&url)
            .await
            .map_err(|error| screen_error(&error))?;
        let response = client.get(url).send().await.map_err(unreachable)?;
        let status = response.status();
        if status.is_server_error() {
            return Err(unreachable("authorization endpoint failed"));
        }
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok());
        if callback_redirect_qualifies(status, location, callback, &state) {
            return Ok(());
        }
        Err(not_tested_reason(
            "callback_rejected",
            "the provider did not redirect a non-interactive login to the callback URL; \
             register the callback URL exactly as shown",
        ))
    }

    /// Present the client's authentication with a deliberately invalid code
    /// and require `invalid_grant`, the one answer proving the provider
    /// authenticated the client before judging the code.
    ///
    /// # Errors
    /// Returns [`WyrdError::DiscoveryUnavailable`] when the endpoint is
    /// refused by screening, unreachable, or answers 5xx, and
    /// [`WyrdError::ConnectionNotTested`] naming `token_endpoint_missing`,
    /// `client_auth_rejected`, or `client_auth_unverified`.
    async fn probe_client_auth(
        &self,
        provider: &OidcProvider,
        target: &TestTarget,
        callback: &Url,
    ) -> Result<(), WyrdError> {
        let Some(token_endpoint) = provider.metadata.token_endpoint.clone() else {
            return Err(not_tested_reason(
                "token_endpoint_missing",
                "the provider advertises no token endpoint",
            ));
        };
        let client = self
            .http
            .client_for(&token_endpoint)
            .await
            .map_err(|error| screen_error(&error))?;
        let code = format!("wyrd-connection-test-{}", auth_state_key());
        let verifier = pkce_verifier();
        let response = authorization_code_request(
            &client,
            token_endpoint,
            &target.client_id,
            &target.client_auth,
            callback.as_str(),
            &code,
            verifier.expose_secret(),
        )?
        .send()
        .await
        .map_err(unreachable)?;
        let status = response.status();
        if status.is_server_error() {
            return Err(unreachable("token endpoint failed"));
        }
        let body = response.bytes().await.map_err(unreachable)?;
        client_auth_outcome(status, &body)
    }

    /// Project a stored row to its redacted view.
    ///
    /// # Errors
    /// Returns [`WyrdError::Internal`] when a stored column does not decode.
    fn view(&self, row: HumanConnectionRow) -> Result<HumanConnectionView, WyrdError> {
        let claim_mapping: ClaimMappingPayload =
            serde_json::from_value(row.claim_mapping).map_err(internal)?;
        let group_role_map: HashMap<String, Vec<String>> =
            serde_json::from_value(row.group_role_map).map_err(internal)?;
        Ok(HumanConnectionView {
            id: row.connection_id,
            tenant_id: DataTenantId::new(row.data_tenant_id).map_err(internal)?,
            revision: u64::try_from(row.revision).map_err(internal)?,
            state: HumanConnectionState::parse(&row.state)?,
            issuer: row.issuer_url,
            client_id: row.client_id,
            client_auth: HumanClientAuth::parse(&row.client_auth)?,
            claim_mapping,
            group_role_map,
            jwks_ttl_secs: u64::try_from(row.jwks_ttl_secs).map_err(internal)?,
            tested_revision: row
                .tested_revision
                .map(u64::try_from)
                .transpose()
                .map_err(internal)?,
            tested_until: row.tested_until,
            created_at: row.created_at,
            updated_at: row.updated_at,
            callback_url: self.callback_url.as_ref().map(Url::to_string),
        })
    }
}

/// Whether an authorization-endpoint answer proves the callback is registered.
///
/// Qualifies only a redirect whose `Location` is an absolute URL with the
/// callback's exact origin and path, carrying exactly one `state` equal to
/// the probe's and either a nonempty `code` or one standard authorization
/// `error`. A provider only redirects to a URI registered for the client, so
/// that answer proves registration; a login page, a foreign or missing
/// location, a mismatched state, or an unknown error proves nothing.
fn callback_redirect_qualifies(
    status: StatusCode,
    location: Option<&str>,
    callback: &Url,
    state: &str,
) -> bool {
    if !status.is_redirection() {
        return false;
    }
    let Some(location) = location.and_then(|location| Url::parse(location).ok()) else {
        return false;
    };
    if location.origin() != callback.origin() || location.path() != callback.path() {
        return false;
    }
    let pairs: Vec<(String, String)> = location.query_pairs().into_owned().collect();
    let single = |name: &str| {
        let mut values = pairs
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str());
        match (values.next(), values.next()) {
            (Some(value), None) => Some(value),
            _ => None,
        }
    };
    let answered = single("code").is_some_and(|code| !code.is_empty())
        || single("error").is_some_and(|error| AUTHORIZATION_ERRORS.contains(&error));
    single("state") == Some(state) && answered
}

/// Judge the token endpoint's answer to the invalid-code probe.
///
/// Only an OAuth error body naming `invalid_grant` on a 4xx proves client
/// authentication; `invalid_client`, `unauthorized_client`, or a bare `401`
/// prove it failed; everything else — success, a malformed body, or any other
/// error — leaves it unproven.
///
/// # Errors
/// Returns [`WyrdError::ConnectionNotTested`] with reason
/// `client_auth_rejected` or `client_auth_unverified`.
fn client_auth_outcome(status: StatusCode, body: &[u8]) -> Result<(), WyrdError> {
    let error_code = status
        .is_client_error()
        .then(|| serde_json::from_slice::<Value>(body).ok())
        .flatten()
        .and_then(|body| body.get("error")?.as_str().map(str::to_owned));
    match error_code.as_deref() {
        Some(GRANT_REFUSAL) => Ok(()),
        Some(code) if CLIENT_AUTH_REFUSALS.contains(&code) => Err(client_auth_rejected()),
        _ if status == StatusCode::UNAUTHORIZED => Err(client_auth_rejected()),
        _ => Err(not_tested_reason(
            "client_auth_unverified",
            "the provider token endpoint did not answer the probe with invalid_grant, so this \
             client's authentication is unproven",
        )),
    }
}

/// The token endpoint refused this client's authentication.
fn client_auth_rejected() -> WyrdError {
    not_tested_reason(
        "client_auth_rejected",
        "the provider token endpoint rejected this client's authentication",
    )
}

/// Verify a recovery API key against this tenant, constant-cost on refusal.
///
/// Reuses the API-key exchange's verification, so the key must parse, name
/// this tenant, and match a live key row by Argon2 with every refusal still
/// running one verification; the owning principal must then be active and
/// hold permissions covering `identity_connections:write`.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when a store read fails,
/// [`WyrdError::RoleCorrupt`] when a stored role document does not decode, and
/// [`WyrdError::Internal`] when verification cannot run.
async fn recovery_key_authorizes(
    conn: &mut TenantConn<'_>,
    presented: &SecretString,
) -> Result<bool, WyrdError> {
    let row = match verify_api_key(conn, presented).await {
        Ok(row) => row,
        Err(ExchangeError::CrossTenant | ExchangeError::NotFound | ExchangeError::HashMismatch) => {
            return Ok(false);
        }
        Err(ExchangeError::Database(error)) => return Err(store_error(error)),
        Err(error) => return Err(internal(error)),
    };
    if row.status != "active" {
        return Ok(false);
    }
    let roles = list_service_account_roles(conn, row.principal_id)
        .await
        .map_err(store_error)?;
    let roles = role_refs(roles).map_err(internal)?;
    let permissions = resolve_permissions(conn, &roles)
        .await
        .map_err(WyrdError::from)?;
    Ok(permissions.contains(&Permission::identity_connections_write()))
}

/// Commit the already-appended decision alone, then return `refusal`.
///
/// # Errors
/// Always returns an error: `refusal`, or the store error when the commit
/// fails.
async fn commit_refusal<T>(conn: TenantConn<'_>, refusal: WyrdError) -> Result<T, WyrdError> {
    conn.commit().await.map_err(store_error)?;
    Err(refusal)
}

/// Report a discovery document naming a different issuer as a failed test
/// check; every other discovery failure keeps its unavailability error.
fn issuer_mismatch_untested(error: WyrdError) -> WyrdError {
    match error {
        WyrdError::DiscoveryUnavailable { details, .. }
            if details.get("reason").and_then(Value::as_str) == Some("issuer_mismatch") =>
        {
            not_tested_reason(
                "issuer_mismatch",
                "the provider discovery document names a different issuer",
            )
        }
        other => other,
    }
}

/// The provider could not be reached or failed; testing fails closed.
fn unreachable(error: impl Display) -> WyrdError {
    tracing::warn!(error = %error, "tenant connection provider unavailable");
    WyrdError::DiscoveryUnavailable {
        message: "the identity provider could not be reached".to_owned(),
        details: json!({}),
    }
}

/// A failed test check with a stable machine-readable reason.
fn not_tested_reason(reason: &str, message: &str) -> WyrdError {
    WyrdError::ConnectionNotTested {
        message: message.to_owned(),
        details: json!({ "reason": reason }),
    }
}

/// Activation without a current stamp.
fn not_tested(message: &str) -> WyrdError {
    not_tested_reason("test_missing_or_expired", message)
}

/// A revision, state, or recovery-key conflict.
fn conflict(message: &str) -> WyrdError {
    WyrdError::ConnectionConflict {
        message: message.to_owned(),
        details: json!({}),
    }
}

/// No such connection in this tenant.
fn not_found(message: &str) -> WyrdError {
    WyrdError::NotFound {
        message: message.to_owned(),
        details: json!({}),
    }
}

/// Map an unexpected server-side failure, logging the cause server-side only.
fn internal(cause: impl Display) -> WyrdError {
    tracing::error!(error = %cause, "tenant connection operation failed");
    WyrdError::Internal {
        message: "tenant connection operation failed".to_owned(),
        details: json!({}),
    }
}

/// Candidate-probe judgments: which provider answers qualify a revision.
#[cfg(test)]
mod probe_tests {
    use reqwest::StatusCode;
    use serde_json::json;
    use url::Url;
    use wyrd_spec::error::WyrdError;

    use super::{callback_redirect_qualifies, client_auth_outcome};

    /// The deployment callback the probes judge against.
    fn callback() -> Url {
        Url::parse("https://wyrd.example.com/auth/callback").expect("callback parses")
    }

    /// Judge one authorization answer against state `s1`.
    fn qualifies(status: u16, location: Option<&str>) -> bool {
        callback_redirect_qualifies(
            StatusCode::from_u16(status).expect("status is valid"),
            location,
            &callback(),
            "s1",
        )
    }

    /// The `details.reason` a probe refusal carries.
    fn reason(outcome: Result<(), WyrdError>) -> String {
        match outcome {
            Err(WyrdError::ConnectionNotTested { details, .. }) => details["reason"]
                .as_str()
                .expect("reason is a string")
                .to_owned(),
            other => panic!("expected a not-tested refusal, got {other:?}"),
        }
    }

    /// Only an exact callback redirect echoing the state with a code or a
    /// standard error qualifies.
    #[test]
    fn only_an_exact_state_matching_callback_redirect_qualifies() {
        let base = "https://wyrd.example.com/auth/callback";
        assert!(qualifies(302, Some(&format!("{base}?code=abc&state=s1"))));
        assert!(qualifies(
            303,
            Some(&format!("{base}?error=login_required&state=s1"))
        ));
        assert!(qualifies(
            302,
            Some(&format!("{base}?state=s1&error=consent_required"))
        ));

        let refused = [
            (200, None),
            (302, None),
            (400, Some(format!("{base}?code=abc&state=s1"))),
            (302, Some(format!("{base}?code=abc&state=other"))),
            (302, Some(format!("{base}?code=abc"))),
            (302, Some(format!("{base}?code=abc&state=s1&state=s1"))),
            (302, Some(format!("{base}?state=s1"))),
            (302, Some(format!("{base}?code=&state=s1"))),
            (302, Some(format!("{base}?error=made_up&state=s1"))),
            (
                302,
                Some("https://evil.example.com/auth/callback?code=a&state=s1".to_owned()),
            ),
            (
                302,
                Some("http://wyrd.example.com/auth/callback?code=a&state=s1".to_owned()),
            ),
            (
                302,
                Some("https://wyrd.example.com:8443/auth/callback?code=a&state=s1".to_owned()),
            ),
            (
                302,
                Some("https://wyrd.example.com/auth/callback/x?code=a&state=s1".to_owned()),
            ),
            (302, Some("/auth/callback?code=a&state=s1".to_owned())),
            (
                302,
                Some("https://idp.example.com/login?state=s1".to_owned()),
            ),
        ];
        for (status, location) in refused {
            assert!(
                !qualifies(status, location.as_deref()),
                "{status} {location:?} must not qualify"
            );
        }
    }

    /// Only `invalid_grant` proves client authentication; client errors are
    /// rejections and every other answer is unverified.
    #[test]
    fn only_invalid_grant_proves_client_authentication() {
        let body = |code: &str| json!({ "error": code }).to_string().into_bytes();
        assert!(client_auth_outcome(StatusCode::BAD_REQUEST, &body("invalid_grant")).is_ok());

        for (status, payload, expected) in [
            (
                StatusCode::BAD_REQUEST,
                body("invalid_client"),
                "client_auth_rejected",
            ),
            (
                StatusCode::UNAUTHORIZED,
                body("invalid_client"),
                "client_auth_rejected",
            ),
            (
                StatusCode::BAD_REQUEST,
                body("unauthorized_client"),
                "client_auth_rejected",
            ),
            (
                StatusCode::UNAUTHORIZED,
                b"not json".to_vec(),
                "client_auth_rejected",
            ),
            (
                StatusCode::OK,
                body("invalid_grant"),
                "client_auth_unverified",
            ),
            (
                StatusCode::OK,
                br#"{"id_token":"x"}"#.to_vec(),
                "client_auth_unverified",
            ),
            (
                StatusCode::BAD_REQUEST,
                b"not json".to_vec(),
                "client_auth_unverified",
            ),
            (
                StatusCode::BAD_REQUEST,
                body("invalid_request"),
                "client_auth_unverified",
            ),
            (
                StatusCode::BAD_REQUEST,
                br#"{"error":7}"#.to_vec(),
                "client_auth_unverified",
            ),
            (StatusCode::FOUND, Vec::new(), "client_auth_unverified"),
        ] {
            assert_eq!(
                reason(client_auth_outcome(status, &payload)),
                expected,
                "{status} {}",
                String::from_utf8_lossy(&payload)
            );
        }
    }
}
