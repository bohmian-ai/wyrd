//! Authentication audit events on the process audit stage.
//!
//! Token grants, API-key issuance, refresh-family revocation, and card-scope
//! mints are each recorded as one [`AuditEvent`] staged on the process
//! [`wyrd_runtime::audit::AuditStage`], which the server backs with its one
//! Scribe outbox into retained audit history. A grant never waits for, or
//! fails on, that write; this module owns no table and no other sink.

use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditDetail, AuditEvent, AuditOutcome};
use wyrd_spec::vala::audit_detail::AuditErrorCode;

/// Operation for every issued access token: authorization code, API key, JWT
/// bearer, delegation, and refresh rotation.
pub const TOKEN_EXCHANGE_OPERATION: &str = "auth.token.exchange";
/// Operation for a minted API key; matches the issuing route's decision row.
pub const API_KEY_ISSUE_OPERATION: &str = "auth.api_key.issue";
/// Operation for a refresh-token family revoked after reuse was detected.
pub const REFRESH_FAMILY_REVOKE_OPERATION: &str = "auth.refresh.revoke_family";
/// Operation for a card-ref scope minted into (or refused from) a token.
pub const CARD_SCOPE_MINT_OPERATION: &str = "auth.card_scope.mint";
/// Operation for a human login a provider callback completed: its User is
/// signed in and its authorization code or device approval recorded.
pub const LOGIN_OPERATION: &str = "auth.login";
/// Operation for a human login whose provider-asserted groups changed the
/// User's durable role assignments.
pub const USER_ROLES_SYNC_OPERATION: &str = "auth.user.roles.sync";

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
/// [`principal_event`] with `detail` attached; see it for the resource and
/// permission rules.
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
    principal_event(
        request_id,
        operation,
        principal_id,
        principal_kind,
        card_ref,
        outcome,
    )
    .with_detail(detail)
}

/// Build one detail-less auth audit event attributed to the acting principal.
///
/// The resource is the acting card when there is one, otherwise the principal
/// id. The permission is the operation name, because these grants authenticate
/// rather than evaluate a dynamic permission; callers that did evaluate one
/// overwrite `permission` (and `resource`) on the returned event.
#[must_use]
pub fn principal_event(
    request_id: &str,
    operation: &str,
    principal_id: PrincipalId,
    principal_kind: PrincipalKindTag,
    card_ref: Option<CardRef>,
    outcome: AuditOutcome,
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
}

/// Map a stored `principal_kind` column value onto its audit tag.
///
/// One owner for both planes: `platform.principals` stores `global_admin` or
/// `user`, `wyrd.auth_service_accounts` stores `tenant_admin`, `service`,
/// `agent`, or `system`, and `wyrd.auth_users` stores `user`. Unknown values are recorded as
/// `Service`, the non-human default; the grant paths reject unknown kinds
/// before any event is built.
#[must_use]
pub fn principal_kind_tag(value: &str) -> PrincipalKindTag {
    match value {
        "global_admin" => PrincipalKindTag::GlobalAdmin,
        "tenant_admin" => PrincipalKindTag::TenantAdmin,
        "user" => PrincipalKindTag::User,
        "agent" => PrincipalKindTag::Agent,
        "system" => PrincipalKindTag::System,
        _ => PrincipalKindTag::Service,
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
        | WyrdError::DeviceAuthorization { .. }
        | WyrdError::CredentialRevoked { .. } => AuditErrorCode::InvalidToken,
        WyrdError::PrincipalNotFound { .. } | WyrdError::RegistryCardNotFound { .. } => {
            AuditErrorCode::NotFound
        }
        WyrdError::PrincipalKindCardKindMismatch { .. } | WyrdError::CardScopeTooLarge { .. } => {
            AuditErrorCode::ValidationFailed
        }
        _ => AuditErrorCode::PermissionDenied,
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

#[cfg(test)]
pub(crate) mod test_audit {
    //! A recording audit stage for auth tests.

    use std::sync::{Arc, Mutex};

    use wyrd_runtime::audit::AuditStage;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::vala::api::AuditEvent;

    /// Records every staged decision in staging order, in place of the
    /// server's Scribe outbox, so a test asserts exactly what a grant staged.
    #[derive(Debug, Default)]
    pub(crate) struct RecordedAudit {
        /// Every staged decision with its tenant.
        staged: Mutex<Vec<(DataTenantId, AuditEvent)>>,
    }

    impl RecordedAudit {
        /// A fresh, empty recorder.
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        /// Every decision staged with `operation`, with its tenant, in
        /// staging order.
        ///
        /// # Panics
        ///
        /// Panics when the recorder lock is poisoned.
        pub(crate) fn operation(&self, operation: &str) -> Vec<(DataTenantId, AuditEvent)> {
            self.staged
                .lock()
                .expect("audit recorder lock")
                .iter()
                .filter(|(_, event)| event.operation == operation)
                .cloned()
                .collect()
        }
    }

    impl AuditStage for RecordedAudit {
        /// Records `event` for `tenant`.
        ///
        /// # Panics
        ///
        /// Panics when the recorder lock is poisoned.
        fn stage(&self, tenant: DataTenantId, event: AuditEvent) {
            self.staged
                .lock()
                .expect("audit recorder lock")
                .push((tenant, event));
        }
    }
}
