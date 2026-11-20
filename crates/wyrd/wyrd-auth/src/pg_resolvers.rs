//! Postgres-backed trusted-issuer and workload-binding resolvers.
//!
//! These are the production implementations of [`IssuerConfigResolver`] and
//! [`WorkloadBindingResolver`]. They live in `wyrd-auth` (not the SQL-free
//! `wyrd-auth-oidc` crate) because they hold the [`WyrdPostgres`] handle and,
//! for issuers, the optional process-wide stored-secret facade.
//!
//! Issuer trust and workload bindings are tenant-scoped: every read acquires a
//! [`TenantConn`](wyrd_sql::TenantConn) through [`WyrdPostgres`], so Postgres
//! RLS is the load-bearing isolation boundary. A resolve for tenant A can
//! never surface tenant B's issuers or bindings.
//!
//! The `client_secret_enc` BYTEA column stores the common stored-secret
//! envelope, bound to the tenant (or platform), purpose, issuer URL, client
//! id, and client-authentication method. It is opened on read through
//! [`WyrdKms`] after the row's transaction ends. A row that carries a secret
//! but no `[kms]` is configured fails closed (a 503-class `JwksUnavailable`),
//! never a silent drop that would masquerade as an untrusted issuer (401).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::platform_login::PlatformConnection;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, IssuerVerification, OidcError,
    TrustedIssuer, WorkloadBinding, WorkloadBindingResolver,
};
use wyrd_kms::{SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::HUMAN_SUBJECT_CLAIM;
use wyrd_spec::auth::IssuerTokenPolicy;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::ids::CardUid;
use wyrd_sql::WyrdPostgres;
use wyrd_sql::queries::auth::{
    TrustedIssuerWrite, WorkloadBindingWrite, trusted_issuer_by_url, trusted_issuers_for_tenant,
    workload_binding_by_subject,
};
use wyrd_sql::queries::platform::identity::PlatformOidcConnectionRow;
use wyrd_sql::row_types::auth::{HumanConnectionRow, TrustedIssuerRow};

const CLIENT_AUTH_SECRET_BASIC: &str = "SecretBasic";
const CLIENT_AUTH_SECRET_POST: &str = "SecretPost";
const CLIENT_AUTH_PRIVATE_KEY_JWT: &str = "PrivateKeyJwt";
const CLIENT_AUTH_PUBLIC: &str = "Public";

const PRINCIPAL_KIND_HUMAN: &str = "Human";
const PRINCIPAL_KIND_WORKLOAD: &str = "Workload";

// --------------------------------------------------------------------------
// PgIssuerResolver
// --------------------------------------------------------------------------

/// Production [`IssuerConfigResolver`] backed by `wyrd.auth_trusted_issuers`.
///
/// Holds the role-separated runtime [`WyrdPostgres`] handle and an optional
/// process-wide stored-secret facade. The facade is `Option` because a
/// deployment with only secret-free issuers (`PrivateKeyJwt`/`Public`) needs
/// none; opening is only attempted for rows that actually carry a secret.
#[derive(Clone)]
pub struct PgIssuerResolver {
    /// Runtime Postgres handle; every read runs on an RLS [`TenantConn`](wyrd_sql::TenantConn)
    /// acquired through [`WyrdPostgres::tenant_conn`], so a resolve for one
    /// tenant can never observe another tenant's issuers.
    postgres: WyrdPostgres,
    /// Facade that opens stored client secrets, `None` without `[kms]`. A
    /// row carrying a secret with no facade fails closed with
    /// [`IssuerDecodeError::KmsMissing`] instead of being dropped.
    kms: Option<Arc<WyrdKms>>,
}

/// Redacted debug view: names the facade and hides the store handle.
impl std::fmt::Debug for PgIssuerResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgIssuerResolver")
            .field("kms", &self.kms)
            .finish_non_exhaustive()
    }
}

impl PgIssuerResolver {
    /// Construct a resolver over the runtime Postgres handle with an optional
    /// stored-secret facade.
    #[must_use]
    pub fn new(postgres: WyrdPostgres, kms: Option<Arc<WyrdKms>>) -> Self {
        Self { postgres, kms }
    }
}

impl IssuerConfigResolver for PgIssuerResolver {
    /// List every trusted issuer configured for `tenant`, opening stored
    /// client secrets after the read's transaction ends.
    ///
    /// # Errors
    ///
    /// Returns [`OidcError::JwksUnavailable`] when the tenant connection
    /// cannot be acquired, the issuer query fails, or any row cannot be
    /// decoded or decrypted.
    ///
    /// # Cancellation
    ///
    /// The lookup is read-only; cancelling releases the tenant connection.
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant))]
    async fn trusted_issuers(
        &self,
        tenant: &DataTenantId,
    ) -> Result<Vec<TrustedIssuer>, OidcError> {
        let mut conn = self.postgres.tenant_conn(*tenant).await.map_err(|error| {
            tracing::warn!(error = %error, "issuer resolver failed to acquire tenant connection");
            OidcError::JwksUnavailable {
                issuer: tenant.as_uuid().to_string(),
                message: error.to_string(),
            }
        })?;
        let rows = trusted_issuers_for_tenant(&mut conn)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "issuer resolver query failed");
                OidcError::JwksUnavailable {
                    issuer: tenant.as_uuid().to_string(),
                    message: error.to_string(),
                }
            })?;

        drop(conn);
        let mut issuers = Vec::with_capacity(rows.len());
        for row in rows {
            let issuer_url = row.issuer_url.clone();
            issuers.push(
                trusted_issuer_from_row(*tenant, row, self.kms.as_deref())
                    .await
                    .map_err(|error| OidcError::JwksUnavailable {
                        issuer: issuer_url,
                        message: error.to_string(),
                    })?,
            );
        }
        Ok(issuers)
    }

    /// Fetch only the tenant's issuer row keyed by `issuer` instead of listing
    /// and filtering every configured issuer.
    ///
    /// # Errors
    ///
    /// Returns [`OidcError::JwksUnavailable`] when the tenant connection
    /// cannot be acquired, the issuer query fails, or the matching row cannot
    /// be decoded or decrypted. A missing issuer is `Ok(None)`.
    ///
    /// # Cancellation
    ///
    /// The lookup is read-only; cancelling releases the tenant connection.
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant, issuer = %issuer))]
    async fn trusted_issuer(
        &self,
        tenant: &DataTenantId,
        issuer: &IssuerUrl,
    ) -> Result<Option<TrustedIssuer>, OidcError> {
        let mut conn = self.postgres.tenant_conn(*tenant).await.map_err(|error| {
            tracing::warn!(error = %error, "issuer resolver failed to acquire tenant connection");
            OidcError::JwksUnavailable {
                issuer: tenant.as_uuid().to_string(),
                message: error.to_string(),
            }
        })?;
        let row = trusted_issuer_by_url(&mut conn, issuer.as_str())
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "issuer resolver query failed");
                OidcError::JwksUnavailable {
                    issuer: tenant.as_uuid().to_string(),
                    message: error.to_string(),
                }
            })?;
        drop(conn);
        let Some(row) = row else {
            return Ok(None);
        };
        trusted_issuer_from_row(*tenant, row, self.kms.as_deref())
            .await
            .map(Some)
            .map_err(|error| OidcError::JwksUnavailable {
                issuer: issuer.as_str().to_owned(),
                message: error.to_string(),
            })
    }
}

