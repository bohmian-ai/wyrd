//! Authentication audit events on the canonical audit outbox.
//!
//! Token grants, API-key issuance, refresh-family revocation, and card-scope
//! mints are each recorded as one [`AuditEvent`] appended to
//! `vala.audit_staging` through [`append_audit`], the single audit write path.
//! The server's `AuditPublisher` is the only thing that moves those rows into
//! retained history; this module owns no table and no other sink.

use sqlx::PgPool;
use vala_sql::queries::audit_staging::append_audit;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};
use wyrd_spec::vala::audit_detail::AuditErrorCode;
use wyrd_sql::TenantConn;

/// Operation for every issued access token: authorization code, API key, JWT
/// bearer, delegation, and refresh rotation.
pub const TOKEN_EXCHANGE_OPERATION: &str = "auth.token.exchange";
/// Operation for a minted API key; matches the issuing route's decision row.
pub const API_KEY_ISSUE_OPERATION: &str = "auth.api_key.issue";
/// Operation for a refresh-token family revoked after reuse was detected.
pub const REFRESH_FAMILY_REVOKE_OPERATION: &str = "auth.refresh.revoke_family";
/// Operation for a card-ref scope minted into (or refused from) a token.
pub const CARD_SCOPE_MINT_OPERATION: &str = "auth.card_scope.mint";

/// Parse the caller's request id into the audit correlation id.
///
/// Server routes always pass a UUID v7 request id. Internal callers such as
/// bootstrap pass a fixed label; those rows get a fresh v7 id instead of
/// failing the grant, because the audit row itself is what must exist.
#[must_use]
pub fn audit_request_id(request_id: &str) -> RequestId {
    RequestId::parse(request_id).unwrap_or_else(|_| RequestId::now_v7())
}

/// Build one auth audit event attributed to the acting principal.
///
/// The resource is the acting card when there is one, otherwise the principal
/// id. The permission is the operation name, because these grants authenticate
/// rather than evaluate a dynamic permission; callers that did evaluate one
/// overwrite `permission` (and `resource`) on the returned event.
#[must_use]
pub fn auth_event(
    request_id: &str,
    operation: &str,
    principal_id: PrincipalId,
    principal_kind: PrincipalKindTag,
    card_ref: Option<CardRef>,
    outcome: AuditOutcome,
    detail: AuditDetail,
) -> AuditEvent {
    let resource = card_ref
        .as_ref()
        .map_or_else(|| format!("principal:{principal_id}"), ToString::to_string);
    AuditEvent::new(
        audit_request_id(request_id),
        None,
        operation.to_owned(),
        resource,
        card_ref,
        principal_id,
        principal_kind,
        operation.to_owned(),
        outcome,
    )
    .with_detail(detail)
}

/// Map a stored `principal_kind` column value onto its audit tag.
///
/// Unknown values are recorded as `Service`, the non-human default; the grant
/// paths reject unknown kinds before any event is built.
#[must_use]
pub fn principal_kind_tag(value: &str) -> PrincipalKindTag {
    match value {
        "user" => PrincipalKindTag::User,
        "agent" => PrincipalKindTag::Agent,
        _ => PrincipalKindTag::Service,
    }
}

/// Append one auth audit event on the grant's own transaction.
///
/// The row commits exactly when the caller commits `conn`, so a token or key is
/// never returned without its audit row.
///
/// # Errors
/// Returns [`WyrdError::AuditUnavailable`] when the append fails.
pub async fn append_auth_audit(
    conn: &mut TenantConn<'_>,
    event: &AuditEvent,
) -> Result<(), WyrdError> {
    append_audit(conn, event)
        .await
        .map(|_| ())
        .map_err(|error| audit_unavailable(&error))
}

/// Append one auth failure event in its own transaction, logging instead of
/// failing.
///
/// Refused grants already return an error to the caller; a missing refusal row
/// must not replace that error, so acquire, append, and commit failures are
/// logged at `error` and swallowed.
pub async fn record_auth_audit_best_effort(
    pool: &PgPool,
    tenant: DataTenantId,
    event: &AuditEvent,
) {
    let result = async {
        let mut conn = TenantConn::acquire(pool, tenant).await?;
        append_audit(&mut conn, event).await?;
        conn.commit().await
    }
    .await;
    if let Err(error) = result {
        tracing::error!(
            error = %error,
            operation = %event.operation,
            "auth failure audit could not be staged"
        );
    }
}

/// Map a refused grant's error onto the closed audit failure code.
///
/// Credential problems collapse to `InvalidToken`, missing principals or cards
/// to `NotFound`, and shape violations to `ValidationFailed`. Everything else —
/// backend and internal failures — is recorded as a refusal (`PermissionDenied`).
#[must_use]
pub fn auth_failure_code(error: &WyrdError) -> AuditErrorCode {
    match error {
        WyrdError::InvalidToken { .. }
        | WyrdError::TokenExpired { .. }
        | WyrdError::BadTokenFormat { .. }
        | WyrdError::InvalidNonce { .. }
        | WyrdError::InvalidState { .. }
        | WyrdError::CredentialRevoked { .. } => AuditErrorCode::InvalidToken,
        WyrdError::PrincipalNotFound { .. } | WyrdError::RegistryCardNotFound { .. } => {
            AuditErrorCode::NotFound
        }
        WyrdError::PrincipalKindCardKindMismatch { .. } | WyrdError::CardScopeTooLarge { .. } => {
            AuditErrorCode::ValidationFailed
        }
        WyrdError::AuditUnavailable { .. } => AuditErrorCode::AuditUnavailable,
        _ => AuditErrorCode::PermissionDenied,
    }
}

/// Build the fail-closed public error for a failed audit append.
fn audit_unavailable(error: &dyn std::fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "auth audit append failed; refusing grant");
    WyrdError::AuditUnavailable {
        message: "audit append failed".to_owned(),
        details: serde_json::json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::{AuditErrorCode, audit_request_id, auth_failure_code};
    use wyrd_spec::error::WyrdError;

    /// Credential, lookup, and backend failures land on distinct closed codes.
    #[test]
    fn failure_codes_group_refusals() {
        let invalid_state = WyrdError::InvalidState {
            message: "state".to_owned(),
            details: serde_json::json!({}),
        };
        let internal = WyrdError::Internal {
            message: "boom".to_owned(),
            details: serde_json::json!({}),
        };
        assert_eq!(
            auth_failure_code(&invalid_state),
            AuditErrorCode::InvalidToken
        );
        assert_eq!(
            auth_failure_code(&internal),
            AuditErrorCode::PermissionDenied
        );
    }

    /// A v7 request id is kept; a label is replaced rather than rejected.
    #[test]
    fn request_id_keeps_v7_and_replaces_labels() {
        let id = "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff";
        assert_eq!(audit_request_id(id).as_str(), id);
        assert_ne!(audit_request_id("not-a-uuid").as_str(), "not-a-uuid");
    }
}
