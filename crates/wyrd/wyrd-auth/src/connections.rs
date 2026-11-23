//! Tenant human OIDC connection authority.
//!
//! [`HumanConnections`] is the one owner of a tenant's human login trust. The
//! admin lifecycle (stage, test, activate, deactivate, remove) and the login
//! and callback read ([`HumanConnections::active_connection`]) all go
//! through it, and it reads only `wyrd.auth_human_connections`: workload trust
//! in `wyrd.auth_trusted_issuers` never authorizes a human login.
//!
//! Every mutation opens one tenant transaction, takes the tenant's connection
//! slot lock, stages the caller's already-evaluated canonical audit decision on
//! the process audit stage, validates against the locked state, and writes.
//! Staging never waits for, or fails on, the audit commit. Provider network
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

use secrecy::SecretString;
use serde_json::json;
use url::Url;
use uuid::Uuid;
use wyrd_auth_oidc::{
    ProviderMetadata, RelyingParty, RelyingPartyError, ScreenedHttp, TrustedIssuer,
};
use wyrd_kms::{SecretPurpose, SecretScope, WyrdKms};
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    AbsoluteUrl, Attribution, ClaimMappingPayload, ConnectionActivate, ConnectionInput,
    ConnectionTestResponse, ConnectionTester, HumanClientAuth, HumanConnectionState,
    HumanConnectionView, HumanConnectionsResponse, IssuerUrl, LoginInitiation, PrincipalId,
    Sha256Hex,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome};
use wyrd_sql::queries::auth::{
    HumanConnectionWrite, LoginState, deactivate_active_human_connection,
    human_candidate_test_is_current, human_connection_in_state, insert_human_candidate,
    insert_login_state, live_human_connections, lock_human_connection_slot,
    promote_tested_human_candidate, remove_human_connection, renew_human_connection_secret,
    replace_human_candidate, stamp_human_candidate_tested,
};
use wyrd_sql::row_types::auth::{HumanConnectionBinding, HumanConnectionRow};
use wyrd_sql::{TenantConn, WyrdPostgres};

use wyrd_runtime::audit::AuditStage;

use crate::audit::{principal_event, principal_kind_tag};
use crate::authority::TenantAuthority;
use crate::error::{relying_party_error, store_error};
use crate::exchange_api_key::{ExchangeError, verify_api_key};
use crate::login::LOGIN_STATE_TTL;
use crate::pg_resolvers::{ClientSecretOwner, IssuerSealError, human_connection_trusted_issuer};

/// How long a successful candidate test authorizes activation.
pub const CONNECTION_TEST_VALIDITY: Duration = Duration::from_mins(15);

/// JWKS key-cache lifetime for a connection that inherits none.
const DEFAULT_JWKS_TTL_SECS: i64 = 300;

/// Path of the deployment's common provider callback on its public origin.
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
/// [`TenantConn`] acquired through [`WyrdPostgres`]), the deployment
/// stored-secret facade, and the human-login relying party. Built once per server and cheap to clone;
/// clones share the relying party's provider cache.
#[derive(Clone)]
pub struct HumanConnections {
    /// Runtime Postgres handle every tenant transaction is acquired from.
    postgres: WyrdPostgres,
    /// Stored-secret facade for provider client secrets; `None` without
    /// `[kms]`, which can then stage only public clients.
    kms: Option<Arc<WyrdKms>>,
    /// The human-login relying party; every provider request it makes goes
    /// through the screened, redirect-disabled, proxy-free HTTP capability.
    relying_party: RelyingParty,
    /// Process audit stage every connection decision is staged on.
    audit: Arc<dyn AuditStage>,
}

impl Debug for HumanConnections {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HumanConnections").finish_non_exhaustive()
    }
}