// --------------------------------------------------------------------------
// PgWorkloadBindingResolver
// --------------------------------------------------------------------------

/// Production [`WorkloadBindingResolver`] backed by `wyrd.auth_workload_bindings`.
///
/// Holds only the runtime [`WyrdPostgres`] handle — bindings carry no
/// secrets, so no `[kms]` is needed. Each lookup opens its tenant
/// transaction through [`WyrdPostgres::tenant_conn`], so RLS binds the read to
/// the requested tenant. Audience precedence (exact match preferred,
/// `NULL`-audience fallback) is enforced in the SQL query
/// (`workload_binding_by_subject`).
#[derive(Clone)]
pub struct PgWorkloadBindingResolver {
    /// Runtime Postgres owner; tenant transactions come only from
    /// [`WyrdPostgres::tenant_conn`].
    postgres: WyrdPostgres,
}

/// Redacted debug view: hides the store handle.
impl std::fmt::Debug for PgWorkloadBindingResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgWorkloadBindingResolver")
            .finish_non_exhaustive()
    }
}

impl PgWorkloadBindingResolver {
    /// Construct a resolver over the runtime Postgres handle.
    #[must_use]
    pub fn new(postgres: WyrdPostgres) -> Self {
        Self { postgres }
    }
}

impl WorkloadBindingResolver for PgWorkloadBindingResolver {
    /// Read the tenant's workload binding for `issuer` and `subject`.
    ///
    /// Prefers a row whose audience equals `audience` and falls back to a
    /// `NULL`-audience row; returns `Ok(None)` when neither exists so the
    /// caller refuses the exchange.
    ///
    /// # Errors
    ///
    /// Returns [`OidcError::JwksUnavailable`] when the tenant connection
    /// cannot be acquired, the binding query fails, or the stored root
    /// `workload_uid` is not a valid Card UID.
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant, issuer = %issuer))]
    async fn binding(
        &self,
        tenant: &DataTenantId,
        issuer: &IssuerUrl,
        subject: &str,
        audience: Option<&str>,
    ) -> Result<Option<WorkloadBinding>, OidcError> {
        let mut conn = self.postgres.tenant_conn(*tenant).await.map_err(|error| {
            tracing::warn!(error = %error, "binding resolver failed to acquire tenant connection");
            OidcError::JwksUnavailable {
                issuer: issuer.as_str().to_owned(),
                message: error.to_string(),
            }
        })?;
        let row = workload_binding_by_subject(&mut conn, issuer.as_str(), subject, audience)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, "binding resolver query failed");
                OidcError::JwksUnavailable {
                    issuer: issuer.as_str().to_owned(),
                    message: error.to_string(),
                }
            })?;
        row.map(|row| {
            let workload_uid = row
                .workload_uid
                .map(CardUid::from_uuid)
                .transpose()
                .map_err(|error| {
                    tracing::warn!(error = %error, "stored binding root is not a Card UID");
                    OidcError::JwksUnavailable {
                        issuer: issuer.as_str().to_owned(),
                        message: error.to_string(),
                    }
                })?;
            Ok(WorkloadBinding {
                tenant_id: *tenant,
                issuer: issuer.clone(),
                subject: row.subject,
                audience: row.audience,
                principal_id: PrincipalId::new(row.principal_id),
                workload_uid,
            })
        })
        .transpose()
    }
}

// --------------------------------------------------------------------------
// Seal errors (write/encode path)
// --------------------------------------------------------------------------

/// Failure encoding a [`TrustedIssuer`] into a [`TrustedIssuerWrite`] for seeding.
#[derive(Debug, thiserror::Error)]
pub enum IssuerSealError {
    /// The issuer authenticates with a client secret but no `[kms]` is
    /// configured. Writes fail closed rather than persist a plaintext or
    /// empty secret column.
    #[error("the client secret cannot be stored: no [kms] is configured")]
    KmsMissing,
    /// Sealing the client secret failed, including a wrapping-backend failure.
    #[error("failed to encrypt the client secret")]
    Encrypt,
    /// A JSONB column could not be serialized.
    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
}

