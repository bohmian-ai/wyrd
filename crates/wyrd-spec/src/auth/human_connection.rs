//! Wire contract for a tenant's human OIDC login connection.
//!
//! A tenant has at most one `Active` connection that human login trusts and at
//! most one `Candidate` staged to replace it. The lifecycle is headless:
//! `PUT /v1/identity/oidc/candidate` stages or rotates, `POST .../test` proves
//! the exact candidate revision against the provider, and `POST .../activate`
//! swaps it in. The bearer credential, never a request field, selects the
//! tenant.
//!
//! Secrets travel inbound only. [`HumanConnectionView`] carries no secret or
//! sealing metadata, and [`HumanClientAuth`] deliberately has no
//! `PrivateKeyJwt` arm: an unimplemented method is refused with
//! `WYRD_AUTH_400_UNSUPPORTED_CLIENT_AUTH` by [`ConnectionInput::from_json`]
//! and never appears in a generated schema.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::DataTenantId;
use crate::auth::{ClaimMappingPayload, IssuerUrl, SecretBearer};
use crate::error::WyrdError;

/// Wire spelling of the client-authentication method Wyrd does not implement.
const PRIVATE_KEY_JWT: &str = "PrivateKeyJwt";

/// Lifecycle state of one tenant human connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum HumanConnectionState {
    /// Staged configuration; never trusted by login.
    Candidate,
    /// The one configuration human login trusts.
    Active,
    /// Retired, deactivated, or removed; never trusted by login.
    Inactive,
}

impl HumanConnectionState {
    /// Stable storage and wire spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "Candidate",
            Self::Active => "Active",
            Self::Inactive => "Inactive",
        }
    }

    /// Parse the stored spelling.
    ///
    /// # Errors
    /// Returns [`WyrdError::Internal`] for an unknown stored value.
    pub fn parse(value: &str) -> Result<Self, WyrdError> {
        match value {
            "Candidate" => Ok(Self::Candidate),
            "Active" => Ok(Self::Active),
            "Inactive" => Ok(Self::Inactive),
            other => Err(WyrdError::Internal {
                message: "stored human connection state is unknown".to_owned(),
                details: serde_json::json!({ "state": other }),
            }),
        }
    }
}

/// How Wyrd authenticates to the provider token endpoint for human login.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum HumanClientAuth {
    /// HTTP Basic with the client secret; requires `client_secret`.
    SecretBasic,
    /// Client secret in the form body; requires `client_secret`.
    SecretPost,
    /// Public PKCE client; `client_secret` must be absent.
    Public,
}

impl HumanClientAuth {
    /// Stable storage and wire spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SecretBasic => "SecretBasic",
            Self::SecretPost => "SecretPost",
            Self::Public => "Public",
        }
    }

    /// Parse the stored spelling.
    ///
    /// # Errors
    /// Returns [`WyrdError::Internal`] for an unknown stored value.
    pub fn parse(value: &str) -> Result<Self, WyrdError> {
        match value {
            "SecretBasic" => Ok(Self::SecretBasic),
            "SecretPost" => Ok(Self::SecretPost),
            "Public" => Ok(Self::Public),
            other => Err(WyrdError::Internal {
                message: "stored human connection client auth is unknown".to_owned(),
                details: serde_json::json!({ "client_auth": other }),
            }),
        }
    }

    /// Whether this method sends a client secret.
    #[must_use]
    pub fn requires_secret(self) -> bool {
        !matches!(self, Self::Public)
    }
}

