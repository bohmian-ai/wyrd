//! API-key issuance for card-bound non-human principals.

use chrono::{Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;
use wyrd_auth_issue::{self, IssueError};
use wyrd_runtime::{Permission, Principal, PrincipalId};
use wyrd_spec::auth::{IssueKeyRequest, IssueKeyResponse, SecretBearer};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{insert_api_key, service_account_by_card_ref};

use crate::audit::{API_KEY_ISSUE_OPERATION, append_auth_audit, auth_event};

/// API-key issue settings.
#[derive(Debug, Clone)]
pub struct ApiKeySettings {
    /// Default API-key lifetime.
    pub default_ttl: Duration,
}

impl Default for ApiKeySettings {
    fn default() -> Self {
        Self {
            default_ttl: Duration::days(365),
        }
    }
}

/// API-key issue service.
#[derive(Debug, Clone, Default)]
pub struct IssueApiKey {
    /// Issue settings.
    pub settings: ApiKeySettings,
}

/// Internal result carrying non-wire fields needed by transactional audit.
#[derive(Debug, Clone)]
pub struct IssuedApiKey {
    /// Wire response.
    pub response: IssueKeyResponse,
    /// Target non-human principal id.
    pub service_account_id: Uuid,
    /// API key row id.
    pub api_key_id: Uuid,
}

/// API-key issuance failure.
#[derive(Debug, thiserror::Error)]
pub enum IssueKeyError {
    /// No active principal row exists for the `CardRef`.
    #[error("no principal bound to card_ref {0}")]
    ServiceAccountNotFound(String),
    /// Card kind cannot back a non-human principal.
    #[error("card kind {card_kind:?} cannot be bound to a non-human principal")]
    PrincipalKindCardKindMismatch {
        /// Supplied card kind.
        card_kind: CardKind,
    },
    /// Argon2 hash failed.
    #[error("api key hash failed")]
    Hash(#[from] IssueError),
    /// Blocking task failed.
    #[error("api key hash task failed")]
    Join(#[from] tokio::task::JoinError),
    /// Database operation failed.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
}

impl IssueApiKey {
    /// Issue an API key for a registered Service or Agent card.
    ///
    /// # Errors
    /// Returns a typed error when the card cannot be bound, hashing fails, or
    /// the database rejects the insert.
    #[tracing::instrument(
        level = "debug",
        skip(self, conn, request),
        fields(card_ref = %request.card_ref, actor = %actor.id),
        err,
    )]
    pub async fn execute(
        &self,
        conn: &mut TenantConn<'_>,
        request: IssueKeyRequest,
        actor: &Principal,
    ) -> Result<IssuedApiKey, IssueKeyError> {
        let principal_kind = principal_kind_for_card(&request.card_ref)?;
        let Some(row) =
            service_account_by_card_ref(conn, principal_kind, &request.card_ref).await?
        else {
            return Err(IssueKeyError::ServiceAccountNotFound(
                request.card_ref.to_string(),
            ));
        };

        let api_key_id = Uuid::new_v4();
        let plaintext = WyrdApiKey::generate(conn.data_tenant_id());
        let prefix = plaintext.prefix.clone();
        let raw = plaintext.secret.clone();
        let key_hash =
            tokio::task::spawn_blocking(move || wyrd_auth_issue::hash_api_key(&raw)).await??;
        let ttl = request
            .expires_in_seconds
            .and_then(|seconds| Duration::try_seconds(i64::from(seconds)))
            .unwrap_or(self.settings.default_ttl);
        let now = Utc::now();
        let expires_at = now + ttl;

        insert_api_key(
            conn,
            api_key_id,
            row.id,
            &prefix,
            &key_hash,
            actor.id.as_uuid(),
            Some(expires_at),
        )
        .await?;

        Ok(IssuedApiKey {
            response: IssueKeyResponse {
                key_id: api_key_id,
                key: SecretBearer::new(plaintext.secret.expose_secret().to_owned()),
                prefix,
                card_ref: request.card_ref,
                created_at: now,
                expires_at,
            },
            service_account_id: row.id,
            api_key_id,
        })
    }

    /// Stage the credential-issuance audit event on the issuing transaction.
    ///
    /// The event names the target card as its resource and `service_accounts`
    /// write as its permission, and records the key id and expiry — never the
    /// key. It commits with the key row, so a key is never returned unaudited.
    ///
    /// # Errors
    /// Returns [`WyrdError::AuditUnavailable`] when the audit append fails.
    pub async fn audit(
        &self,
        conn: &mut TenantConn<'_>,
        issued: &IssuedApiKey,
        actor: &Principal,
        request_id: &str,
    ) -> Result<(), WyrdError> {
        let mut event = auth_event(
            request_id,
            API_KEY_ISSUE_OPERATION,
            actor.id,
            actor.kind.tag(),
            actor.card_ref().cloned(),
            AuditOutcome::Allowed,
            AuditDetail::CredentialIssuance {
                target_principal_id: PrincipalId::new(issued.service_account_id),
                api_key_id: issued.api_key_id,
                expires_at: issued.response.expires_at,
            },
        );
        event.resource = issued.response.card_ref.to_string();
        event.permission = Permission::service_accounts_write().to_string();
        append_auth_audit(conn, &event).await
    }
}

/// Parsed API-key credential.
#[derive(Debug, Clone)]
pub struct WyrdApiKey {
    /// Tenant parsed from the key prefix.
    pub tenant_id: wyrd_spec::DataTenantId,
    /// Lookup prefix stored in Postgres.
    pub prefix: String,
    /// Full secret.
    pub secret: SecretString,
}

impl WyrdApiKey {
    /// Generate a plaintext API key for a tenant.
    #[must_use]
    pub fn generate(tenant_id: wyrd_spec::DataTenantId) -> Self {
        let random = Uuid::new_v4().simple().to_string();
        let tenant = tenant_id.as_uuid().simple().to_string();
        let prefix = format!("wyrd_sk_{tenant}_{}", &random[..8]);
        let secret = SecretString::from(format!("{prefix}_{}", Uuid::new_v4().simple()));
        Self {
            tenant_id,
            prefix,
            secret,
        }
    }

    /// Parse the tenant and lookup prefix from a raw key.
    ///
    /// # Errors
    /// Returns an error when the key does not follow Wyrd's prefix shape.
    pub fn parse(raw: &str) -> Result<Self, WyrdApiKeyParseError> {
        let mut parts = raw.split('_');
        if parts.next() != Some("wyrd") || parts.next() != Some("sk") {
            return Err(WyrdApiKeyParseError);
        }
        let tenant = parts.next().ok_or(WyrdApiKeyParseError)?;
        let visible = parts.next().ok_or(WyrdApiKeyParseError)?;
        if parts.next().is_none() {
            return Err(WyrdApiKeyParseError);
        }
        let tenant_uuid = Uuid::parse_str(tenant).map_err(|_| WyrdApiKeyParseError)?;
        let tenant_id =
            wyrd_spec::DataTenantId::try_from(tenant_uuid).map_err(|_| WyrdApiKeyParseError)?;
        Ok(Self {
            tenant_id,
            prefix: format!("wyrd_sk_{tenant}_{visible}"),
            secret: SecretString::from(raw.to_owned()),
        })
    }
}

/// API key parse error.
#[derive(Debug, thiserror::Error)]
#[error("invalid Wyrd API key")]
pub struct WyrdApiKeyParseError;

/// Return the non-human principal kind for a `CardRef`.
pub fn principal_kind_for_card(card_ref: &CardRef) -> Result<&'static str, IssueKeyError> {
    match card_ref.kind {
        CardKind::Service => Ok("service"),
        CardKind::Agent => Ok("agent"),
        ref other => Err(IssueKeyError::PrincipalKindCardKindMismatch {
            card_kind: other.clone(),
        }),
    }
}