/// Failure decoding a [`TrustedIssuerRow`], [`HumanConnectionRow`], or
/// platform connection row back into a verification-ready issuer.
///
/// Every variant is fail-closed: the issuer resolver maps it to
/// [`OidcError::JwksUnavailable`] and the human and platform connection
/// owners to a server-side (5xx) error, so an undecodable row never reads as
/// an untrusted (401) issuer. No variant carries secret material.
#[derive(Debug, thiserror::Error)]
pub(crate) enum IssuerDecodeError {
    /// The stored `issuer_url` is not a valid issuer URL; carries the parse
    /// error text.
    #[error("stored issuer url is invalid: {0}")]
    IssuerUrl(String),
    /// The stored `jwks_uri` is not an absolute URL; carries the parse error
    /// text.
    #[error("stored jwks uri is invalid: {0}")]
    JwksUri(String),
    /// The stored `client_auth` names no known client-authentication method.
    #[error("unknown client_auth discriminant {0:?}")]
    ClientAuth(String),
    /// The stored `principal_kind` names no known token policy.
    #[error("unknown principal_kind discriminant {0:?}")]
    PrincipalKind(String),
    /// A secret-bearing method (`SecretBasic`/`SecretPost`) has no stored
    /// ciphertext.
    #[error("client_auth requires a secret but client_secret_enc is null")]
    MissingSecret,
    /// The row carries a stored secret but no `[kms]` is configured.
    #[error("client secret present but no [kms] is configured")]
    KmsMissing,
    /// The stored envelope does not open under its expected context: a
    /// disallowed key, a tampered or substituted value, or a backend failure.
    #[error("client secret could not be decrypted")]
    Decrypt,
    /// The opened secret is not UTF-8; its bytes are discarded, not echoed.
    #[error("decrypted client secret is not valid utf-8")]
    SecretEncoding,
    /// A stored human connection maps a subject claim other than `sub`;
    /// carries the stored path. Human identity is `(issuer, sub)` only.
    #[error("human connection subject claim {0:?} is not \"sub\"")]
    HumanSubject(String),
    /// A JSONB column does not match its expected shape.
    #[error("json column decode failed: {0}")]
    Json(#[from] serde_json::Error),
}

// --------------------------------------------------------------------------
// ClaimMapping JSONB bridge
// --------------------------------------------------------------------------

/// Serde mirror of [`ClaimMapping`], whose [`ClaimPath`] is not (de)serializable.
#[derive(Debug, Serialize, Deserialize)]
struct ClaimMappingDto {
    subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    groups: Option<String>,
}

impl ClaimMappingDto {
    fn from_domain(mapping: &ClaimMapping) -> Self {
        Self {
            subject: mapping.subject.as_str().to_owned(),
            email: mapping.email.as_ref().map(|p| p.as_str().to_owned()),
            groups: mapping.groups.as_ref().map(|p| p.as_str().to_owned()),
        }
    }

    fn into_domain(self) -> ClaimMapping {
        ClaimMapping {
            subject: ClaimPath::new(self.subject),
            email: self.email.map(ClaimPath::new),
            groups: self.groups.map(ClaimPath::new),
        }
    }
}

fn claim_mapping_to_value(mapping: &ClaimMapping) -> Result<Value, serde_json::Error> {
    serde_json::to_value(ClaimMappingDto::from_domain(mapping))
}

/// Decode a stored claim-mapping JSONB column into the domain mapping.
///
/// # Errors
/// Returns the serde error when the column does not match the mapping shape.
pub(crate) fn claim_mapping_from_value(value: Value) -> Result<ClaimMapping, serde_json::Error> {
    let dto: ClaimMappingDto = serde_json::from_value(value)?;
    Ok(dto.into_domain())
}

// --------------------------------------------------------------------------
// Discriminant <-> domain mapping
// --------------------------------------------------------------------------

fn client_auth_discriminant(auth: &ClientAuth) -> &'static str {
    match auth {
        ClientAuth::SecretBasic(_) => CLIENT_AUTH_SECRET_BASIC,
        ClientAuth::SecretPost(_) => CLIENT_AUTH_SECRET_POST,
        ClientAuth::PrivateKeyJwt => CLIENT_AUTH_PRIVATE_KEY_JWT,
        ClientAuth::Public => CLIENT_AUTH_PUBLIC,
    }
}

fn principal_kind_discriminant(kind: IssuerTokenPolicy) -> &'static str {
    match kind {
        IssuerTokenPolicy::Human => PRINCIPAL_KIND_HUMAN,
        IssuerTokenPolicy::Workload => PRINCIPAL_KIND_WORKLOAD,
    }
}

fn principal_kind_from_str(value: &str) -> Result<IssuerTokenPolicy, IssuerDecodeError> {
    match value {
        PRINCIPAL_KIND_HUMAN => Ok(IssuerTokenPolicy::Human),
        PRINCIPAL_KIND_WORKLOAD => Ok(IssuerTokenPolicy::Workload),
        other => Err(IssuerDecodeError::PrincipalKind(other.to_owned())),
    }
}

// --------------------------------------------------------------------------
// Secret sealing (common stored-secret envelope)
// --------------------------------------------------------------------------

/// The trusted owner metadata a stored OIDC client secret is bound to.
///
/// Built from verified authority and the row's trusted columns, never from
/// the envelope: the tenant (or the platform), the owning table's purpose,
/// the issuer URL, the client id, the client-authentication method, and, for
/// a tenant human connection, the connection id. A tenant's Active and
/// Candidate connections can share issuer, client, and method, so only the
/// connection id keeps one's secret from opening as the other's.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClientSecretOwner<'a> {
    /// Owning tenant, or the platform.
    pub(crate) scope: SecretScope,
    /// Which table owns the secret.
    pub(crate) purpose: SecretPurpose,
    /// Issuer URL of the connection or issuer.
    pub(crate) issuer_url: &'a str,
    /// OAuth client id.
    pub(crate) client_id: &'a str,
    /// The owning human connection's id; `None` for an issuer or platform
    /// connection, which its issuer URL already identifies.
    pub(crate) connection_id: Option<&'a str>,
}