/// Redacted projection of one tenant human connection.
///
/// Never carries a client secret, its ciphertext, or sealing-key identifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct HumanConnectionView {
    /// Stable connection id; kept as a tombstone after removal.
    pub id: Uuid,
    /// Owning tenant.
    pub tenant_id: DataTenantId,
    /// Tenant-monotonic configuration revision.
    pub revision: u64,
    /// Lifecycle state.
    pub state: HumanConnectionState,
    /// Provider issuer URL, pinned as the ID-token `iss`.
    pub issuer: String,
    /// Wyrd's client id at the provider, also the ID-token audience.
    pub client_id: String,
    /// Client-authentication method; the secret itself is never returned.
    pub client_auth: HumanClientAuth,
    /// Verified claim mapping (`subject`, optional `email` and `groups`).
    pub claim_mapping: ClaimMappingPayload,
    /// Provider group to tenant role names.
    pub group_role_map: HashMap<String, Vec<String>>,
    /// JWKS key-cache lifetime in seconds.
    pub jwks_ttl_secs: u64,
    /// Revision the last successful test proved, when one is current.
    pub tested_revision: Option<u64>,
    /// Instant after which activation requires a fresh test.
    pub tested_until: Option<DateTime<Utc>>,
    /// Creation instant.
    pub created_at: DateTime<Utc>,
    /// Last mutation instant.
    pub updated_at: DateTime<Utc>,
    /// Exact redirect URI to register at the provider, derived from the
    /// deployment's configured public origin. `None` only when the deployment
    /// has no public origin configured, in which case staging and testing are
    /// refused.
    pub callback_url: Option<String>,
}

/// `GET /v1/identity/oidc/connections` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct HumanConnectionsResponse {
    /// The connection human login trusts, if any.
    pub active: Option<HumanConnectionView>,
    /// The staged replacement, if any.
    pub candidate: Option<HumanConnectionView>,
    /// Redirect URI to register at the provider, available before any
    /// connection exists. `None` when no public origin is configured.
    pub callback_url: Option<String>,
}

/// `PUT /v1/identity/oidc/candidate` body: stage or rotate the candidate.
///
/// Also the rotation path for a secret or group-role map on an unchanged
/// issuer: stage the same issuer with the new values, test, and activate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConnectionInput {
    /// Provider issuer URL; endpoints and JWKS come from its discovery document.
    pub issuer: IssuerUrl,
    /// Wyrd's client id at the provider; the ID-token audience is derived
    /// from it.
    pub client_id: String,
    /// Client-authentication method.
    pub client_auth: HumanClientAuth,
    /// Client secret for `SecretBasic`/`SecretPost`; must be absent for
    /// `Public`. Sealed before storage and never returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<SecretBearer>,
    /// Claim mapping.
    pub claim_mapping: ClaimMappingPayload,
    /// Provider group to tenant role names.
    #[serde(default)]
    pub group_role_map: HashMap<String, Vec<String>>,
    /// Revision of the candidate being replaced. Required when a candidate
    /// exists; must be omitted when none does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
}

impl ConnectionInput {
    /// Decode a raw request body, refusing unsupported client authentication
    /// with its own stable code before generic validation.
    ///
    /// # Errors
    /// Returns [`WyrdError::UnsupportedClientAuth`] when `client_auth` names
    /// `PrivateKeyJwt`, [`WyrdError::Validation`] when the body does not match
    /// the contract, and the result of [`Self::validate`] otherwise.
    pub fn from_json(body: serde_json::Value) -> Result<Self, WyrdError> {
        if body.get("client_auth").and_then(serde_json::Value::as_str) == Some(PRIVATE_KEY_JWT) {
            return Err(unsupported_client_auth());
        }
        let input: Self = serde_json::from_value(body).map_err(|error| WyrdError::Validation {
            message: format!("connection input is invalid: {error}"),
            details: serde_json::json!({}),
        })?;
        input.validate()?;
        Ok(input)
    }

    /// Check the cross-field rules the type cannot express.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `client_id` or the subject claim
    /// path is empty, a secret method omits its secret, or `Public` carries
    /// one.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.client_id.trim().is_empty() {
            return Err(validation("client_id must not be empty", "client_id"));
        }
        if self.claim_mapping.subject.trim().is_empty() {
            return Err(validation(
                "claim_mapping.subject must not be empty",
                "claim_mapping",
            ));
        }
        let has_secret = self
            .client_secret
            .as_ref()
            .is_some_and(|secret| !secret.expose().is_empty());
        match (self.client_auth.requires_secret(), has_secret) {
            (true, false) => Err(validation(
                "client_secret is required for SecretBasic and SecretPost",
                "client_secret",
            )),
            (false, true) => Err(validation(
                "client_secret must be omitted for a Public client",
                "client_secret",
            )),
            _ => Ok(()),
        }
    }
}

/// `POST /v1/identity/oidc/candidate/test` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConnectionTestRequest {
    /// Candidate revision to test; a newer revision is a conflict.
    pub expected_revision: u64,
}

