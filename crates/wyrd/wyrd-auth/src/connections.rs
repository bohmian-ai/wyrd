//! Tenant human OIDC connection authority.
//!
//! [`HumanConnections`] is the one owner of a tenant's human login trust. The
//! admin lifecycle (stage, test, activate, deactivate, remove) and the login
//! and callback read ([`HumanConnections::active_connection`]) all go
//! through it, and it reads only `wyrd.auth_human_connections`: workload trust
//! in `wyrd.auth_trusted_issuers` never authorizes a human login.
//!
//! Every mutation opens one tenant transaction, takes the tenant's connection
//! slot lock, appends the caller's already-evaluated canonical audit decision,
//! validates against the locked state, and writes. A refusal reached after the
//! decision commits the decision alone; a failed audit append aborts before any
//! write, so no change is ever durable without its audit row. Provider network
//! IO (discovery and the signing-key check that begin a candidate test) runs
//! before any transaction and outside any lock.
//!
//! A candidate is tested by a real sign-in: [`HumanConnections::begin_test`]
//! writes one-use login state bound to the exact candidate revision and the
//! authorized caller, and the common callback redeems the provider's code and
//! verifies its ID token exactly as a login before
//! [`HumanConnections::stamp_test_sign_in`] marks only that revision tested.
//!
//! Human session issuance takes the same slot lock and requires the exact
//! connection revision a session is bound to to still be Active, so every
//! lifecycle mutation here also cuts off the sessions of the connection it
//! retires, on every replica.

use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use url::Url;
use uuid::Uuid;
use wyrd_auth_oidc::{OidcProvider, ScreenedHttp, TrustedIssuer, usable_jwks_keys};
use wyrd_crypt::SealingKeyring;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, ClaimMappingPayload, ConnectionActivate, ConnectionInput, ConnectionTestResponse,
    ConnectionTester, HumanClientAuth, HumanConnectionState, HumanConnectionView,
    HumanConnectionsResponse, IssuerUrl, LoginInitiation, PrincipalId, PrincipalKindTag, Sha256Hex,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_sql::queries::auth::{
    HumanConnectionWrite, LoginState, deactivate_active_human_connection,
    human_candidate_test_is_current, human_connection_in_state, insert_human_candidate,
    insert_login_state, list_service_account_roles, list_user_roles, live_human_connections,
    lock_human_connection_slot, promote_tested_human_candidate, remove_human_connection,
    replace_human_candidate, service_account_by_id, stamp_human_candidate_tested, user_by_id,
};
use wyrd_sql::row_types::auth::{HumanConnectionBinding, HumanConnectionRow};
use wyrd_sql::{TenantConn, WyrdPostgres};

use crate::audit::{append_auth_audit, principal_event, principal_kind_tag};
use crate::callback::{DiscoveryFailure, discover_provider};
use crate::error::store_error;
use crate::exchange_api_key::{ExchangeError, role_refs, verify_api_key};
use crate::issuance::resolve_permissions;
use crate::login::{
    LOGIN_COMPLETE_PATH, LOGIN_STATE_TTL, auth_nonce, auth_state_key,
    browser_authorization_endpoint, build_authorization_url, pkce_verifier,
};
use crate::pg_resolvers::{human_connection_trusted_issuer, seal_secret};

/// How long a successful candidate test authorizes activation.
pub const CONNECTION_TEST_VALIDITY: Duration = Duration::from_mins(15);

/// JWKS key-cache lifetime for a connection that inherits none.
const DEFAULT_JWKS_TTL_SECS: i64 = 300;

/// Path of the deployment's common provider callback on the public origin.
const CALLBACK_PATH: &str = "/auth/callback";

/// Audit operation of the decision a completed test sign-in records when it
/// re-checks its tester's authority before marking the candidate tested.
pub const CANDIDATE_TESTED_OPERATION: &str = "identity.oidc.candidate.tested";

/// Audit resource naming a tenant's connection slot, shared with the admin
/// routes' decisions.
const CONNECTION_RESOURCE: &str = "identity:oidc_connection";

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
    /// `{public_origin}/login/complete`, or `None` without a public origin.
    completion_url: Option<Url>,
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