impl ClientSecretOwner<'_> {
    /// Seal `client_auth`'s secret, or return `None` for a method that holds
    /// none.
    ///
    /// # Errors
    /// Returns the errors of [`Self::seal_secret`].
    pub(crate) async fn seal(
        &self,
        client_auth: &ClientAuth,
        kms: Option<&WyrdKms>,
    ) -> Result<Option<Vec<u8>>, IssuerSealError> {
        let (ClientAuth::SecretBasic(secret) | ClientAuth::SecretPost(secret)) = client_auth else {
            return Ok(None);
        };
        self.seal_secret(
            client_auth_discriminant(client_auth),
            secret.expose_secret().as_bytes(),
            kms,
        )
        .await
        .map(Some)
    }

    /// Seal `secret` for the stored client-authentication `method`.
    ///
    /// # Errors
    /// Returns [`IssuerSealError::KmsMissing`] when `kms` is `None`, and
    /// [`IssuerSealError::Encrypt`] when sealing fails.
    pub(crate) async fn seal_secret(
        &self,
        method: &str,
        secret: &[u8],
        kms: Option<&WyrdKms>,
    ) -> Result<Vec<u8>, IssuerSealError> {
        let kms = kms.ok_or(IssuerSealError::KmsMissing)?;
        let authority = self.authority(method);
        kms.seal(&self.context(&authority), secret)
            .await
            .map_err(|error| {
                tracing::warn!(%error, purpose = self.purpose.as_str(), "client secret could not be sealed");
                IssuerSealError::Encrypt
            })
    }

    /// Renew the encryption of `envelope`, sealed for the stored
    /// client-authentication `method`: the same secret resealed under a fresh
    /// data key and nonce wrapped by the active key.
    ///
    /// # Errors
    /// Returns [`IssuerSealError::KmsMissing`] when `kms` is `None`, and
    /// [`IssuerSealError::Encrypt`] when the envelope does not open under
    /// this owner or the reseal fails.
    pub(crate) async fn renew_secret(
        &self,
        method: &str,
        envelope: &[u8],
        kms: Option<&WyrdKms>,
    ) -> Result<Vec<u8>, IssuerSealError> {
        let kms = kms.ok_or(IssuerSealError::KmsMissing)?;
        let authority = self.authority(method);
        kms.renew(&self.context(&authority), envelope)
            .await
            .map_err(|error| {
                tracing::warn!(%error, purpose = self.purpose.as_str(), "client secret could not be renewed");
                IssuerSealError::Encrypt
            })
    }

    /// Rebuild a stored client-authentication method, opening its secret.
    ///
    /// Shared by the workload issuer, platform connection, and tenant human
    /// connection decoders so every store opens secrets through one path.
    ///
    /// # Errors
    /// Returns [`IssuerDecodeError`] for an unknown discriminant, a secret
    /// method with no stored secret, a missing facade, an envelope that does
    /// not open under this owner, or a non-UTF-8 secret.
    pub(crate) async fn client_auth(
        &self,
        discriminant: &str,
        secret_enc: Option<&[u8]>,
        kms: Option<&WyrdKms>,
    ) -> Result<ClientAuth, IssuerDecodeError> {
        let secret = || async {
            let bytes = secret_enc.ok_or(IssuerDecodeError::MissingSecret)?;
            let kms = kms.ok_or(IssuerDecodeError::KmsMissing)?;
            let authority = self.authority(discriminant);
            let plaintext = kms
                .open(&self.context(&authority), bytes)
                .await
                .map_err(|error| {
                    tracing::warn!(%error, purpose = self.purpose.as_str(), "client secret could not be opened");
                    IssuerDecodeError::Decrypt
                })?;
            let text =
                std::str::from_utf8(&plaintext).map_err(|_| IssuerDecodeError::SecretEncoding)?;
            Ok::<_, IssuerDecodeError>(SecretString::from(text.to_owned()))
        };
        match discriminant {
            CLIENT_AUTH_SECRET_BASIC => Ok(ClientAuth::SecretBasic(secret().await?)),
            CLIENT_AUTH_SECRET_POST => Ok(ClientAuth::SecretPost(secret().await?)),
            CLIENT_AUTH_PRIVATE_KEY_JWT => Ok(ClientAuth::PrivateKeyJwt),
            CLIENT_AUTH_PUBLIC => Ok(ClientAuth::Public),
            other => Err(IssuerDecodeError::ClientAuth(other.to_owned())),
        }
    }

    /// The envelope authority for `method`: the client id, the method, and
    /// the connection id when there is one.
    fn authority<'b>(&'b self, method: &'b str) -> Vec<&'b str> {
        [self.client_id, method]
            .into_iter()
            .chain(self.connection_id)
            .collect()
    }

    /// The envelope context over `authority`, from [`Self::authority`].
    fn context<'a>(&'a self, authority: &'a [&'a str]) -> SecretContext<'a> {
        SecretContext {
            scope: self.scope,
            purpose: self.purpose,
            owner: self.issuer_url,
            authority,
            version: None,
        }
    }
}

// --------------------------------------------------------------------------
// Row -> TrustedIssuer
// --------------------------------------------------------------------------

/// Reconstruct a full [`TrustedIssuer`] from a stored row.
///
/// `tenant` is the bound tenant the row was read under: RLS guarantees
/// `row.data_tenant_id == tenant.as_uuid()`, so we carry the already-validated
/// [`DataTenantId`] through rather than re-parsing the column (which would
/// re-impose `UUIDv7` validation the boundary already passed).
async fn trusted_issuer_from_row(
    tenant: DataTenantId,
    row: TrustedIssuerRow,
    kms: Option<&WyrdKms>,
) -> Result<TrustedIssuer, IssuerDecodeError> {
    let issuer = IssuerUrl::new(row.issuer_url.clone())
        .map_err(|error| IssuerDecodeError::IssuerUrl(error.to_string()))?;
    let jwks_uri =
        Url::parse(&row.jwks_uri).map_err(|error| IssuerDecodeError::JwksUri(error.to_string()))?;
    let client_auth = ClientSecretOwner {
        scope: SecretScope::Tenant(tenant),
        purpose: SecretPurpose::AuthTrustedIssuer,
        issuer_url: &row.issuer_url,
        client_id: &row.client_id,
        connection_id: None,
    }
    .client_auth(&row.client_auth, row.client_secret_enc.as_deref(), kms)
    .await?;
    let claim_mapping = claim_mapping_from_value(row.claim_mapping)?;
    let group_role_map: HashMap<String, Vec<String>> = serde_json::from_value(row.group_role_map)?;
    let default_roles: Vec<String> = serde_json::from_value(row.default_roles)?;
    let principal_kind = principal_kind_from_str(&row.principal_kind)?;

    Ok(TrustedIssuer {
        tenant_id: tenant,
        issuer,
        jwks_uri,
        expected_audience: row.expected_audience,
        client_id: row.client_id,
        client_auth,
        claim_mapping,
        group_role_map,
        default_roles,
        principal_kind,
        jwks_ttl: Duration::from_secs(row.jwks_ttl_secs.max(0).unsigned_abs()),
    })
}

