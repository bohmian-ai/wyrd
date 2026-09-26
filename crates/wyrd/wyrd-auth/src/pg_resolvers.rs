//! Postgres-backed trusted-issuer and workload-binding resolvers.
//!
//! These are the production implementations of [`IssuerConfigResolver`] and
//! [`WorkloadBindingResolver`]. They live in `wyrd-server` (not the SQL-free
//! `wyrd-auth-oidc` crate) because they hold the [`PgPool`] and the process-wide
//! sealing key, and call commit 03's tenant-scoped read-path query functions.
//!
//! Issuer trust is tenant-scoped (F02): every read goes through a
//! [`TenantConn`](wyrd_sql::TenantConn), so Postgres RLS is the load-bearing isolation boundary. A
//! resolve for tenant A can never surface tenant B's issuers or bindings.
//!
//! The `client_secret_enc` BYTEA column stores `nonce ‖ ciphertext` (AES-256-GCM
//! via `wyrd-crypt`). It is decrypted on read with the sealing key. A row that
//! carries a secret but no sealing key is configured fails closed (a 503-class
//! `JwksUnavailable`), never a silent drop that would masquerade as an untrusted
//! issuer (401).

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
use wyrd_crypt::SealingKeyring;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::HUMAN_SUBJECT_CLAIM;
use wyrd_spec::auth::IssuerTokenPolicy;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::reference::CardRef;
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
/// process-wide sealing keyring. The keyring is `Option` because a deployment
/// with only secret-free issuers (`PrivateKeyJwt`/`Public`) needs none;
/// decryption is only attempted for rows that actually carry a sealed secret.
#[derive(Clone)]
pub struct PgIssuerResolver {
    /// Runtime Postgres handle; every read runs on an RLS [`TenantConn`](wyrd_sql::TenantConn)
    /// acquired through [`WyrdPostgres::tenant_conn`], so a resolve for one
    /// tenant can never observe another tenant's issuers.
    postgres: WyrdPostgres,
    /// Keyring that opens sealed client secrets, `None` on a keyless
    /// deployment. A row carrying a secret with no keyring fails closed with
    /// [`IssuerDecodeError::SealingKeyMissing`] instead of being dropped.
    sealing_key: Option<Arc<SealingKeyring>>,
}

/// Redacted debug view: names the keyring presence and hides the store handle.
impl std::fmt::Debug for PgIssuerResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgIssuerResolver")
            .field("sealing_key", &self.sealing_key)
            .finish_non_exhaustive()
    }
}

impl PgIssuerResolver {
    /// Construct a resolver over the runtime Postgres handle with an optional
    /// sealing keyring.
    #[must_use]
    pub fn new(postgres: WyrdPostgres, sealing_key: Option<Arc<SealingKeyring>>) -> Self {
        Self {
            postgres,
            sealing_key,
        }
    }
}

impl IssuerConfigResolver for PgIssuerResolver {
    /// List every trusted issuer configured for `tenant`, decrypting sealed
    /// client secrets with the process sealing key.
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

        let sealing_key = self.sealing_key.as_deref();
        rows.into_iter()
            .map(|row| {
                let issuer_url = row.issuer_url.clone();
                trusted_issuer_from_row(*tenant, row, sealing_key).map_err(|error| {
                    OidcError::JwksUnavailable {
                        issuer: issuer_url,
                        message: error.to_string(),
                    }
                })
            })
            .collect()
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
        row.map(|row| {
            trusted_issuer_from_row(*tenant, row, self.sealing_key.as_deref()).map_err(|error| {
                OidcError::JwksUnavailable {
                    issuer: issuer.as_str().to_owned(),
                    message: error.to_string(),
                }
            })
        })
        .transpose()
    }
}

// --------------------------------------------------------------------------
// PgWorkloadBindingResolver
// --------------------------------------------------------------------------

/// Production [`WorkloadBindingResolver`] backed by `wyrd.auth_workload_bindings`.
///
/// Holds only the runtime [`WyrdPostgres`] handle — bindings carry no
/// secrets, so no sealing key is needed. Each lookup opens its tenant
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
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant, issuer = %issuer))]
    async fn binding(
        &self,
        tenant: &DataTenantId,
        issuer: &IssuerUrl,
        subject: &str,
        audience: Option<&str>,
    ) -> Result<Option<CardRef>, OidcError> {
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
        Ok(row.map(|row| row.card_ref))
    }
}

// --------------------------------------------------------------------------
// Seal errors (write/encode path)
// --------------------------------------------------------------------------