/// The candidate revision a test sign-in is begun for.
struct TestTarget {
    /// The exact candidate connection id and revision; the test state binds
    /// it and the stamp lands only on this revision.
    binding: HumanConnectionBinding,
    /// Issuer the discovery document must name.
    issuer: IssuerUrl,
    /// OAuth client id the sign-in authorizes.
    client_id: String,
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
    /// The callback URL is `{public_origin}/auth/callback` and the browser
    /// completion URL `{public_origin}/login/complete`; both are `None` when
    /// the deployment configures no public origin, in which case staging,
    /// testing, and login refuse.
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
            completion_url: public_origin.and_then(|origin| origin.join(LOGIN_COMPLETE_PATH).ok()),
        }
    }

    /// The screened HTTP capability every provider request is made through;
    /// login and the callback exchange reuse it so their provider requests
    /// are screened exactly as candidate testing's are.
    #[must_use]
    pub(crate) fn http(&self) -> ScreenedHttp {
        self.http
    }

    /// The runtime Postgres handle this owner acquires tenant transactions
    /// from; login state and callback issuance share it so every human-login
    /// statement runs under the same role-separated store.
    #[must_use]
    pub(crate) fn postgres(&self) -> &WyrdPostgres {
        &self.postgres
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
    /// `input` must come from [`ConnectionInput::from_slice`], which is the one
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
                let keyring = self.require_keyring()?;
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

    /// Begin a real test sign-in for the exact candidate revision and return
    /// the provider authorization URL that completes it.
    ///
    /// Everything that can refuse without the network runs first: a public
    /// origin (the callback the sign-in returns to) and a sealing keyring (the
    /// callback refuses every login on a keyless deployment, which can never
    /// activate). The candidate must exist at `expected_revision`. Holding no
    /// transaction or lock, screened discovery must then name exactly the
    /// candidate's issuer, its JWKS must decode to at least one key the
    /// verifier can use, and the authorization endpoint must pass the
    /// deployment's scheme screen. Only then is one-use login state written,
    /// exactly as a login's — PKCE verifier, nonce, state hash, five-minute
    /// expiry, the deployment callback, and the candidate's issuer and client
    /// — but bound to this candidate revision and to `tester`, the caller
    /// whose authority the callback re-checks before it marks the revision
    /// tested. Nothing is marked tested here, and no URL is returned unless
    /// its state row is durable.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when no public origin or sealing
    /// keyring is configured, [`WyrdError::ConnectionConflict`] when no
    /// candidate exists at `expected_revision`,
    /// [`WyrdError::ConnectionNotTested`] with `details.reason`
    /// `issuer_mismatch` or `jwks_unusable`, [`WyrdError::DiscoveryUnavailable`]
    /// when the provider or its authorization endpoint is refused by
    /// screening or discovery fails, [`WyrdError::AuthVerifyUnavailable`]
    /// when the store fails, and [`WyrdError::Internal`] when the candidate
    /// row does not decode. Cancellation before the commit persists nothing.
    pub async fn begin_test(
        &self,
        tenant: DataTenantId,
        expected_revision: u64,
        tester: ConnectionTester,
    ) -> Result<ConnectionTestResponse, WyrdError> {
        let redirect_uri = self.require_callback()?.clone();
        self.require_keyring()?;
        let target = self.test_target(tenant, expected_revision).await?;
        let provider = discover_provider(&target.issuer, self.http)
            .await
            .map_err(|failure| match failure {
                DiscoveryFailure::IssuerMismatch => not_tested_reason(
                    "issuer_mismatch",
                    "the provider discovery document names a different issuer",
                ),
                DiscoveryFailure::Unavailable(error) => error,
            })?;
        self.require_usable_jwks(&target, &provider).await?;
        let authorization_endpoint = browser_authorization_endpoint(provider, self.http)?;
        let state_key = auth_state_key();
        let code_verifier = pkce_verifier();
        let nonce = auth_nonce();
        let authorization_url = build_authorization_url(
            &authorization_endpoint,
            &target.client_id,
            redirect_uri.as_str(),
            &state_key,
            code_verifier.expose_secret(),
            &nonce,
        );
        let authorization_url =
            AbsoluteUrl::new(authorization_url.as_str().to_owned()).map_err(internal)?;
        let row = LoginState {
            connection: target.binding,
            issuer: target.issuer.to_string(),
            client_id: target.client_id,
            redirect_uri: redirect_uri.to_string(),
            code_verifier,
            nonce,
            initiation: LoginInitiation::ConnectionTest(tester),
        };
        let mut conn = self.begin(tenant).await?;
        let state_hash = Sha256Hex::digest(state_key.as_bytes());
        if !insert_login_state(&mut conn, &state_hash, &row, LOGIN_STATE_TTL)
            .await
            .map_err(store_error)?
        {
            return Err(internal(
                "a fresh connection test state was already recorded",
            ));
        }
        conn.commit().await.map_err(store_error)?;
        Ok(ConnectionTestResponse { authorization_url })
    }

    /// The candidate a consumed test sign-in is bound to, as the trust its ID
    /// token verifies against; `None` when the tenant's candidate is no
    /// longer exactly the bound connection id, revision, issuer, and client.
    ///
    /// A candidate has no stored JWKS URI until it is tested, so the trust
    /// uses `jwks_uri` from the provider's fresh discovery: the URI the stamp
    /// then records.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails or
    /// the candidate row cannot be opened (for example, its secret was sealed
    /// under a key this process does not hold).
    pub(crate) async fn tested_candidate(
        &self,
        tenant: DataTenantId,
        login_state: &LoginState,
        jwks_uri: &Url,
    ) -> Result<Option<TrustedIssuer>, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let row = human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let Some(mut row) = row.filter(|row| {
            row.connection_id == login_state.connection.connection_id
                && row.revision == login_state.connection.connection_revision
        }) else {
            return Ok(None);
        };
        row.jwks_uri = Some(jwks_uri.to_string());
        let trusted = human_connection_trusted_issuer(tenant, row, self.keyring.as_deref())
            .map_err(|error| {
                tracing::error!(error = %error, tenant_id = %tenant, "tested candidate is unusable");
                WyrdError::AuthVerifyUnavailable {
                    message: "the tested connection is unavailable".to_owned(),
                    details: json!({ "retry_after_seconds": 1 }),
                }
            })?;
        Ok((trusted.issuer.as_str() == login_state.issuer
            && trusted.client_id == login_state.client_id)
            .then_some(trusted))
    }

    /// Mark the candidate revision a verified test sign-in is bound to tested
    /// for [`CONNECTION_TEST_VALIDITY`], recording `jwks_uri` as its key set.
    ///
    /// One tenant transaction takes the connection slot lock and re-checks the
    /// tester's authority from the store — still an active principal of this
    /// tenant whose stored roles grant `identity_connections:write` — then
    /// appends that Allowed or Denied [`CANDIDATE_TESTED_OPERATION`] decision.
    /// A denial commits the decision alone. An allowed decision stamps only
    /// the bound revision, and only while it is still the Candidate. Nothing
    /// else is written: no User, session, or credential.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when the tester is no
    /// longer authorized, [`WyrdError::ConnectionConflict`] when the candidate
    /// changed during the sign-in, [`WyrdError::AuditUnavailable`] when the
    /// decision cannot be appended — leaving the candidate untested —
    /// [`WyrdError::RoleCorrupt`] when a stored role does not decode, and the
    /// store errors of [`Self::list`].
    pub(crate) async fn stamp_test_sign_in(
        &self,
        tenant: DataTenantId,
        binding: HumanConnectionBinding,
        tester: ConnectionTester,
        jwks_uri: &Url,
        request_id: &str,
    ) -> Result<(), WyrdError> {
        let mut conn = self.begin(tenant).await?;
        lock_human_connection_slot(&mut conn)
            .await
            .map_err(store_error)?;
        let allowed = tester_authorized(&mut conn, tester).await?;
        let required = Permission::identity_connections_write();
        let decision = AuditEvent {
            resource: CONNECTION_RESOURCE.to_owned(),
            permission: required.to_string(),
            ..principal_event(
                request_id,
                CANDIDATE_TESTED_OPERATION,
                tester.principal_id,
                tester.principal_kind,
                None,
                if allowed {
                    AuditOutcome::Allowed
                } else {
                    AuditOutcome::Denied
                },
            )
        };
        append_auth_audit(&mut conn, &decision).await?;
        if !allowed {
            return commit_refusal(
                conn,
                WyrdError::PermissionDeniedRbac {
                    message: "the principal that began this connection test no longer holds \
                              identity_connections:write"
                        .to_owned(),
                    details: json!({ "required": required.to_string() }),
                },
            )
            .await;
        }
        if stamp_human_candidate_tested(
            &mut conn,
            binding.connection_id,
            binding.connection_revision,
            jwks_uri.as_str(),
            CONNECTION_TEST_VALIDITY,
        )
        .await
        .map_err(store_error)?
        .is_none()
        {
            return commit_refusal(
                conn,
                conflict("the candidate changed while it was being tested"),
            )
            .await;
        }
        conn.commit().await.map_err(store_error)
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
    /// Two decisions are audited on that transaction: the bearer caller's
    /// `decision`, appended when the lock is taken, and — whenever the
    /// recovery key resolves to an active principal — that principal's own
    /// Allowed or Denied decision on `identity_connections:write`, appended
    /// before promotion or the committed refusal.
    ///
    /// A keyless deployment is refused right after the lock and decision are
    /// taken, before any candidate or recovery-key read: the activated
    /// connection could not complete a human login, secretless providers
    /// included, because completions are sealed by the keyring. The refusal
    /// commits the caller's appended decision, so the evaluated permission
    /// is audited.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with reason `sealing_key_missing`
    /// when no sealing keyring is configured,
    /// [`WyrdError::ConnectionConflict`] for a missing or stale
    /// candidate or an invalid recovery key, [`WyrdError::ConnectionNotTested`]
    /// when the stamp is missing or expired, [`WyrdError::AuditUnavailable`]
    /// when either decision cannot be appended — leaving the Active and
    /// Candidate unchanged — and the store errors of [`Self::list`].
    pub async fn activate(
        &self,
        tenant: DataTenantId,
        request: ConnectionActivate,
        decision: &AuditEvent,
    ) -> Result<HumanConnectionView, WyrdError> {
        let mut conn = self.begin_locked(tenant, decision).await?;
        if let Err(refusal) = self.require_keyring() {
            return commit_refusal(conn, refusal).await;
        }
        let recovery_key = request.recovery_api_key.into_secret_string();
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
        if !recovery_key_authorizes(&mut conn, &recovery_key, decision).await? {
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

    /// Resolve the tenant's Active connection, or `None` when it has none.
    ///
    /// Read durably on every call, so every replica sees activation,
    /// replacement, and deactivation at once. The returned binding names the
    /// exact revision read; login state records it, the callback requires it to
    /// be unchanged, and session issuance re-checks it under the slot lock.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails or the
    /// Active row cannot be opened (for example, its secret was sealed under a
    /// key this process does not hold).
    pub async fn active_connection(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<ActiveHumanConnection>, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let row = human_connection_in_state(&mut conn, HumanConnectionState::Active.as_str())
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
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
        Ok(Some(ActiveHumanConnection { trusted, binding }))
    }

    /// The fixed same-origin BFF route a completed browser login is sent to.
    ///
    /// `{public_origin}/login/complete` (see [`LOGIN_COMPLETE_PATH`]): no query
    /// string and no capability, so the redirect carries nothing a browser,
    /// log, or referrer could replay.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] naming the missing public origin.
    pub fn completion_url(&self) -> Result<&Url, WyrdError> {
        self.completion_url
            .as_ref()
            .ok_or_else(public_origin_missing)
    }

    /// The deployment sealing keyring, required wherever this owner seals.
    ///
    /// A provider client secret is sealed before it is stored, and a completed
    /// human login is sealed until the BFF or CLI redeems it, so a keyless
    /// deployment can neither store a secret nor complete a human login, and
    /// refuses to begin one.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with reason `sealing_key_missing`
    /// when no sealing keyring is configured.
    pub(crate) fn require_keyring(&self) -> Result<&Arc<SealingKeyring>, WyrdError> {
        self.keyring.as_ref().ok_or_else(|| WyrdError::Validation {
            message: "a deployment sealing key (WYRD_SEALING_KEY_FILE) is required to seal \
                      provider client secrets and completed human logins at rest"
                .to_owned(),
            details: json!({ "reason": "sealing_key_missing" }),
        })
    }

    /// Refuse when no public origin is configured.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] naming the missing public origin.
    pub fn require_callback(&self) -> Result<&Url, WyrdError> {
        self.callback_url.as_ref().ok_or_else(public_origin_missing)
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

    /// Read the candidate at `expected_revision` a test sign-in is begun for.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when no candidate exists at
    /// that revision, the store errors of [`Self::begin`], and
    /// [`WyrdError::Internal`] when the stored issuer does not decode.
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
        Ok(TestTarget {
            binding: HumanConnectionBinding {
                connection_id: row.connection_id,
                connection_revision: row.revision,
            },
            issuer: IssuerUrl::new(row.issuer_url).map_err(internal)?,
            client_id: row.client_id,
        })
    }

    /// Require the discovered JWKS to decode to at least one usable key,
    /// through the same screened fetch and decoder the verifier uses.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionNotTested`] with reason `jwks_unusable`
    /// when the key set is unreachable, malformed, or holds no usable key.
    async fn require_usable_jwks(
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

/// Whether a test sign-in's tester is still authorized to mark a candidate
/// tested: an active principal of the RLS tenant whose stored roles grant
/// `identity_connections:write`.
///
/// A user's roles are its synced grants; a headless principal's are its
/// service-account grants and its stored kind must still match the recorded
/// one. A principal that is gone, inactive, or of a kind that holds no tenant
/// roles is not authorized.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when a store read fails,
/// [`WyrdError::RoleCorrupt`] when a stored role document does not decode, and
/// [`WyrdError::Internal`] when a stored role name is invalid.
async fn tester_authorized(
    conn: &mut TenantConn<'_>,
    tester: ConnectionTester,
) -> Result<bool, WyrdError> {
    let id = tester.principal_id.as_uuid();
    let roles = match tester.principal_kind {
        PrincipalKindTag::User => match user_by_id(conn, id).await.map_err(store_error)? {
            Some(user) if user.status == "active" => {
                list_user_roles(conn, id).await.map_err(store_error)?
            }
            _ => return Ok(false),
        },
        PrincipalKindTag::TenantAdmin | PrincipalKindTag::Service | PrincipalKindTag::Agent => {
            match service_account_by_id(conn, id).await.map_err(store_error)? {
                Some(row) if principal_kind_tag(&row.principal_kind) == tester.principal_kind => {
                    list_service_account_roles(conn, id)
                        .await
                        .map_err(store_error)?
                }
                _ => return Ok(false),
            }
        }
        PrincipalKindTag::GlobalAdmin | PrincipalKindTag::System => return Ok(false),
    };
    let roles = role_refs(roles).map_err(internal)?;
    let permissions = resolve_permissions(conn, &roles)
        .await
        .map_err(WyrdError::from)?;
    Ok(permissions.contains(&Permission::identity_connections_write()))
}

/// Verify a recovery API key against this tenant, constant-cost on refusal,
/// and audit the recovery principal's permission decision.
///
/// Reuses the API-key exchange's verification, so the key must parse, name
/// this tenant, and match a live key row by Argon2 with every refusal still
/// running one verification; the owning principal must then be active and
/// hold permissions covering `identity_connections:write`.
///
/// A key that resolves to an active principal reaches a real permission
/// decision, so one Allowed or Denied event attributed to that principal and
/// its verified credential id is appended on `conn`, sharing `bearer`'s
/// request, operation, and resource. Malformed, unknown, cross-tenant,
/// mismatched, and inactive keys resolve no decision and append nothing, so
/// their refusals stay indistinguishable.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when a store read fails,
/// [`WyrdError::RoleCorrupt`] when a stored role document does not decode,
/// [`WyrdError::AuditUnavailable`] when the decision cannot be appended, and
/// [`WyrdError::Internal`] when verification cannot run.
async fn recovery_key_authorizes(
    conn: &mut TenantConn<'_>,
    presented: &SecretString,
    bearer: &AuditEvent,
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
    let required = Permission::identity_connections_write();
    let allowed = permissions.contains(&required);
    let recovery = AuditEvent {
        card_ref: row.card_ref.map(|card_ref| card_ref.0),
        principal_id: PrincipalId::new(row.principal_id),
        principal_kind: principal_kind_tag(&row.principal_kind),
        credential_id: Some(row.api_key_id),
        permission: required.to_string(),
        outcome: if allowed {
            AuditOutcome::Allowed
        } else {
            AuditOutcome::Denied
        },
        detail: None,
        ..bearer.clone()
    };
    append_auth_audit(conn, &recovery).await?;
    Ok(allowed)
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

/// The deployment configures no public origin, so it has no callback or
/// completion URL.
fn public_origin_missing() -> WyrdError {
    WyrdError::Validation {
        message: "the deployment has no public origin configured (WYRD_PUBLIC_ORIGIN), so no \
                  callback URL can be registered"
            .to_owned(),
        details: json!({ "reason": "public_origin_missing" }),
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

/// Candidate activation against a real tenant store.
#[cfg(test)]
mod pg_tests {
    use url::Url;
    use uuid::Uuid;
    use wyrd_auth_oidc::ScreenedHttp;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::error::WyrdError;

    use super::HumanConnections;

    /// A keyless deployment cannot activate even a secretless (public-client)
    /// candidate: the refusal names the missing sealing key and happens
    /// before any candidate or recovery key is consulted. The caller's
    /// evaluated decision still commits as exactly one canonical staged row
    /// that carries no recovery-key material, and no connection is promoted.
    ///
    /// # Panics
    /// Panics when the fixture cannot start, activation is not refused, or
    /// the staged audit and connection state disagree with that contract.
    #[tokio::test]
    async fn activation_without_a_sealing_key_is_refused_for_a_secretless_provider() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let connections = HumanConnections::new(
            fixture.wyrd_postgres().clone(),
            None,
            ScreenedHttp::allowing_internal(),
            Some(&Url::parse("https://wyrd.example.com").expect("origin parses")),
        );
        let decision = crate::audit::principal_event(
            "req-activate",
            "identity.oidc.candidate.activate",
            wyrd_runtime::PrincipalId::new(Uuid::new_v4()),
            wyrd_spec::auth::PrincipalKindTag::User,
            None,
            super::AuditOutcome::Allowed,
        );
        let recovery_secret = "wyrd_recovery_never_staged";
        let request = wyrd_spec::auth::ConnectionActivate {
            expected_revision: 1,
            recovery_api_key: wyrd_spec::auth::SecretBearer::new(recovery_secret.to_owned()),
        };

        let outcome = connections
            .activate(fixture.data_tenant_id(), request, &decision)
            .await;
        match outcome {
            Err(WyrdError::Validation { details, .. }) => {
                assert_eq!(details["reason"], "sealing_key_missing");
            }
            other => panic!("expected sealing_key_missing, got {other:?}"),
        }

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let staged: Vec<String> = sqlx::query_scalar(
            "SELECT row_to_json(s)::text FROM vala.audit_staging s
              WHERE data_tenant_id = $1 AND operation = $2 AND outcome = 'allowed'",
        )
        .bind(fixture.data_tenant_id().as_uuid())
        .bind("identity.oidc.candidate.activate")
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("staged decisions read");
        assert_eq!(staged.len(), 1, "the allowed decision commits once");
        assert!(
            !staged[0].contains(recovery_secret),
            "the staged decision carries no recovery key"
        );
        let active = super::human_connection_in_state(
            &mut conn,
            wyrd_spec::auth::HumanConnectionState::Active.as_str(),
        )
        .await
        .expect("active connection reads");
        assert!(active.is_none(), "no connection is promoted");
    }
}