/// `POST /v1/identity/oidc/candidate/test` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConnectionTestResponse {
    /// The candidate with its fresh test stamp.
    pub candidate: HumanConnectionView,
}

/// `POST /v1/identity/oidc/candidate/activate` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConnectionActivate {
    /// Candidate revision to activate; must match the tested revision.
    pub expected_revision: u64,
    /// API key of an existing headless principal of this tenant holding
    /// `identity_connections:write`, proving a non-SSO way back in exists
    /// before the old provider is retired. Verified, never stored or echoed.
    pub recovery_api_key: SecretBearer,
}

/// Refuse a Human issuer on the workload trusted-issuer surfaces.
///
/// # Errors
/// Returns [`WyrdError::HumanConnectionRequired`] when `principal_kind` is
/// `Human`.
pub fn refuse_human_trusted_issuer(
    principal_kind: crate::auth::IssuerTokenPolicy,
) -> Result<(), WyrdError> {
    if principal_kind == crate::auth::IssuerTokenPolicy::Human {
        return Err(WyrdError::HumanConnectionRequired {
            message: "trusted issuers are workload-only; configure human login through the \
                      tenant OIDC connection API (/v1/identity/oidc)"
                .to_owned(),
            details: serde_json::json!({ "principal_kind": "human" }),
        });
    }
    Ok(())
}

/// The `UNSUPPORTED_CLIENT_AUTH` refusal.
fn unsupported_client_auth() -> WyrdError {
    WyrdError::UnsupportedClientAuth {
        message: "PrivateKeyJwt client authentication is not supported for tenant connections"
            .to_owned(),
        details: serde_json::json!({ "field": "client_auth" }),
    }
}

/// A field-scoped validation refusal.
fn validation(message: &str, field: &str) -> WyrdError {
    WyrdError::Validation {
        message: message.to_owned(),
        details: serde_json::json!({ "field": field }),
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionInput, HumanClientAuth, refuse_human_trusted_issuer};
    use crate::auth::IssuerTokenPolicy;
    use crate::error::WyrdError;

    /// A minimal valid public-client body.
    fn body() -> serde_json::Value {
        serde_json::json!({
            "issuer": "https://idp.example.com",
            "client_id": "wyrd",
            "client_auth": "Public",
            "claim_mapping": { "subject": "sub" },
        })
    }

    /// `PrivateKeyJwt` gets its own stable code, not a generic decode error.
    #[test]
    fn private_key_jwt_is_refused_with_its_own_code() {
        let mut raw = body();
        raw["client_auth"] = serde_json::json!("PrivateKeyJwt");
        assert!(matches!(
            ConnectionInput::from_json(raw),
            Err(WyrdError::UnsupportedClientAuth { .. })
        ));
    }

    /// Secret presence must agree with the client-auth method.
    #[test]
    fn secret_presence_follows_the_method() {
        let input = ConnectionInput::from_json(body()).expect("public client decodes");
        assert_eq!(input.client_auth, HumanClientAuth::Public);

        let mut missing = body();
        missing["client_auth"] = serde_json::json!("SecretPost");
        assert!(matches!(
            ConnectionInput::from_json(missing),
            Err(WyrdError::Validation { .. })
        ));

        let mut extra = body();
        extra["client_secret"] = serde_json::json!("s3cret");
        assert!(matches!(
            ConnectionInput::from_json(extra),
            Err(WyrdError::Validation { .. })
        ));
    }

    /// The generated schema never offers `PrivateKeyJwt` and never leaks
    /// secret-bearing field shapes into the view.
    #[test]
    fn schema_does_not_offer_private_key_jwt() {
        let schema = schemars::schema_for!(ConnectionInput);
        let text = serde_json::to_string(&schema).expect("schema serializes");
        assert!(!text.contains("PrivateKeyJwt"));
    }

    /// Workload trusted issuers pass; Human ones are redirected to this API.
    #[test]
    fn human_trusted_issuer_is_refused() {
        assert!(refuse_human_trusted_issuer(IssuerTokenPolicy::Workload).is_ok());
        assert!(matches!(
            refuse_human_trusted_issuer(IssuerTokenPolicy::Human),
            Err(WyrdError::HumanConnectionRequired { .. })
        ));
    }
}