impl From<IssueKeyError> for WyrdError {
    fn from(error: IssueKeyError) -> Self {
        match error {
            IssueKeyError::ServiceAccountNotFound(card_ref) => WyrdError::PrincipalNotFound {
                message: "no active Service or Agent principal is bound to card_ref".to_owned(),
                details: serde_json::json!({ "card_ref": card_ref }),
            },
            IssueKeyError::PrincipalKindCardKindMismatch { card_kind } => {
                WyrdError::PrincipalKindCardKindMismatch {
                    message: "card kind cannot be bound to a non-human principal".to_owned(),
                    details: serde_json::json!({ "card_kind": card_kind.wire_name() }),
                }
            }
            IssueKeyError::Hash(_) | IssueKeyError::Join(_) | IssueKeyError::Database(_) => {
                WyrdError::Internal {
                    message: "failed to issue API key".to_owned(),
                    details: serde_json::json!({}),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WyrdApiKey, principal_kind_for_card};
    use secrecy::ExposeSecret;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    #[test]
    fn generated_api_key_parses_tenant_and_prefix() {
        let tenant = wyrd_spec::DataTenantId::new_v7();
        let key = WyrdApiKey::generate(tenant);
        let parsed = WyrdApiKey::parse(key.secret.expose_secret()).expect("key parses");

        assert_eq!(parsed.tenant_id, tenant);
        assert_eq!(parsed.prefix, key.prefix);
    }

    /// Platform Service keys preserve the reserved `SYSTEM_OWNER` identity.
    #[test]
    fn generated_system_owner_api_key_parses_tenant_and_prefix() {
        let tenant = wyrd_spec::DataTenantId::SYSTEM_OWNER;
        let key = WyrdApiKey::generate(tenant);
        let parsed = WyrdApiKey::parse(key.secret.expose_secret()).expect("system key parses");

        assert_eq!(parsed.tenant_id, tenant);
        assert_eq!(parsed.prefix, key.prefix);
    }

    #[test]
    fn parse_rejects_wrong_prefix() {
        assert!(WyrdApiKey::parse("sk_12345").is_err());
    }

    #[test]
    fn parse_rejects_missing_segments() {
        assert!(WyrdApiKey::parse("wyrd_sk_onlythree").is_err());
    }

    #[test]
    fn parse_rejects_non_uuid_tenant() {
        assert!(WyrdApiKey::parse("wyrd_sk_notauuid_aaaa_bbbb").is_err());
    }

    #[test]
    fn principal_kind_rejects_non_human_card_kind() {
        let error = principal_kind_for_card(&card_ref(CardKind::Model)).expect_err("rejected");

        assert!(error.to_string().contains("non-human principal"));
    }

    #[test]
    fn hash_runs_on_blocking_pool() {
        let source = include_str!("issue_api_key.rs");

        assert!(source.contains("tokio::task::spawn_blocking"));
        assert!(source.contains("wyrd_auth_issue::hash_api_key"));
    }

    fn card_ref(kind: CardKind) -> CardRef {
        CardRef {
            kind,
            name: CardName::new("runtime").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }
}
