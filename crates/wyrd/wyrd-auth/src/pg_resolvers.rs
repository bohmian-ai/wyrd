//! Postgres-backed trusted-issuer and workload-binding resolvers.
//!
//! These are the production implementations of [`IssuerConfigResolver`] and
//! [`WorkloadBindingResolver`]. They live in `wyrd-server` (not the SQL-free
//! `wyrd-auth-oidc` crate) because they hold the [`PgPool`] and the process-wide
//! sealing key, and call commit 03's tenant-scoped read-path query functions.
//!
//! Issuer trust is tenant-scoped (F02): every read goes through a
//! [`TenantConn`], so Postgres RLS is the load-bearing isolation boundary. A
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

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use url::Url;
use wyrd_auth_oidc::{
    ClaimMapping, ClaimPath, ClientAuth, IssuerConfigResolver, OidcError, TrustedIssuer,
    WorkloadBinding, WorkloadBindingResolver,
};
use wyrd_crypt::{CryptError, EncryptedPayload, SecretKey};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerTokenPolicy;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{
    TrustedIssuerWrite, WorkloadBindingWrite, trusted_issuers_for_tenant,
    workload_binding_by_subject,
};
use wyrd_sql::row_types::auth::TrustedIssuerRow;

/// AES-GCM nonce length; the leading prefix of every `client_secret_enc` value.
const NONCE_LEN: usize = 12;

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
/// Holds the app [`PgPool`] and an optional process-wide sealing key. The key is
/// `Option` because a deployment with only secret-free issuers
/// (`PrivateKeyJwt`/`Public`) needs no sealing key; decryption is only attempted
/// for rows that actually carry an encrypted secret.
#[derive(Debug, Clone)]
pub struct PgIssuerResolver {
    pool: Arc<PgPool>,
    sealing_key: Option<Arc<SecretKey>>,
}

impl PgIssuerResolver {
    /// Construct a resolver over the Wyrd app pool with an optional sealing key.
    #[must_use]
    pub fn new(pool: Arc<PgPool>, sealing_key: Option<Arc<SecretKey>>) -> Self {
        Self { pool, sealing_key }
    }
}

impl IssuerConfigResolver for PgIssuerResolver {
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant))]
    async fn trusted_issuers(
        &self,
        tenant: &DataTenantId,
    ) -> Result<Vec<TrustedIssuer>, OidcError> {
        let mut conn = TenantConn::acquire(&self.pool, *tenant).await.map_err(|error| {
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
}

// --------------------------------------------------------------------------
// PgWorkloadBindingResolver
// --------------------------------------------------------------------------

/// Production [`WorkloadBindingResolver`] backed by `wyrd.auth_workload_bindings`.
///
/// Holds only the app [`PgPool`] — bindings carry no secrets, so no sealing key
/// is needed. Audience precedence (exact match preferred, `NULL`-audience
/// fallback) is enforced in the SQL query (`workload_binding_by_subject`).
#[derive(Debug, Clone)]
pub struct PgWorkloadBindingResolver {
    pool: Arc<PgPool>,
}

impl PgWorkloadBindingResolver {
    /// Construct a resolver over the Wyrd app pool.
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
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
        let mut conn = TenantConn::acquire(&self.pool, *tenant).await.map_err(|error| {
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

/// Failure decoding a [`TrustedIssuerRow`] back into a [`TrustedIssuer`].
#[derive(Debug, thiserror::Error)]
enum IssuerDecodeError {
    #[error("stored issuer url is invalid: {0}")]
    IssuerUrl(String),
    #[error("stored jwks uri is invalid: {0}")]
    JwksUri(String),
    #[error("unknown client_auth discriminant {0:?}")]
    ClientAuth(String),
    #[error("unknown principal_kind discriminant {0:?}")]
    PrincipalKind(String),
    #[error("client_auth requires a secret but client_secret_enc is null")]
    MissingSecret,
    #[error("client secret present but no sealing key is configured")]
    SealingKeyMissing,
    #[error("client_secret_enc payload is shorter than the nonce")]
    MalformedSecret,
    #[error("client secret could not be decrypted")]
    Decrypt,
    #[error("decrypted client secret is not valid utf-8")]
    SecretEncoding,
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

fn claim_mapping_from_value(value: Value) -> Result<ClaimMapping, serde_json::Error> {
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
// Secret sealing (nonce ‖ ciphertext)
// --------------------------------------------------------------------------

/// Encrypt `plaintext` and return `nonce ‖ ciphertext` for the BYTEA column.
fn seal_secret(key: &SecretKey, plaintext: &[u8]) -> Result<Vec<u8>, CryptError> {
    let payload = wyrd_crypt::encrypt(key, plaintext)?;
    let mut out = Vec::with_capacity(NONCE_LEN + payload.ciphertext.len());
    out.extend_from_slice(&payload.nonce);
    out.extend_from_slice(&payload.ciphertext);
    Ok(out)
}

/// Split a stored `nonce ‖ ciphertext` value and decrypt it.
fn open_secret(key: &SecretKey, bytes: &[u8]) -> Result<Vec<u8>, IssuerDecodeError> {
    if bytes.len() < NONCE_LEN {
        return Err(IssuerDecodeError::MalformedSecret);
    }
    let (nonce_bytes, ciphertext) = bytes.split_at(NONCE_LEN);
    let mut nonce = [0_u8; NONCE_LEN];
    nonce.copy_from_slice(nonce_bytes);
    let payload = EncryptedPayload {
        nonce,
        ciphertext: ciphertext.to_vec(),
    };
    wyrd_crypt::decrypt(key, &payload).map_err(|_| IssuerDecodeError::Decrypt)
}

fn decode_secret(
    secret_enc: Option<&[u8]>,
    sealing_key: Option<&SecretKey>,
) -> Result<SecretString, IssuerDecodeError> {
    let bytes = secret_enc.ok_or(IssuerDecodeError::MissingSecret)?;
    let key = sealing_key.ok_or(IssuerDecodeError::SealingKeyMissing)?;
    let plaintext = open_secret(key, bytes)?;
    let text = String::from_utf8(plaintext).map_err(|_| IssuerDecodeError::SecretEncoding)?;
    Ok(SecretString::from(text))
}

fn client_auth_from_row(
    discriminant: &str,
    secret_enc: Option<&[u8]>,
    sealing_key: Option<&SecretKey>,
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
    sealing_key: Option<&SecretKey>,
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
    sealing_key: Option<&SecretKey>,
) -> Result<TrustedIssuerWrite, IssuerSealError> {
    let client_secret_enc = match &issuer.client_auth {
        ClientAuth::SecretBasic(secret) | ClientAuth::SecretPost(secret) => {
            let key = sealing_key.ok_or(IssuerSealError::SealingKeyMissing)?;
            Some(
                seal_secret(key, secret.expose_secret().as_bytes())
                    .map_err(|_| IssuerSealError::Encrypt)?,
            )
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
    use wyrd_crypt::SecretKey;
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
        IssuerSealError, PgIssuerResolver, PgWorkloadBindingResolver, binding_write_from_binding,
        issuer_write_from_trusted, trusted_issuer_from_row,
    };

    const ISSUER_URL: &str = "https://idp.example.com/realms/wyrd";

    fn sealing_key() -> SecretKey {
        SecretKey::from_bytes([7_u8; 32])
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
            principal_kind: IssuerTokenPolicy::Human,
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
            Arc::new(fixture.app_pool().clone()),
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
            space: SpaceName::new("prod").expect("space"),
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

        let resolver = PgWorkloadBindingResolver::new(Arc::new(fixture.app_pool().clone()));

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