/// Rebuild the trusted issuer human login verifies against from a tenant's
/// Active human-connection row.
///
/// The sibling of [`trusted_issuer_from_row`] for the human connection table:
/// the same secret, claim-mapping, and URL decode steps, except that the ID
/// token audience is always the client id and no default roles exist, since a
/// human's roles come only from the connection's group map.
///
/// A stored connection whose subject claim is not exactly `sub` — written
/// before authoring required it — fails closed here, so it never reaches
/// login or the callback's identity lookup.
///
/// # Errors
/// Returns [`IssuerDecodeError`] when the issuer or JWKS URI is malformed or
/// absent, a JSON column does not decode, the subject claim is not `sub`
/// ([`IssuerDecodeError::HumanSubject`]), or the client authentication cannot
/// be reconstructed — including a stored secret that does not open.
pub(crate) async fn human_connection_trusted_issuer(
    tenant: DataTenantId,
    row: HumanConnectionRow,
    kms: Option<&WyrdKms>,
) -> Result<TrustedIssuer, IssuerDecodeError> {
    let claim_mapping = claim_mapping_from_value(row.claim_mapping)?;
    if claim_mapping.subject.as_str() != HUMAN_SUBJECT_CLAIM {
        return Err(IssuerDecodeError::HumanSubject(
            claim_mapping.subject.as_str().to_owned(),
        ));
    }
    let issuer = IssuerUrl::new(row.issuer_url)
        .map_err(|error| IssuerDecodeError::IssuerUrl(error.to_string()))?;
    let jwks_uri = row
        .jwks_uri
        .as_deref()
        .ok_or_else(|| IssuerDecodeError::JwksUri("the connection has no jwks uri".to_owned()))?;
    let jwks_uri =
        Url::parse(jwks_uri).map_err(|error| IssuerDecodeError::JwksUri(error.to_string()))?;
    let connection_id = row.connection_id.to_string();
    let client_auth = ClientSecretOwner {
        scope: SecretScope::Tenant(tenant),
        purpose: SecretPurpose::AuthHumanConnection,
        issuer_url: issuer.as_str(),
        client_id: &row.client_id,
        connection_id: Some(&connection_id),
    }
    .client_auth(&row.client_auth, row.client_secret_enc.as_deref(), kms)
    .await?;
    Ok(TrustedIssuer {
        tenant_id: tenant,
        issuer,
        jwks_uri,
        expected_audience: row.client_id.clone(),
        client_id: row.client_id,
        client_auth,
        claim_mapping,
        group_role_map: serde_json::from_value(row.group_role_map)?,
        default_roles: Vec::new(),
        principal_kind: IssuerTokenPolicy::Human,
        jwks_ttl: Duration::from_secs(row.jwks_ttl_secs.max(1).unsigned_abs()),
    })
}

// --------------------------------------------------------------------------
// Row -> platform connection
// --------------------------------------------------------------------------

/// Reconstruct the platform-scope OIDC connection from its stored row.
///
/// Shares every decode step with the tenant issuer path — sealed client secret,
/// claim mapping, URL validation — because the platform connection is the same
/// kind of thing minus the tenancy. It produces [`IssuerVerification`] directly
/// rather than a [`TrustedIssuer`], since a platform connection has no tenant
/// to put in one and no groups or default roles to carry: a platform
/// principal's authority comes from its grant, never from a provider claim.
///
/// # Errors
/// Returns an error when the issuer or JWKS URL is malformed, the claim mapping
/// cannot be decoded, or the client authentication cannot be reconstructed —
/// including when the row carries a secret and no `[kms]` is configured,
/// which fails closed rather than degrading to an unauthenticated client.
pub async fn platform_connection_from_row(
    row: PlatformOidcConnectionRow,
    kms: Option<&WyrdKms>,
) -> Result<PlatformConnection, PlatformConnectionError> {
    let issuer = IssuerUrl::new(row.issuer_url.clone())
        .map_err(|error| PlatformConnectionError::IssuerUrl(error.to_string()))?;
    let jwks_uri = Url::parse(&row.jwks_uri)
        .map_err(|error| PlatformConnectionError::JwksUri(error.to_string()))?;
    let client_auth = ClientSecretOwner {
        scope: SecretScope::Platform,
        purpose: SecretPurpose::PlatformOidc,
        issuer_url: &row.issuer_url,
        client_id: &row.client_id,
        connection_id: None,
    }
    .client_auth(&row.client_auth, row.client_secret_enc.as_deref(), kms)
    .await
    .map_err(|error| PlatformConnectionError::ClientAuth(error.to_string()))?;
    let claim_mapping = claim_mapping_from_value(row.claim_mapping)
        .map_err(|error| PlatformConnectionError::ClaimMapping(error.to_string()))?;

    Ok(crate::platform_login::PlatformConnection {
        verification: IssuerVerification {
            issuer,
            jwks_uri,
            // A human ID token is addressed to the relying party's client
            // (OpenID Connect Core 1.0 §3.1.3.7 step 3).
            expected_audience: row.client_id.clone(),
            claim_mapping,
            // A platform connection exists to let people sign in. A workload
            // reaches the platform plane with a credential, never a federated
            // token, so this is not configurable.
            principal_kind: IssuerTokenPolicy::Human,
        },
        client_id: row.client_id,
        client_auth,
    })
}

/// Failure decoding a stored platform OIDC connection.
#[derive(Debug, thiserror::Error)]
pub enum PlatformConnectionError {
    /// The stored issuer URL is not a valid issuer.
    #[error("stored platform issuer url is invalid: {0}")]
    IssuerUrl(String),
    /// The stored JWKS URL is not a valid URL.
    #[error("stored platform jwks uri is invalid: {0}")]
    JwksUri(String),
    /// The client authentication could not be reconstructed.
    #[error("stored platform client authentication is unusable: {0}")]
    ClientAuth(String),
    /// The claim mapping payload could not be decoded.
    #[error("stored platform claim mapping is invalid: {0}")]
    ClaimMapping(String),
}

/// Seal the platform connection's client secret for storage, bound to the
/// platform scope, `issuer_url`, and `client_id`.
///
/// # Errors
/// Returns [`IssuerSealError::KmsMissing`] when a secret is present and no
/// `[kms]` is configured, and [`IssuerSealError::Encrypt`] when sealing
/// fails. A secret-bearing connection is never stored in the clear.
pub async fn seal_platform_client_secret(
    issuer_url: &str,
    client_id: &str,
    client_auth: &ClientAuth,
    kms: Option<&WyrdKms>,
) -> Result<Option<Vec<u8>>, IssuerSealError> {
    ClientSecretOwner {
        scope: SecretScope::Platform,
        purpose: SecretPurpose::PlatformOidc,
        issuer_url,
        client_id,
        connection_id: None,
    }
    .seal(client_auth, kms)
    .await
}

/// Renew the platform connection's client-secret encryption: the same secret
/// resealed under a fresh data key for the same issuer, client id, and
/// method. Callers run this before opening the transaction that swaps it in.
///
/// # Errors
/// Returns [`IssuerSealError::KmsMissing`] without a facade and
/// [`IssuerSealError::Encrypt`] when the envelope does not open for this
/// connection or the reseal fails.
pub async fn renew_platform_client_secret(
    row: &PlatformOidcConnectionRow,
    envelope: &[u8],
    kms: Option<&WyrdKms>,
) -> Result<Vec<u8>, IssuerSealError> {
    ClientSecretOwner {
        scope: SecretScope::Platform,
        purpose: SecretPurpose::PlatformOidc,
        issuer_url: &row.issuer_url,
        client_id: &row.client_id,
        connection_id: None,
    }
    .renew_secret(&row.client_auth, envelope, kms)
    .await
}

