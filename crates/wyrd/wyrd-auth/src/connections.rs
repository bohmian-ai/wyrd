//! Tenant human OIDC connection authority.
//!
//! [`HumanConnections`] is the one owner of a tenant's human login trust. The
//! admin lifecycle (stage, test, activate, deactivate, remove) and the login
//! and callback read ([`HumanConnections::active_trusted_issuer`]) all go
//! through it, and it reads only `wyrd.auth_human_connections`: workload trust
//! in `wyrd.auth_trusted_issuers` never authorizes a human login.
//!
//! Every mutation opens one tenant transaction, takes the tenant's connection
//! slot lock, appends the caller's already-evaluated canonical audit decision,
//! validates against the locked state, and writes. A refusal reached after the
//! decision commits the decision alone; a failed audit append aborts before any
//! write, so no change is ever durable without its audit row. Provider network
//! IO (the candidate test) runs before that transaction and outside any lock.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;
use wyrd_auth_oidc::{ClientAuth, OidcError, OidcProvider, ScreenError, ScreenedHttp, TrustedIssuer};
use wyrd_crypt::SealingKeyring;
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{
    ClaimMappingPayload, ConnectionActivate, ConnectionInput, HumanClientAuth,
    HumanConnectionState, HumanConnectionView, HumanConnectionsResponse, IssuerTokenPolicy,
    IssuerUrl,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::AuditEvent;
use wyrd_sql::queries::auth::{
    HumanConnectionWrite, api_key_by_prefix, deactivate_active_human_connection,
    human_candidate_test_is_current, human_connection_in_state, insert_human_candidate,
    list_service_account_roles, live_human_connections, lock_human_connection_slot,
    promote_tested_human_candidate, remove_human_connection, replace_human_candidate,
    stamp_human_candidate_tested,
};
use wyrd_sql::row_types::auth::HumanConnectionRow;
use wyrd_sql::{SqlError, TenantConn};

use crate::audit::append_auth_audit;
use crate::credential_verify::verify_presented;
use crate::exchange_api_key::role_refs;
use crate::issuance::resolve_permissions;
use crate::issue_api_key::WyrdApiKey;
use crate::login::{auth_nonce, auth_state_key, build_authorization_url, pkce_verifier};
use crate::pg_resolvers::{claim_mapping_from_value, client_auth_from_row, seal_secret};

/// How long a successful candidate test authorizes activation.
pub const CONNECTION_TEST_VALIDITY: Duration = Duration::from_mins(15);

/// JWKS key-cache lifetime for a connection that inherits none.
const DEFAULT_JWKS_TTL_SECS: i64 = 300;

/// Path of the deployment's common provider callback on the public origin.
const CALLBACK_PATH: &str = "/auth/callback";

/// Provider error codes that mean the token endpoint refused Wyrd's client
/// authentication rather than the probe's deliberately bogus code.
const CLIENT_AUTH_REFUSALS: [&str; 2] = ["invalid_client", "unauthorized_client"];

/// The tenant human-connection owner.
///
/// Holds the runtime app pool (every statement runs on an RLS [`TenantConn`]),
/// the deployment sealing keyring, the screened provider HTTP capability, and
/// the callback URL derived from the configured public origin. Cheap to clone.
#[derive(Clone)]
pub struct HumanConnections {
    app: PgPool,
    keyring: Option<Arc<SealingKeyring>>,
    http: ScreenedHttp,
    callback_url: Option<Url>,
}

impl std::fmt::Debug for HumanConnections {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HumanConnections")
            .field("callback_url", &self.callback_url)
            .finish_non_exhaustive()
    }
}

/// A validated, sealed candidate ready for [`HumanConnections::put_candidate`].
#[derive(Debug)]
pub struct StagedCandidate {
    expected_revision: Option<u64>,
    write: HumanConnectionWrite,
}

/// The locked candidate a network test runs against, with its opened secret.
struct TestTarget {
    connection_id: Uuid,
    revision: i64,
    issuer: IssuerUrl,
    client_id: String,
    client_auth: ClientAuth,
}