/// Failure encoding a [`TrustedIssuer`] into a [`TrustedIssuerWrite`] for seeding.
#[derive(Debug, thiserror::Error)]
pub enum IssuerSealError {
    /// The issuer authenticates with a client secret but no sealing key is
    /// configured. Boot fails closed rather than persist a plaintext or empty
    /// secret column.
    #[error("trusted issuer carries a client secret but no sealing key is configured")]
    SealingKeyMissing,
    /// AES-GCM encryption of the client secret failed.
    #[error("failed to encrypt trusted issuer client secret")]
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
    /// The row carries a sealed secret but the process holds no keyring.
    #[error("client secret present but no sealing key is configured")]
    SealingKeyMissing,
    /// No held sealing key opens the stored ciphertext, or it is malformed
    /// or tampered.
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
// Secret sealing (versioned keyring envelope)
// --------------------------------------------------------------------------

/// Seal `plaintext` under the keyring's write key for a BYTEA column.
///
/// # Errors
/// Returns [`IssuerSealError::Encrypt`] when encryption fails.
pub(crate) fn seal_secret(
    keyring: &SealingKeyring,
    plaintext: &[u8],
) -> Result<Vec<u8>, IssuerSealError> {
    keyring
        .seal(plaintext)
        .map_err(|_| IssuerSealError::Encrypt)
}

/// Open a stored sealed value with whichever held key it names.
///
/// The versioned envelope names its sealing key, so a secret sealed under a
/// retained (pre-rotation) key still opens while the rewrap catches up. The
/// underlying crypto error is dropped so no key or ciphertext detail leaks.
///
/// # Errors
/// Returns [`IssuerDecodeError::Decrypt`] when the envelope is malformed, names
/// a key the keyring does not hold, or fails authentication.
fn open_secret(keyring: &SealingKeyring, bytes: &[u8]) -> Result<Vec<u8>, IssuerDecodeError> {
    keyring.open(bytes).map_err(|_| IssuerDecodeError::Decrypt)
}

/// Open a row's sealed client secret into a [`SecretString`].
///
/// Called only for secret-bearing client-authentication methods; the
/// plaintext never leaves the returned secret wrapper.
///
/// # Errors
/// Returns [`IssuerDecodeError::MissingSecret`] when the row stores no
/// ciphertext, [`IssuerDecodeError::SealingKeyMissing`] when the process holds
/// no keyring, [`IssuerDecodeError::Decrypt`] when no held key opens it, and
/// [`IssuerDecodeError::SecretEncoding`] when the plaintext is not UTF-8.
fn decode_secret(
    secret_enc: Option<&[u8]>,
    sealing_key: Option<&SealingKeyring>,
) -> Result<SecretString, IssuerDecodeError> {
    let bytes = secret_enc.ok_or(IssuerDecodeError::MissingSecret)?;
    let key = sealing_key.ok_or(IssuerDecodeError::SealingKeyMissing)?;
    let plaintext = open_secret(key, bytes)?;
    let text = String::from_utf8(plaintext).map_err(|_| IssuerDecodeError::SecretEncoding)?;
    Ok(SecretString::from(text))
}

/// Rebuild a stored client-authentication method, opening its sealed secret.
///
/// Shared by the workload issuer, platform connection, and tenant human
/// connection decoders so every store opens secrets through one keyring path.
///
/// # Errors
/// Returns [`IssuerDecodeError`] for an unknown discriminant, a secret method
/// with no stored secret, a missing keyring, or a secret no held key opens.
pub(crate) fn client_auth_from_row(
    discriminant: &str,
    secret_enc: Option<&[u8]>,
    sealing_key: Option<&SealingKeyring>,
) -> Result<ClientAuth, IssuerDecodeError> {
    match discriminant {
        CLIENT_AUTH_SECRET_BASIC => Ok(ClientAuth::SecretBasic(decode_secret(
            secret_enc,
            sealing_key,
        )?)),
        CLIENT_AUTH_SECRET_POST => Ok(ClientAuth::SecretPost(decode_secret(
            secret_enc,
            sealing_key,
        )?)),
        CLIENT_AUTH_PRIVATE_KEY_JWT => Ok(ClientAuth::PrivateKeyJwt),
        CLIENT_AUTH_PUBLIC => Ok(ClientAuth::Public),
        other => Err(IssuerDecodeError::ClientAuth(other.to_owned())),
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
fn trusted_issuer_from_row(
    tenant: DataTenantId,
    row: TrustedIssuerRow,
    sealing_key: Option<&SealingKeyring>,
) -> Result<TrustedIssuer, IssuerDecodeError> {
    let issuer = IssuerUrl::new(row.issuer_url.clone())
        .map_err(|error| IssuerDecodeError::IssuerUrl(error.to_string()))?;
    let jwks_uri =
        Url::parse(&row.jwks_uri).map_err(|error| IssuerDecodeError::JwksUri(error.to_string()))?;
    let client_auth = client_auth_from_row(
        &row.client_auth,
        row.client_secret_enc.as_deref(),
        sealing_key,
    )?;
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
/// be reconstructed — including a sealed secret this process holds no key for.
pub(crate) fn human_connection_trusted_issuer(
    tenant: DataTenantId,
    row: HumanConnectionRow,
    sealing_key: Option<&SealingKeyring>,
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
    let client_auth = client_auth_from_row(
        &row.client_auth,
        row.client_secret_enc.as_deref(),
        sealing_key,
    )?;
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
/// including when the row carries a secret and no sealing key is configured,
/// which fails closed rather than degrading to an unauthenticated client.
pub fn platform_connection_from_row(
    row: PlatformOidcConnectionRow,
    sealing_key: Option<&SealingKeyring>,
) -> Result<PlatformConnection, PlatformConnectionError> {
    let issuer = IssuerUrl::new(row.issuer_url.clone())
        .map_err(|error| PlatformConnectionError::IssuerUrl(error.to_string()))?;
    let jwks_uri = Url::parse(&row.jwks_uri)
        .map_err(|error| PlatformConnectionError::JwksUri(error.to_string()))?;
    let client_auth = client_auth_from_row(
        &row.client_auth,
        row.client_secret_enc.as_deref(),
        sealing_key,
    )
    .map_err(|error| PlatformConnectionError::ClientAuth(error.to_string()))?;
    let claim_mapping = claim_mapping_from_value(row.claim_mapping)
        .map_err(|error| PlatformConnectionError::ClaimMapping(error.to_string()))?;

    Ok(crate::platform_login::PlatformConnection {
        verification: IssuerVerification {
            issuer,
            jwks_uri,
            expected_audience: row.expected_audience,
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

/// Seal a platform connection's client secret for storage.
///
/// # Errors
/// Returns [`IssuerSealError::SealingKeyMissing`] when a secret is present and
/// no sealing key is configured, and [`IssuerSealError::Encrypt`] when
/// encryption fails. A secret-bearing connection is never stored in the clear.
pub fn seal_platform_client_secret(
    client_auth: &ClientAuth,
    sealing_key: Option<&SealingKeyring>,
) -> Result<Option<Vec<u8>>, IssuerSealError> {
    match client_auth {
        ClientAuth::SecretBasic(secret) | ClientAuth::SecretPost(secret) => {
            let key = sealing_key.ok_or(IssuerSealError::SealingKeyMissing)?;
            Ok(Some(seal_secret(key, secret.expose_secret().as_bytes())?))
        }
        ClientAuth::PrivateKeyJwt | ClientAuth::Public => Ok(None),
    }
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
/// The client secret (for `SecretBasic`/`SecretPost`) is encrypted with the
/// sealing key into `nonce ‖ ciphertext`. A secret-bearing issuer with no
/// sealing key fails closed via [`IssuerSealError::SealingKeyMissing`].
///
/// # Errors
/// Returns [`IssuerSealError`] when a secret is present without a sealing key,
/// encryption fails, or a JSONB column cannot be serialized.
pub fn issuer_write_from_trusted(
    issuer: &TrustedIssuer,
    sealing_key: Option<&SealingKeyring>,
) -> Result<TrustedIssuerWrite, IssuerSealError> {
    let client_secret_enc = match &issuer.client_auth {
        ClientAuth::SecretBasic(secret) | ClientAuth::SecretPost(secret) => {
            let key = sealing_key.ok_or(IssuerSealError::SealingKeyMissing)?;
            Some(seal_secret(key, secret.expose_secret().as_bytes())?)
        }
        ClientAuth::PrivateKeyJwt | ClientAuth::Public => None,
    };

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

/// Encode a [`WorkloadBinding`] into a [`WorkloadBindingWrite`] for boot seeding.
///
/// # Errors
/// Returns a [`serde_json::Error`] when the structured `card_ref` cannot be
/// serialized to JSONB.
pub fn binding_write_from_binding(
    binding: &WorkloadBinding,
) -> Result<WorkloadBindingWrite, serde_json::Error> {
    Ok(WorkloadBindingWrite {
        issuer_url: binding.issuer.as_str().to_owned(),
        subject: binding.subject.clone(),
        audience: binding.audience.clone(),
        card_ref: serde_json::to_value(&binding.card_ref)?,
    })
}

#[cfg(test)]
mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use secrecy::{ExposeSecret, SecretString};
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, TrustedIssuer, WorkloadBinding,
        WorkloadBindingResolver,
    };
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::IssuerUrl;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;
    use wyrd_sql::queries::auth::{upsert_trusted_issuer, upsert_workload_binding};

    use super::{
        IssuerDecodeError, IssuerSealError, PgIssuerResolver, PgWorkloadBindingResolver,
        binding_write_from_binding, human_connection_trusted_issuer, issuer_write_from_trusted,
        trusted_issuer_from_row,
    };

    const ISSUER_URL: &str = "https://idp.example.com/realms/wyrd";

    fn sealing_key() -> SealingKeyring {
        SealingKeyring::new(SecretKey::from_bytes([7_u8; 32]))
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

    #[test]
    fn round_trips_through_write_and_row_losslessly() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let key = sealing_key();
        let issuer = sample_issuer(tenant);

        let write = issuer_write_from_trusted(&issuer, Some(&key)).expect("encode succeeds");
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

        let decoded = trusted_issuer_from_row(tenant, row, Some(&key)).expect("decode succeeds");
        assert_issuer_eq(&issuer, &decoded);
        match decoded.client_auth {
            ClientAuth::SecretPost(secret) => assert_eq!(secret.expose_secret(), "super-secret"),
            other => panic!("expected SecretPost, got {other:?}"),
        }
    }

    /// A stored human connection row mapping `subject` as `email` fails closed
    /// at decode, while the same row mapping `sub` decodes with its email and
    /// group paths intact.
    #[test]
    fn stored_human_connection_requires_the_sub_subject_claim() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let row = |subject: &str| wyrd_sql::row_types::auth::HumanConnectionRow {
            connection_id: uuid::Uuid::new_v4(),
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
            .expect_err("a stored non-sub subject fails closed");
        assert!(matches!(error, IssuerDecodeError::HumanSubject(path) if path == "email"));

        let trusted = human_connection_trusted_issuer(tenant, row("sub"), None)
            .expect("a sub mapping decodes");
        assert_eq!(trusted.claim_mapping.subject.as_str(), "sub");
        assert_eq!(
            trusted.claim_mapping.email.as_ref().map(ClaimPath::as_str),
            Some("email")
        );
    }

    #[test]
    fn encode_fails_closed_when_secret_present_without_sealing_key() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let issuer = sample_issuer(tenant);

        let error = issuer_write_from_trusted(&issuer, None)
            .expect_err("a secret-bearing issuer with no sealing key must fail closed");
        assert!(matches!(error, IssuerSealError::SealingKeyMissing));
    }

    #[test]
    fn decode_fails_closed_when_secret_present_without_sealing_key() {
        let tenant: DataTenantId = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("tenant id is valid");
        let key = sealing_key();
        let issuer = sample_issuer(tenant);
        let write = issuer_write_from_trusted(&issuer, Some(&key)).expect("encode succeeds");
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

        let error = trusted_issuer_from_row(tenant, row, None)
            .expect_err("decrypting a secret with no key must fail closed");
        assert!(format!("{error}").contains("no sealing key"));
    }

    #[tokio::test]
    async fn resolver_reads_and_decrypts_seeded_issuer() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = sealing_key();
        let issuer = sample_issuer(tenant);
        let write = issuer_write_from_trusted(&issuer, Some(&key)).expect("encode succeeds");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        upsert_trusted_issuer(&mut conn, &write)
            .await
            .expect("issuer upsert");
        conn.commit().await.expect("seed commits");

        let resolver = PgIssuerResolver::new(
            fixture.wyrd_postgres().clone(),
            Some(Arc::new(sealing_key())),
        );
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

    #[tokio::test]
    async fn binding_resolver_prefers_audience_then_falls_back() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let key = sealing_key();
        let issuer = sample_issuer(tenant);
        let issuer_write = issuer_write_from_trusted(&issuer, Some(&key)).expect("encode succeeds");

        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("my-model").expect("card name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: Some(SpaceName::new("prod").expect("space")),
            uid: None,
        };
        let binding = WorkloadBinding {
            tenant_id: tenant,
            issuer: issuer.issuer.clone(),
            subject: "system:serviceaccount:default/my-sa".to_owned(),
            audience: None,
            card_ref: card_ref.clone(),
        };
        let binding_write = binding_write_from_binding(&binding).expect("binding encodes");

        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
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
        assert_eq!(resolved_binding.name.to_string(), "my-model");

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