/// Name the stored discriminant for a client authentication method.
#[must_use]
pub fn client_auth_label(auth: &ClientAuth) -> &'static str {
    client_auth_discriminant(auth)
}

// --------------------------------------------------------------------------
// TrustedIssuer -> write (seed path)
// --------------------------------------------------------------------------

/// Encode a [`TrustedIssuer`] into a [`TrustedIssuerWrite`] for boot seeding.
///
/// The client secret (for `SecretBasic`/`SecretPost`) is sealed into the
/// common envelope bound to the issuer's tenant, URL, client id, and method.
/// Callers run this before opening the transaction that writes it, since a
/// remote backend waits on its key service. A secret-bearing issuer with no
/// facade fails closed via [`IssuerSealError::KmsMissing`].
///
/// # Errors
/// Returns [`IssuerSealError`] when a secret is present without a facade,
/// sealing fails, or a JSONB column cannot be serialized.
pub async fn issuer_write_from_trusted(
    issuer: &TrustedIssuer,
    kms: Option<&WyrdKms>,
) -> Result<TrustedIssuerWrite, IssuerSealError> {
    let client_secret_enc = ClientSecretOwner {
        scope: SecretScope::Tenant(issuer.tenant_id),
        purpose: SecretPurpose::AuthTrustedIssuer,
        issuer_url: issuer.issuer.as_str(),
        client_id: &issuer.client_id,
        connection_id: None,
    }
    .seal(&issuer.client_auth, kms)
    .await?;

    Ok(TrustedIssuerWrite {
        issuer_url: issuer.issuer.as_str().to_owned(),
        jwks_uri: issuer.jwks_uri.to_string(),
        expected_audience: issuer.expected_audience.clone(),
        client_id: issuer.client_id.clone(),
        client_auth: client_auth_discriminant(&issuer.client_auth).to_owned(),
        claim_mapping: claim_mapping_to_value(&issuer.claim_mapping)?,
        group_role_map: serde_json::to_value(&issuer.group_role_map)?,
        default_roles: serde_json::to_value(&issuer.default_roles)?,
        principal_kind: principal_kind_discriminant(issuer.principal_kind).to_owned(),
        jwks_ttl_secs: i64::try_from(issuer.jwks_ttl.as_secs()).unwrap_or(i64::MAX),
        client_secret_enc,
    })
}

/// Renew a stored trusted issuer's client-secret encryption: the same secret
/// resealed under a fresh data key, bound to the same tenant, issuer URL,
/// client id, and method. Callers run this outside any transaction, since a
/// remote backend waits on its key service, then swap on the observed bytes.
///
/// # Errors
/// Returns [`IssuerSealError::KmsMissing`] without a facade and
/// [`IssuerSealError::Encrypt`] when the envelope does not open for this
/// issuer or the reseal fails.
pub async fn renew_trusted_issuer_secret(
    tenant: DataTenantId,
    row: &TrustedIssuerRow,
    envelope: &[u8],
    kms: Option<&WyrdKms>,
) -> Result<Vec<u8>, IssuerSealError> {
    ClientSecretOwner {
        scope: SecretScope::Tenant(tenant),
        purpose: SecretPurpose::AuthTrustedIssuer,
        issuer_url: &row.issuer_url,
        client_id: &row.client_id,
        connection_id: None,
    }
    .renew_secret(&row.client_auth, envelope, kms)
    .await
}

/// Seal `secret` as the replacement client secret of a stored trusted issuer,
/// bound to its tenant, issuer URL, client id, and method. The old envelope is
/// never opened, so a replacement succeeds when it can no longer decrypt.
/// Callers run this outside any transaction, then swap on the observed bytes.
///
/// # Errors
/// Returns [`IssuerSealError::KmsMissing`] without a facade and
/// [`IssuerSealError::Encrypt`] when sealing fails.
pub async fn seal_trusted_issuer_replacement(
    tenant: DataTenantId,
    row: &TrustedIssuerRow,
    secret: &[u8],
    kms: Option<&WyrdKms>,
) -> Result<Vec<u8>, IssuerSealError> {
    ClientSecretOwner {
        scope: SecretScope::Tenant(tenant),
        purpose: SecretPurpose::AuthTrustedIssuer,
        issuer_url: &row.issuer_url,
        client_id: &row.client_id,
        connection_id: None,
    }
    .seal_secret(&row.client_auth, secret, kms)
    .await
}