impl HumanConnections {
    /// Build the owner over the app pool, keyring, screened HTTP, and public
    /// origin.
    ///
    /// The callback URL is `{public_origin}/auth/callback`; `None` when the
    /// deployment configures no public origin, in which case staging and
    /// testing refuse.
    #[must_use]
    pub fn new(
        app: PgPool,
        keyring: Option<Arc<SealingKeyring>>,
        http: ScreenedHttp,
        public_origin: Option<&Url>,
    ) -> Self {
        Self {
            app,
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
        let rows = live_human_connections(&mut conn).await.map_err(store_error)?;
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

    /// Prepare a candidate write without IO: require a public origin, check
    /// the input, and seal the secret under the keyring's write key.
    ///
    /// Callers run this before authorizing, so every refusal it can produce is
    /// a deployment or input fact rather than an unaudited decision.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when no public origin is configured,
    /// the input breaks a cross-field rule, or a secret must be sealed without
    /// a keyring, and [`WyrdError::Internal`] when sealing fails.
    pub fn stage(&self, input: ConnectionInput) -> Result<StagedCandidate, WyrdError> {
        self.require_callback()?;
        input.validate()?;
        let client_secret_enc = match &input.client_secret {
            Some(secret) if input.client_auth.requires_secret() => {
                let keyring = self.keyring.as_deref().ok_or_else(|| WyrdError::Validation {
                    message: "a client secret was supplied but no sealing key is configured"
                        .to_owned(),
                    details: serde_json::json!({ "reason": "sealing_key_missing" }),
                })?;
                Some(seal_secret(keyring, secret.expose().as_bytes()).map_err(internal)?)
            }
            _ => None,
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
        let candidate = human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
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

    /// Prove the exact candidate revision against its provider and stamp it
    /// activatable for [`CONNECTION_TEST_VALIDITY`].
    ///
    /// Screened checks, in order: discovery with issuer pinning; the JWKS
    /// document; the authorization endpoint accepting this client and the
    /// deployment callback; and the token endpoint accepting the client's
    /// authentication (a deliberately invalid code must fail as a grant error,
    /// not a client error). No transaction is held while they run. The stamp
    /// then lands under the slot lock only if the candidate is still at the
    /// tested revision.
    ///
    /// `decision` is appended in the stamp transaction; the caller has already
    /// recorded the allow decision before this network IO began.
    ///
    /// # Errors
    /// Returns [`WyrdError::ConnectionConflict`] when no candidate exists at
    /// `expected_revision`, [`WyrdError::ConnectionNotTested`] naming the
    /// failed check, [`WyrdError::Validation`] when the provider resolves to a
    /// blocked address or no public origin is configured,
    /// [`WyrdError::DiscoveryUnavailable`] when the provider is unreachable or
    /// failing, and the audit/store errors of [`Self::list`].
    pub async fn test_candidate(
        &self,
        tenant: DataTenantId,
        expected_revision: u64,
        decision: &AuditEvent,
    ) -> Result<HumanConnectionView, WyrdError> {
        let callback = self.require_callback()?.clone();
        let target = self.test_target(tenant, expected_revision).await?;
        let jwks_uri = self.probe(&target, &callback).await?;

        let mut conn = self.begin_locked(tenant, decision).await?;
        let stamped = stamp_human_candidate_tested(
            &mut conn,
            target.connection_id,
            target.revision,
            jwks_uri.as_str(),
            CONNECTION_TEST_VALIDITY,
        )
        .await
        .map_err(store_error)?;
        if !stamped {
            return commit_refusal(
                conn,
                conflict("the candidate changed while it was being tested"),
            )
            .await;
        }
        let row = human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
            .await
            .map_err(store_error)?
            .ok_or_else(|| internal("stamped candidate vanished"))?;
        conn.commit().await.map_err(store_error)?;
        self.view(row)
    }

    /// Swap the freshly tested candidate in as the tenant's Active connection.
    ///
    /// Under the slot lock: the candidate must exist at `expected_revision`
    /// with an unexpired stamp for that revision, and `recovery_api_key` must
    /// be a live API key of an active headless principal of this tenant that
    /// holds `identity_connections:write`. Only then is the previous Active
    /// retired and the candidate promoted, in the same transaction.
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
        let candidate = human_connection_in_state(&mut conn, HumanConnectionState::Candidate.as_str())
            .await
            .map_err(store_error)?;
        let Some(candidate) = candidate.filter(|row| {
            u64::try_from(row.revision).ok() == Some(request.expected_revision)
        }) else {
            return commit_refusal(
                conn,
                conflict("no candidate exists at expected_revision"),
            )
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

    /// Retire the Active connection; human login stops immediately.
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

    /// Tombstone one connection: it stops trusting logins, its secret is wiped,
    /// and its id remains for history.
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
        if remove_human_connection(&mut conn, connection_id)
            .await
            .map_err(store_error)?
            .is_none()
        {
            return commit_refusal(conn, not_found("no live human connection has this id")).await;
        }
        conn.commit().await.map_err(store_error)
    }

    /// Resolve the tenant's Active connection as the trusted issuer human
    /// login and callback verify against.
    ///
    /// The audience is always the client id and no default roles exist: a
    /// human's roles come only from the connection's group map. Read durably on
    /// every call, so every replica sees activation, replacement, and
    /// deactivation at once.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuthVerifyUnavailable`] when the store fails or the
    /// active row cannot be opened (for example, its secret was sealed under a
    /// key this process does not hold).
    pub async fn active_trusted_issuer(
        &self,
        tenant: DataTenantId,
    ) -> Result<Option<TrustedIssuer>, WyrdError> {
        let mut conn = self.begin(tenant).await?;
        let row = human_connection_in_state(&mut conn, HumanConnectionState::Active.as_str())
            .await
            .map_err(store_error)?;
        conn.commit().await.map_err(store_error)?;
        row.map(|row| {
            self.trusted_issuer(tenant, row).map_err(|error| {
                tracing::error!(error = %error, tenant_id = %tenant, "active human connection is unusable");
                WyrdError::AuthVerifyUnavailable {
                    message: "tenant login connection is unavailable".to_owned(),
                    details: serde_json::json!({ "retry_after_seconds": 1 }),
                }
            })
        })
        .transpose()
    }

    /// Open one tenant transaction.
    async fn begin(&self, tenant: DataTenantId) -> Result<TenantConn<'_>, WyrdError> {
        TenantConn::acquire(&self.app, tenant)
            .await
            .map_err(store_error)
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

    /// Refuse when no public origin is configured.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] naming the missing public origin.
    pub fn require_callback(&self) -> Result<&Url, WyrdError> {
        self.callback_url.as_ref().ok_or_else(|| WyrdError::Validation {
            message: "the deployment has no public origin configured (WYRD_PUBLIC_ORIGIN), so no \
                      callback URL can be registered"
                .to_owned(),
            details: serde_json::json!({ "reason": "public_origin_missing" }),
        })
    }

    /// Read and open the candidate at `expected_revision` for a network test.
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

    /// Run the screened provider checks and return the discovered JWKS URI.
    async fn probe(&self, target: &TestTarget, callback: &Url) -> Result<Url, WyrdError> {
        let issuer_url = Url::parse(target.issuer.as_str()).map_err(internal)?;
        let client = self.client_for(&issuer_url).await?;
        let provider = OidcProvider::discover(issuer_url, client)
            .await
            .map_err(discovery_error)?;

        let jwks = self.client_for(&provider.metadata.jwks_uri).await?;
        let keys = jwks
            .get(provider.metadata.jwks_uri.clone())
            .send()
            .await
            .map_err(unreachable)?;
        let keys_ok = keys.status().is_success()
            && keys.json::<Value>().await.ok().is_some_and(|document| {
                document
                    .get("keys")
                    .and_then(Value::as_array)
                    .is_some_and(|keys| !keys.is_empty())
            });
        if !keys_ok {
            return Err(not_tested_reason("jwks_unusable", "the provider JWKS has no usable keys"));
        }

        self.probe_callback(&provider, target, callback).await?;
        self.probe_client_auth(&provider, target, callback).await?;
        Ok(provider.metadata.jwks_uri)
    }

    /// Ask the authorization endpoint to start a login for this client and
    /// callback; a 4xx means the provider rejects the client or redirect URI.
    async fn probe_callback(
        &self,
        provider: &OidcProvider,
        target: &TestTarget,
        callback: &Url,
    ) -> Result<(), WyrdError> {
        let verifier = pkce_verifier();
        let url = build_authorization_url(
            &provider.metadata.authorization_endpoint,
            &target.client_id,
            callback.as_str(),
            &auth_state_key(),
            verifier.expose_secret(),
            &auth_nonce(),
        );
        let client = self.client_for(&url).await?;
        let status = client.get(url).send().await.map_err(unreachable)?.status();
        if status.is_server_error() {
            return Err(unreachable("authorization endpoint failed"));
        }
        if status.is_client_error() {
            return Err(not_tested_reason(
                "callback_rejected",
                "the provider rejected this client id or the callback URL; register the \
                 callback URL exactly as shown",
            ));
        }
        Ok(())
    }

    /// Present the client's authentication with a deliberately invalid code:
    /// a grant error proves the client authenticated, a client error proves it
    /// did not.
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
        let client = self.client_for(&token_endpoint).await?;
        let mut form = vec![
            ("grant_type", "authorization_code".to_owned()),
            ("code", format!("wyrd-connection-test-{}", auth_state_key())),
            ("client_id", target.client_id.clone()),
            ("redirect_uri", callback.to_string()),
            ("code_verifier", pkce_verifier().expose_secret().to_owned()),
        ];
        let mut request = client.post(token_endpoint);
        match &target.client_auth {
            ClientAuth::SecretBasic(secret) => {
                request = request.basic_auth(&target.client_id, Some(secret.expose_secret()));
            }
            ClientAuth::SecretPost(secret) => {
                form.push(("client_secret", secret.expose_secret().to_owned()));
            }
            ClientAuth::Public | ClientAuth::PrivateKeyJwt => {}
        }
        let response = request.form(&form).send().await.map_err(unreachable)?;
        let status = response.status();
        if status.is_server_error() {
            return Err(unreachable("token endpoint failed"));
        }
        let error_code = response
            .json::<Value>()
            .await
            .ok()
            .and_then(|body| body.get("error").and_then(Value::as_str).map(str::to_owned));
        let refused = status == reqwest::StatusCode::UNAUTHORIZED
            || error_code
                .as_deref()
                .is_some_and(|code| CLIENT_AUTH_REFUSALS.contains(&code));
        if refused {
            return Err(not_tested_reason(
                "client_auth_rejected",
                "the provider token endpoint rejected this client's authentication",
            ));
        }
        Ok(())
    }

    /// Build a screened client for one provider URL.
    async fn client_for(&self, url: &Url) -> Result<reqwest::Client, WyrdError> {
        self.http.client_for(url).await.map_err(|error| match error {
            ScreenError::Blocked => WyrdError::Validation {
                message: "the provider resolves to a blocked address range".to_owned(),
                details: serde_json::json!({ "field": "issuer" }),
            },
            ScreenError::Unresolved | ScreenError::Client => unreachable(error),
        })
    }

    /// Project a stored row to its redacted view.
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

    /// Build the verification-ready issuer for an Active row.
    fn trusted_issuer(
        &self,
        tenant: DataTenantId,
        row: HumanConnectionRow,
    ) -> Result<TrustedIssuer, String> {
        let issuer = IssuerUrl::new(row.issuer_url).map_err(|error| error.to_string())?;
        let jwks_uri = row
            .jwks_uri
            .as_deref()
            .ok_or("active connection has no jwks uri")?;
        let jwks_uri = Url::parse(jwks_uri).map_err(|error| error.to_string())?;
        let client_auth = client_auth_from_row(
            &row.client_auth,
            row.client_secret_enc.as_deref(),
            self.keyring.as_deref(),
        )
        .map_err(|error| error.to_string())?;
        Ok(TrustedIssuer {
            tenant_id: tenant,
            issuer,
            jwks_uri,
            expected_audience: row.client_id.clone(),
            client_id: row.client_id,
            client_auth,
            claim_mapping: claim_mapping_from_value(row.claim_mapping)
                .map_err(|error| error.to_string())?,
            group_role_map: serde_json::from_value(row.group_role_map)
                .map_err(|error| error.to_string())?,
            default_roles: Vec::new(),
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: Duration::from_secs(row.jwks_ttl_secs.max(1).unsigned_abs()),
        })
    }
}

/// Verify a recovery API key against this tenant, constant-cost on refusal.
///
/// The key must parse, name this tenant, match a live key row by Argon2, belong
/// to an active headless principal, and resolve to permissions covering
/// `identity_connections:write`. Every refusal still runs one verification.
///
/// # Errors
/// Returns [`WyrdError::AuthVerifyUnavailable`] when a store read fails, and
/// [`WyrdError::RoleCorrupt`] when a stored role document does not decode.
async fn recovery_key_authorizes(
    conn: &mut TenantConn<'_>,
    presented: &SecretString,
) -> Result<bool, WyrdError> {
    let row = match WyrdApiKey::parse(presented.expose_secret()) {
        Ok(parsed) if parsed.tenant_id == conn.data_tenant_id() => {
            api_key_by_prefix(conn, &parsed.prefix)
                .await
                .map_err(store_error)?
        }
        _ => None,
    };
    let matched = verify_presented(presented, row.as_ref().map(|row| row.key_hash.as_str()))
        .await
        .map_err(internal)?;
    let Some(row) = row.filter(|row| matched && row.status == "active") else {
        return Ok(false);
    };
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

/// Map discovery failure: an issuer mismatch is a configuration error, every
/// other failure is the provider being unavailable.
fn discovery_error(error: OidcError) -> WyrdError {
    match error {
        OidcError::IssuerMismatch { .. } => not_tested_reason(
            "issuer_mismatch",
            "the provider discovery document names a different issuer",
        ),
        other => unreachable(other),
    }
}

/// The provider could not be reached or failed; activation fails closed.
fn unreachable(error: impl std::fmt::Display) -> WyrdError {
    tracing::warn!(error = %error, "tenant connection provider unavailable");
    WyrdError::DiscoveryUnavailable {
        message: "the identity provider could not be reached".to_owned(),
        details: serde_json::json!({}),
    }
}

/// A failed test check with a stable machine-readable reason.
fn not_tested_reason(reason: &str, message: &str) -> WyrdError {
    WyrdError::ConnectionNotTested {
        message: message.to_owned(),
        details: serde_json::json!({ "reason": reason }),
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
        details: serde_json::json!({}),
    }
}

/// No such connection in this tenant.
fn not_found(message: &str) -> WyrdError {
    WyrdError::NotFound {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}

/// Map a store failure to the fail-closed backend error, logging the cause.
fn store_error(error: impl Into<SqlError>) -> WyrdError {
    let error = error.into();
    tracing::warn!(error = %error, "tenant connection store unavailable");
    WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: serde_json::json!({ "retry_after_seconds": 1 }),
    }
}

/// Map an unexpected server-side failure, logging the cause server-side only.
fn internal(cause: impl std::fmt::Display) -> WyrdError {
    tracing::error!(error = %cause, "tenant connection operation failed");
    WyrdError::Internal {
        message: "tenant connection operation failed".to_owned(),
        details: serde_json::json!({}),
    }
}