/// A validated, sealed candidate ready for [`HumanConnections::put_candidate`].
#[derive(Debug)]
pub struct StagedCandidate {
    /// The candidate revision the caller expects to replace; `None` creates.
    expected_revision: Option<u64>,
    /// The candidate id read while staging, `None` when there was none; the
    /// write lands only while the locked candidate is still this one.
    observed: Option<Uuid>,
    /// The id the secret is sealed to: the observed candidate's, else a
    /// fresh one the insert uses.
    connection_id: Uuid,
    /// The sealed row write, its JWKS TTL filled in under the slot lock.
    write: HumanConnectionWrite,
}

/// A connection secret resealed by [`HumanConnections::stage_renewal`], ready
/// for [`HumanConnections::renew_encryption`].
#[derive(Debug)]
pub struct StagedRenewal {
    /// The connection whose secret was renewed.
    connection_id: Uuid,
    /// The envelope read and opened; the swap lands only while it is stored.
    observed: Vec<u8>,
    /// The same secret resealed under a fresh data key.
    renewed: Vec<u8>,
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
    /// Build the owner over the runtime store, facade, screened HTTP, and
    /// the process audit stage its decisions are staged on.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        kms: Option<Arc<WyrdKms>>,
        http: ScreenedHttp,
        audit: Arc<dyn AuditStage>,
    ) -> Self {
        Self {
            postgres,
            kms,
            relying_party: RelyingParty::new(http),
            audit,
        }
    }

    /// The human-login relying party; login and the callback exchange reuse
    /// it so their provider requests are screened exactly as candidate
    /// testing's are and share one provider cache.
    #[must_use]
    pub(crate) fn relying_party(&self) -> &RelyingParty {
        &self.relying_party
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
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails, and
    /// [`WyrdError::Internal`] when a stored row does not decode.
    pub async fn list(
        &self,
        tenant: DataTenantId,
        decision: &AuditEvent,
        origin: &Url,
    ) -> Result<HumanConnectionsResponse, WyrdError> {
        let callback = provider_callback(origin)?;
        self.audit.stage(tenant, decision.clone());
        let mut conn = self.begin(tenant).await?;
        let rows = live_human_connections(&mut conn)
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;

        let mut response = HumanConnectionsResponse {
            active: None,
            candidate: None,
            callback_url: Some(callback.to_string()),
        };
        for row in rows {
            let view = view(row, &callback)?;
            match view.state {
                HumanConnectionState::Active => response.active = Some(view),
                HumanConnectionState::Candidate => response.candidate = Some(view),
                HumanConnectionState::Inactive => {}
            }
        }
        Ok(response)
    }

    /// Prepare `tenant`'s candidate write: read which candidate it replaces
    /// in one short transaction, then, holding none, seal the secret into the
    /// common envelope bound to the tenant, issuer, client id, method, and
    /// that candidate's id (or a fresh id for a new candidate).
    ///
    /// `input` must come from [`ConnectionInput::from_slice`], which is the one
    /// place its cross-field rules are checked; the durable table re-asserts
    /// the Public-has-no-secret invariant. Callers run this after authorizing
    /// and before [`Self::put_candidate`] opens its write transaction, so a
    /// refusal here follows a staged decision and a key-service wait holds no
    /// lock.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when a secret must be sealed without
    /// `[kms]`, [`WyrdError::ServiceUnavailable`] when sealing fails,
    /// [`WyrdError::AuthVerifyUnavailable`] when the store fails, and
    /// [`WyrdError::Internal`] when JSON encoding fails.
    pub async fn stage(
        &self,
        tenant: DataTenantId,
        input: ConnectionInput,
    ) -> Result<StagedCandidate, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let observed =
            human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
                .await
                .map_err(store_error)?
                .map(|row| row.connection_id);
        conn.commit().await.map_err(store_error)?;
        let connection_id = observed.unwrap_or_else(Uuid::now_v7);
        let owner_id = connection_id.to_string();
        let client_secret_enc = match &input.client_secret {
            Some(secret) => {
                let kms = self.require_kms()?;
                let owner = ClientSecretOwner {
                    scope: SecretScope::Tenant(tenant),
                    purpose: SecretPurpose::AuthHumanConnection,
                    issuer_url: input.issuer.as_str(),
                    client_id: &input.client_id,
                    connection_id: Some(&owner_id),
                };
                Some(
                    owner
                        .seal_secret(
                            input.client_auth.as_str(),
                            secret.expose().as_bytes(),
                            Some(kms),
                        )
                        .await
                        .map_err(|_: IssuerSealError| WyrdError::ServiceUnavailable {
                            message: "provider client secret protection is unavailable".to_owned(),
                            details: json!({ "reason": "kms_unavailable" }),
                        })?,
                )
            }
            None => None,
        };
        Ok(StagedCandidate {
            expected_revision: input.expected_revision,
            observed,
            connection_id,
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
    /// not match the current candidate (or is supplied when none exists) or
    /// the candidate is no longer the one [`Self::stage`] read, and the
    /// audit/store errors of [`Self::list`].
    pub async fn put_candidate(
        &self,
        tenant: DataTenantId,
        staged: StagedCandidate,
        decision: &AuditEvent,
        origin: &Url,
    ) -> Result<HumanConnectionView, WyrdError> {
        let callback = provider_callback(origin)?;
        let StagedCandidate {
            expected_revision,
            observed,
            connection_id,
            mut write,
        } = staged;
        let mut conn = self.begin_locked(tenant, decision).await?;
        let candidate =
            human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
                .await
                .map_err(store_error)?;
        let revision_matches = candidate.as_ref().map(|row| row.connection_id) == observed
            && match (&candidate, expected_revision) {
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
            None => insert_human_candidate(&mut conn, connection_id, &write)
                .await
                .map_err(store_error)?,
        };
        conn.commit().await.map_err(store_error)?;
        view(row, &callback)
    }

    /// Read live connection `connection_id`'s sealed client secret in one
    /// short transaction, then, holding none, renew its encryption: the same
    /// secret resealed under a fresh data key bound to the same tenant,
    /// issuer, client id, method, and connection id.
    ///
    /// Callers run this after authorizing and before
    /// [`Self::renew_encryption`], as [`Self::stage`] precedes
    /// [`Self::put_candidate`].
    ///
    /// # Errors
    /// Returns [`WyrdError::NotFound`] when this tenant has no Active or
    /// Candidate connection with `connection_id`,
    /// [`WyrdError::ConnectionConflict`] when it holds no client secret,
    /// [`WyrdError::ServiceUnavailable`] when `[kms]` is missing or the
    /// secret cannot be opened or resealed, and the store errors of
    /// [`Self::list`].
    pub async fn stage_renewal(
        &self,
        tenant: DataTenantId,
        connection_id: Uuid,
    ) -> Result<StagedRenewal, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let row = live_human_connections(&mut conn)
            .await
            .map_err(store_error)?
            .into_iter()
            .find(|row| {
                row.connection_id == connection_id
                    && row.state != HumanConnectionState::Inactive.as_str()
            })
            .ok_or_else(|| not_found("no active or candidate human connection has this id"))?;
        conn.commit().await.map_err(store_error)?;
        let observed = row
            .client_secret_enc
            .ok_or_else(|| conflict("the connection holds no client secret"))?;
        let owner_id = connection_id.to_string();
        let renewed = ClientSecretOwner {
            scope: SecretScope::Tenant(tenant),
            purpose: SecretPurpose::AuthHumanConnection,
            issuer_url: &row.issuer_url,
            client_id: &row.client_id,
            connection_id: Some(&owner_id),
        }
        .renew_secret(&row.client_auth, &observed, self.kms.as_deref())
        .await
        .map_err(|_: IssuerSealError| WyrdError::ServiceUnavailable {
            message: "provider client secret protection is unavailable".to_owned(),
            details: json!({ "reason": "kms_unavailable" }),
        })?;
        Ok(StagedRenewal {
            connection_id,
            observed,
            renewed,
        })
    }

    /// Store a renewal staged by [`Self::stage_renewal`]. The connection
    /// keeps its id, revision, state, and test stamp, so login and every
    /// session bound to it continue unchanged.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when the connection was
    /// deactivated, removed, replaced, or renewed after it was read, and the
    /// audit/store errors of [`Self::list`].
    pub async fn renew_encryption(
        &self,
        tenant: DataTenantId,
        staged: StagedRenewal,
        decision: &AuditEvent,
        origin: &Url,
    ) -> Result<HumanConnectionView, WyrdError> {
        let callback = provider_callback(origin)?;
        let mut conn = self.begin_locked(tenant, decision).await?;
        let Some(row) = renew_human_connection_secret(
            &mut conn,
            staged.connection_id,
            &staged.observed,
            &staged.renewed,
        )
        .await
        .map_err(store_error)?
        else {
            return commit_refusal(conn, conflict("the connection changed during the renewal"))
                .await;
        };
        conn.commit().await.map_err(store_error)?;
        view(row, &callback)
    }

    /// Begin a real test sign-in for the exact candidate revision and return
    /// the provider authorization URL that completes it.
    ///
    /// The sign-in returns to the callback on `origin`. The candidate must exist at `expected_revision`. Holding no
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
    /// Returns [`WyrdError::ConnectionConflict`] when no candidate exists at
    /// `expected_revision`,
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
        origin: &Url,
    ) -> Result<ConnectionTestResponse, WyrdError> {
        let redirect_uri = provider_callback(origin)?;
        let target = self.test_target(tenant, expected_revision).await?;
        let provider = self
            .relying_party
            .discover(&target.issuer)
            .await
            .map_err(|error| match error {
                RelyingPartyError::IssuerMismatch => not_tested_reason(
                    "issuer_mismatch",
                    "the provider discovery document names a different issuer",
                ),
                error => relying_party_error(error),
            })?;
        require_usable_jwks(&provider)?;
        let authorization = self
            .relying_party
            .authorize(&provider, &target.client_id, redirect_uri.as_str())
            .map_err(relying_party_error)?;
        let authorization_url =
            AbsoluteUrl::new(authorization.url.as_str().to_owned()).map_err(internal)?;
        let row = LoginState {
            connection: target.binding,
            issuer: target.issuer.to_string(),
            client_id: target.client_id,
            redirect_uri: redirect_uri.to_string(),
            code_verifier: authorization.code_verifier,
            nonce: authorization.nonce,
            initiation: LoginInitiation::ConnectionTest(tester),
        };
        let mut conn = self.begin(tenant).await?;
        let state_hash = Sha256Hex::digest(authorization.state.as_bytes());
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
        let trusted = human_connection_trusted_issuer(tenant, row, self.kms.as_deref())
            .await
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
    /// stages that Allowed or Denied [`CANDIDATE_TESTED_OPERATION`] decision.
    /// A denial writes nothing. An allowed decision stamps only
    /// the bound revision, and only while it is still the Candidate. Nothing
    /// else is written: no User, session, or credential.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when the tester is no
    /// longer authorized, [`WyrdError::ConnectionConflict`] when the candidate
    /// changed during the sign-in, [`WyrdError::RoleCorrupt`] when a stored role does not decode, and the
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
                Attribution::direct(tester.principal_id),
                tester.principal_kind,
                None,
                if allowed {
                    AuditOutcome::Allowed
                } else {
                    AuditOutcome::Denied
                },
            )
        };
        self.audit.stage(tenant, decision);
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
    /// Two decisions are staged on the audit stage: the bearer caller's
    /// `decision`, staged when the lock is taken, and — whenever the
    /// recovery key resolves to an active principal — that principal's own
    /// Allowed or Denied decision on `identity_connections:write`, staged
    /// before promotion or the refusal.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] for a missing or stale
    /// candidate or an invalid recovery key, [`WyrdError::ConnectionNotTested`]
    /// when the stamp is missing or expired, and the store errors of
    /// [`Self::list`].
    pub async fn activate(
        &self,
        tenant: DataTenantId,
        request: ConnectionActivate,
        decision: &AuditEvent,
        origin: &Url,
    ) -> Result<HumanConnectionView, WyrdError> {
        let callback = provider_callback(origin)?;
        let mut conn = self.begin_locked(tenant, decision).await?;
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
        if !recovery_key_authorizes(&mut conn, &*self.audit, &recovery_key, decision).await? {
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
        view(row, &callback)
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

    /// What `tenant` contributes to its sign-in methods: whether it has a
    /// usable Active company connection and whether it requires company SSO.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails or
    /// the Active connection cannot be opened.
    pub async fn tenant_login(
        &self,
        tenant: DataTenantId,
    ) -> Result<crate::social::TenantLogin, WyrdError> {
        let company_sso = self.active_connection(tenant).await?.is_some();
        let mut conn = self.begin(tenant).await?;
        let requires_company_sso =
            wyrd_sql::queries::auth::login_policy::requires_company_sso(&mut conn)
                .await
                .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        Ok(crate::social::TenantLogin {
            company_sso,
            requires_company_sso,
        })
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
        let trusted = human_connection_trusted_issuer(tenant, row, self.kms.as_deref())
            .await
            .map_err(|error| {
                tracing::error!(error = %error, tenant_id = %tenant, "active human connection is unusable");
                WyrdError::AuthVerifyUnavailable {
                    message: "tenant login connection is unavailable".to_owned(),
                    details: json!({ "retry_after_seconds": 1 }),
                }
            })?;
        Ok(Some(ActiveHumanConnection { trusted, binding }))
    }

    /// The deployment stored-secret facade, required wherever this owner
    /// seals.
    ///
    /// A provider client secret is sealed before it is stored, so a
    /// deployment without `[kms]` can stage only public-client connections.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] with reason `kms_unconfigured` when
    /// no `[kms]` is configured.
    pub(crate) fn require_kms(&self) -> Result<&WyrdKms, WyrdError> {
        self.kms.as_deref().ok_or_else(|| WyrdError::Validation {
            message: "stored-secret encryption ([kms], WYRD_KMS_*) is required to store \
                      provider client secrets"
                .to_owned(),
            details: json!({ "reason": "kms_unconfigured" }),
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

    /// Stage `decision` on the audit stage, then open a tenant transaction
    /// and take the slot lock.
    ///
    /// The decision is staged first so a transaction or lock failure still
    /// records the allowance it followed.
    ///
    /// # Errors
    /// Returns the store error when the transaction or lock fails.
    async fn begin_locked(
        &self,
        tenant: DataTenantId,
        decision: &AuditEvent,
    ) -> Result<TenantConn<'_>, WyrdError> {
        self.audit.stage(tenant, decision.clone());
        let mut conn = self.begin(tenant).await?;
        lock_human_connection_slot(&mut conn)
            .await
            .map_err(store_error)?;
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
}

/// Project a stored row to its redacted view, naming `callback` as the
/// redirect URI to register at the provider.
///
/// # Errors
/// Returns [`WyrdError::Internal`] when a stored column does not decode.
fn view(row: HumanConnectionRow, callback: &Url) -> Result<HumanConnectionView, WyrdError> {
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
        callback_url: Some(callback.to_string()),
    })
}

/// The provider callback for a request that arrived at `origin`:
/// `{origin}/auth/callback`, the address the browser itself used.
///
/// A forged request origin only redirects its own sender, and the
/// provider accepts only the redirect URIs registered with it.
///
/// # Errors
/// Returns [`WyrdError::Validation`] when `origin` cannot carry a path.
pub fn provider_callback(origin: &Url) -> Result<Url, WyrdError> {
    origin
        .join(CALLBACK_PATH)
        .map_err(|error| WyrdError::Validation {
            message: format!("the request origin cannot form a callback URL: {error}"),
            details: json!({ "reason": "request_origin_invalid" }),
        })
}

/// Whether a test sign-in's tester is still authorized to mark a candidate
/// tested: an active principal of the RLS tenant whose current bindings grant
/// `identity_connections:write`.
///
/// The account's stored kind must still match the recorded one. A principal
/// that is gone, inactive, or of a kind that holds no tenant bindings is not
/// authorized.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when a store read fails,
/// [`WyrdError::RoleCorrupt`] when a stored role document does not decode, and
/// [`WyrdError::Internal`] when a stored role name is invalid.
async fn tester_authorized(
    conn: &mut TenantConn<'_>,
    tester: ConnectionTester,
) -> Result<bool, WyrdError> {
    let grants =
        TenantAuthority::current_grants(conn, tester.principal_id, tester.principal_kind).await?;
    Ok(grants.is_some_and(|grants| grants.contains(&Permission::identity_connections_write())))
}

/// Verify a recovery API key against this tenant, constant-cost on refusal,
/// and audit the recovery principal's permission decision.
///
/// Reuses the API-key exchange's verification, so the key must parse, name
/// this tenant, and match a live key row's SHA-256 verifier; the owning
/// principal must then be active and
/// hold permissions covering `identity_connections:write`.
///
/// A key that resolves to an active principal reaches a real permission
/// decision, so one Allowed or Denied event attributed to that principal and
/// its verified credential id is staged on `audit` for `conn`'s tenant,
/// sharing `bearer`'s request, operation, and resource. Malformed, unknown,
/// cross-tenant, mismatched, and inactive keys resolve no decision and stage
/// nothing, so their refusals stay indistinguishable.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when a store read fails,
/// [`WyrdError::RoleCorrupt`] when a stored role document does not decode,
/// and [`WyrdError::Internal`] when verification cannot run.
async fn recovery_key_authorizes(
    conn: &mut TenantConn<'_>,
    audit: &dyn AuditStage,
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
    let grants = TenantAuthority::current_grants(
        conn,
        PrincipalId::new(row.principal_id),
        principal_kind_tag(&row.principal_kind),
    )
    .await?;
    let required = Permission::identity_connections_write();
    let allowed = grants.is_some_and(|grants| grants.contains(&required));
    let recovery = AuditEvent {
        card_ref: None,
        sub_principal_id: PrincipalId::new(row.principal_id),
        sub_principal_kind: principal_kind_tag(&row.principal_kind),
        act_principal_id: PrincipalId::new(row.principal_id),
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
    audit.stage(conn.data_tenant_id(), recovery);
    Ok(allowed)
}

/// Commit the refused transaction, which wrote nothing, then return `refusal`.
///
/// # Errors
/// Always returns an error: `refusal`, or the store error when the commit
/// fails.
async fn commit_refusal<T>(conn: TenantConn<'_>, refusal: WyrdError) -> Result<T, WyrdError> {
    conn.commit().await.map_err(store_error)?;
    Err(refusal)
}

/// Require the key set a candidate test's fresh discovery fetched to hold at
/// least one key, so a connection whose provider publishes no signing key is
/// never marked tested.
///
/// `openidconnect` already dropped any key it could not decode while
/// discovering, so an empty set means no key the ID-token verifier could use.
///
/// # Errors
/// Returns [`WyrdError::ConnectionNotTested`] with reason `jwks_unusable`
/// when the key set holds no usable key.
fn require_usable_jwks(provider: &ProviderMetadata) -> Result<(), WyrdError> {
    if provider.jwks().keys().is_empty() {
        tracing::warn!("candidate provider JWKS has no usable keys");
        return Err(not_tested_reason(
            "jwks_unusable",
            "the provider JWKS has no usable keys",
        ));
    }
    Ok(())
}

/// A failed connection-test check with a stable machine-readable `reason`.
///
/// Every candidate-test refusal and the activation guard build their
/// [`WyrdError::ConnectionNotTested`] here, so clients branch on the stable
/// `details.reason` code and `message` stays human-facing text.
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