/// Encode a [`WorkloadBinding`] into a [`WorkloadBindingWrite`] for boot seeding.
#[must_use]
pub fn binding_write_from_binding(binding: &WorkloadBinding) -> WorkloadBindingWrite {
    WorkloadBindingWrite {
        issuer_url: binding.issuer.as_str().to_owned(),
        subject: binding.subject.clone(),
        audience: binding.audience.clone(),
        principal_id: binding.principal_id.as_uuid(),
        workload_uid: binding.workload_uid.as_ref().map(CardUid::as_uuid),
    }
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use uuid::Uuid;

    use secrecy::{ExposeSecret, SecretString};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, TrustedIssuer, WorkloadBinding,
        WorkloadBindingResolver,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_kms::{KmsConfig, WyrdKms};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_sql::queries::auth::{upsert_trusted_issuer, upsert_workload_binding};

    use super::{
        IssuerDecodeError, IssuerSealError, PgIssuerResolver, PgWorkloadBindingResolver,
        binding_write_from_binding, human_connection_trusted_issuer, issuer_write_from_trusted,
        trusted_issuer_from_row,
    };
    use crate::exchange_api_key::pg_tests::insert_test_service_account;

    const ISSUER_URL: &str = "https://idp.example.com/realms/wyrd";

    /// An ephemeral memory facade with its own random key.
    ///
    /// # Panics
    /// Panics when the memory configuration stops validating.
    fn kms() -> Arc<WyrdKms> {
        Arc::new(WyrdKms::new(&KmsConfig::memory("test"), false).expect("memory facade"))
    }

    fn sample_issuer(tenant: DataTenantId) -> TrustedIssuer {
        let mut group_role_map = HashMap::new();
        group_role_map.insert("idp-admins".to_owned(), vec!["admin".to_owned()]);
        TrustedIssuer {
            tenant_id: tenant,
            issuer: IssuerUrl::new(ISSUER_URL).expect("issuer url is valid"),
            jwks_uri: format!("{ISSUER_URL}/protocol/openid-connect/certs")
                .parse()
                .expect("jwks uri is valid"),
            expected_audience: "wyrd-api".to_owned(),
            client_id: "wyrd-client".to_owned(),
            client_auth: ClientAuth::SecretPost(SecretString::from("super-secret".to_owned())),
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: Some(ClaimPath::new("email")),
                groups: Some(ClaimPath::new("realm_access.roles")),
            },
            group_role_map,
            default_roles: vec!["viewer".to_owned()],
            principal_kind: IssuerTokenPolicy::Workload,
            jwks_ttl: Duration::from_mins(30),
        }
    }

    fn assert_issuer_eq(expected: &TrustedIssuer, actual: &TrustedIssuer) {
        assert_eq!(actual.tenant_id, expected.tenant_id);
        assert_eq!(actual.issuer.as_str(), expected.issuer.as_str());
        assert_eq!(actual.jwks_uri, expected.jwks_uri);
        assert_eq!(actual.expected_audience, expected.expected_audience);
        assert_eq!(actual.client_id, expected.client_id);
        assert_eq!(actual.principal_kind, expected.principal_kind);
        assert_eq!(actual.default_roles, expected.default_roles);
        assert_eq!(actual.group_role_map, expected.group_role_map);
        assert_eq!(actual.jwks_ttl, expected.jwks_ttl);
        assert_eq!(
            actual.claim_mapping.subject.as_str(),
            expected.claim_mapping.subject.as_str()
        );
        assert_eq!(
            actual.claim_mapping.groups.as_ref().map(ClaimPath::as_str),
            expected
                .claim_mapping
                .groups
                .as_ref()
                .map(ClaimPath::as_str)
        );
    }

    #[tokio::test]
    async fn round_trips_through_write_and_row_losslessly() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let key = kms();
        let issuer = sample_issuer(tenant);

        let write = issuer_write_from_trusted(&issuer, Some(&key))
            .await
            .expect("encode succeeds");
        let row = wyrd_sql::row_types::auth::TrustedIssuerRow {
            data_tenant_id: tenant.as_uuid(),
            issuer_url: write.issuer_url,
            jwks_uri: write.jwks_uri,
            expected_audience: write.expected_audience,
            client_id: write.client_id,
            client_auth: write.client_auth,
            claim_mapping: write.claim_mapping,
            group_role_map: write.group_role_map,
            default_roles: write.default_roles,
            principal_kind: write.principal_kind,
            jwks_ttl_secs: write.jwks_ttl_secs,
            client_secret_enc: write.client_secret_enc,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let decoded = trusted_issuer_from_row(tenant, row, Some(&key))
            .await
            .expect("decode succeeds");
        assert_issuer_eq(&issuer, &decoded);
        match decoded.client_auth {
            ClientAuth::SecretPost(secret) => assert_eq!(secret.expose_secret(), "super-secret"),
            other => panic!("expected SecretPost, got {other:?}"),
        }
    }

    /// A stored human connection row mapping `subject` as `email` fails closed
    /// at decode, while the same row mapping `sub` decodes with its email and
    /// group paths intact.
    ///
    /// # Panics
    ///
    /// Panics when the `email` subject row decodes, its error is not
    /// `HumanSubject("email")`, or the `sub` row fails to decode with its
    /// subject and email paths.
    #[tokio::test]
    async fn stored_human_connection_requires_the_sub_subject_claim() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let row = |subject: &str| wyrd_sql::row_types::auth::HumanConnectionRow {
            connection_id: Uuid::new_v4(),
            data_tenant_id: tenant.as_uuid(),
            revision: 1,
            state: "Active".to_owned(),
            issuer_url: ISSUER_URL.to_owned(),
            client_id: "wyrd".to_owned(),
            client_auth: "Public".to_owned(),
            client_secret_enc: None,
            claim_mapping: serde_json::json!({
                "subject": subject, "email": "email", "groups": "groups"
            }),
            group_role_map: serde_json::json!({}),
            jwks_ttl_secs: 300,
            jwks_uri: Some(format!("{ISSUER_URL}/jwks")),
            tested_revision: None,
            tested_until: None,
            removed_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let error = human_connection_trusted_issuer(tenant, row("email"), None)
            .await
            .expect_err("a stored non-sub subject fails closed");
        assert!(matches!(error, IssuerDecodeError::HumanSubject(path) if path == "email"));

        let trusted = human_connection_trusted_issuer(tenant, row("sub"), None)
            .await
            .expect("a sub mapping decodes");
        assert_eq!(trusted.claim_mapping.subject.as_str(), "sub");
        assert_eq!(
            trusted.claim_mapping.email.as_ref().map(ClaimPath::as_str),
            Some("email")
        );
    }

    #[tokio::test]
    async fn encode_fails_closed_when_secret_present_without_kms() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let issuer = sample_issuer(tenant);

        let error = issuer_write_from_trusted(&issuer, None)
            .await
            .expect_err("a secret-bearing issuer with no facade must fail closed");
        assert!(matches!(error, IssuerSealError::KmsMissing));
    }

    /// A stored secret opens only with a facade, under its own tenant, and
    /// under the facade holding its key; every other read fails closed.
    ///
    /// # Panics
    /// Panics when encoding fails or a keyless, cross-tenant, or other-key
    /// decode succeeds.
    #[tokio::test]
    async fn decode_fails_closed_without_the_owner_or_key() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let key = kms();
        let issuer = sample_issuer(tenant);
        let write = issuer_write_from_trusted(&issuer, Some(&key))
            .await
            .expect("encode succeeds");
        let row = wyrd_sql::row_types::auth::TrustedIssuerRow {
            data_tenant_id: tenant.as_uuid(),
            issuer_url: write.issuer_url,
            jwks_uri: write.jwks_uri,
            expected_audience: write.expected_audience,
            client_id: write.client_id,
            client_auth: write.client_auth,
            claim_mapping: write.claim_mapping,
            group_role_map: write.group_role_map,
            default_roles: write.default_roles,
            principal_kind: write.principal_kind,
            jwks_ttl_secs: write.jwks_ttl_secs,
            client_secret_enc: write.client_secret_enc,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let keyless = trusted_issuer_from_row(tenant, row.clone(), None)
            .await
            .expect_err("opening a secret with no facade must fail closed");
        assert!(matches!(keyless, IssuerDecodeError::KmsMissing));
        let foreign = trusted_issuer_from_row(DataTenantId::new_v7(), row.clone(), Some(&key))
            .await
            .expect_err("a secret copied to another tenant must not open");
        assert!(matches!(foreign, IssuerDecodeError::Decrypt));
        let other = trusted_issuer_from_row(tenant, row, Some(&kms()))
            .await
            .expect_err("another deployment key must not open the secret");
        assert!(matches!(other, IssuerDecodeError::Decrypt));
    }

    /// Through the resolver the jwt-bearer exchange uses, a trusted issuer
    /// whose stored envelope was copied from another tenant's issuer for the
    /// same URL fails closed instead of serving that tenant's secret, while
    /// the source tenant still resolves its own.
    ///
    /// # Panics
    /// Panics when seeding or the copy fails, the copied row resolves, or the
    /// source tenant's issuer stops resolving with its secret.
    #[tokio::test]
    async fn resolver_refuses_an_envelope_copied_from_another_tenant() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let first = fixture.data_tenant_id();
        let second = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(second, "issuer-copied")
            .await
            .expect("second tenant seeds");
        let key = kms();
        for (tenant, secret) in [(first, "first-secret"), (second, "second-secret")] {
            let mut issuer = sample_issuer(tenant);
            issuer.client_auth = ClientAuth::SecretPost(SecretString::from(secret.to_owned()));
            let write = issuer_write_from_trusted(&issuer, Some(&key))
                .await
                .expect("encode succeeds");
            let mut conn = fixture
                .tenant_conn_for(tenant)
                .await
                .expect("tenant conn opens");
            upsert_trusted_issuer(&mut conn, &write)
                .await
                .expect("issuer upsert");
            conn.commit().await.expect("seed commits");
        }
        let superuser = fixture.superuser_pool().expect("superuser pool opens");
        sqlx::query(
            "UPDATE wyrd.auth_trusted_issuers SET client_secret_enc = \
               (SELECT client_secret_enc FROM wyrd.auth_trusted_issuers WHERE data_tenant_id = $1) \
             WHERE data_tenant_id = $2",
        )
        .bind(second.as_uuid())
        .bind(first.as_uuid())
        .execute(&superuser)
        .await
        .expect("envelope copies");

        let resolver = PgIssuerResolver::new(fixture.wyrd_postgres().clone(), Some(key));
        let url = IssuerUrl::new(ISSUER_URL).expect("issuer url is valid");
        assert!(
            resolver.trusted_issuer(&first, &url).await.is_err(),
            "the copied envelope does not open for the first tenant"
        );
        let own = resolver
            .trusted_issuer(&second, &url)
            .await
            .expect("the source tenant resolves")
            .expect("its issuer is found");
        match &own.client_auth {
            ClientAuth::SecretPost(secret) => assert_eq!(secret.expose_secret(), "second-secret"),
            other => panic!("expected SecretPost, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn resolver_reads_and_decrypts_seeded_issuer() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = kms();
        let issuer = sample_issuer(tenant);
        let write = issuer_write_from_trusted(&issuer, Some(&key))
            .await
            .expect("encode succeeds");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("issuer upsert");
        conn.commit().await.expect("seed commits");

        let resolver = PgIssuerResolver::new(fixture.wyrd_postgres().clone(), Some(key));
        let issuers = resolver
            .trusted_issuers(&tenant)
            .await
            .expect("resolve succeeds");

        assert_eq!(issuers.len(), 1);
        assert_issuer_eq(&issuer, &issuers[0]);
        match &issuers[0].client_auth {
            ClientAuth::SecretPost(secret) => assert_eq!(secret.expose_secret(), "super-secret"),
            other => panic!("expected SecretPost, got {other:?}"),
        }

        let keyed = resolver
            .trusted_issuer(&tenant, &issuer.issuer)
            .await
            .expect("keyed resolve succeeds")
            .expect("seeded issuer is found by URL");
        assert_issuer_eq(&issuer, &keyed);
        let unknown = IssuerUrl::new("https://unknown.example.com".to_owned()).expect("url");
        assert!(
            resolver
                .trusted_issuer(&tenant, &unknown)
                .await
                .expect("keyed miss succeeds")
                .is_none(),
            "an unconfigured issuer URL resolves to None"
        );
    }

    /// A `NULL`-audience binding answers an audience-qualified lookup through
    /// [`PgWorkloadBindingResolver`], and an unbound subject resolves to `None`.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or seed writes fail, a lookup errors, the
    /// fallback lookup misses or returns another account or root, or the
    /// unbound subject resolves.
    #[tokio::test]
    async fn binding_resolver_prefers_audience_then_falls_back() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = kms();
        let issuer = sample_issuer(tenant);
        let issuer_write = issuer_write_from_trusted(&issuer, Some(&key))
            .await
            .expect("encode succeeds");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let account = insert_test_service_account(&mut conn, Uuid::now_v7(), None).await;
        let binding = WorkloadBinding {
            tenant_id: tenant,
            issuer: issuer.issuer.clone(),
            subject: "system:serviceaccount:default/my-sa".to_owned(),
            audience: None,
            principal_id: PrincipalId::new(account),
            workload_uid: None,
        };
        let binding_write = binding_write_from_binding(&binding);

        upsert_trusted_issuer(&mut conn, &issuer_write)
            .await
            .expect("issuer upsert");
        upsert_workload_binding(&mut conn, &binding_write)
            .await
            .expect("binding upsert");
        conn.commit().await.expect("seed commits");

        let resolver = PgWorkloadBindingResolver::new(fixture.wyrd_postgres().clone());

        // A NULL-audience binding answers an audience-qualified lookup (fallback).
        let resolved_binding = resolver
            .binding(
                &tenant,
                &issuer.issuer,
                "system:serviceaccount:default/my-sa",
                Some("any-audience"),
            )
            .await
            .expect("lookup ok")
            .expect("subject resolves via fallback");
        assert_eq!(resolved_binding.principal_id, binding.principal_id);
        assert_eq!(resolved_binding.workload_uid, None);

        // An unbound subject resolves to None.
        let unbound = resolver
            .binding(
                &tenant,
                &issuer.issuer,
                "system:serviceaccount:default/other-sa",
                None,
            )
            .await
            .expect("lookup ok");
        assert!(unbound.is_none());
    }
}
