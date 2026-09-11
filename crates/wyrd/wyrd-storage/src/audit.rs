//! Transactional storage lifecycle audit helpers.

use sqlx::types::Uuid;
use vala_sql::queries::audit_staging::append_audit;
use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
use wyrd_spec::error::WyrdError;
use wyrd_spec::storage::StorageBackendKind;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_spec::vala::audit_detail::{
    AuditDetail, AuditErrorCode, StorageAuditOperation, StorageBackend, StoragePath,
};
use wyrd_sql::TenantConn;

use crate::service::{StorageCaller, StoragePrincipalKind};

/// Typed storage lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UploadAuditOperation {
    SessionCreated,
    BackendInitialized,
    BackendFailed,
    Complete,
    Abort,
    Download,
    Reclaimed,
}

impl UploadAuditOperation {
    fn as_wire_str(self) -> &'static str {
        match self {
            Self::SessionCreated => "session_created",
            Self::BackendInitialized => "backend_initialized",
            Self::BackendFailed => "backend_failed",
            Self::Complete => "complete",
            Self::Abort => "abort",
            Self::Download => "download",
            Self::Reclaimed => "reclaimed",
        }
    }
}

/// Append one transition-level storage audit row to the Vala transactional outbox.
// justification: the audit row must preserve the complete storage transition context as one atomic append; bundling these fields would obscure the wire-level audit contract.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write(
    conn: &mut TenantConn<'_>,
    caller: &StorageCaller,
    operation: UploadAuditOperation,
    upload_id: Option<Uuid>,
    storage_path: &str,
    backend: StorageBackendKind,
    status_code: i32,
    error_code: Option<&str>,
) -> Result<(), WyrdError> {
    let detail = AuditDetail::Storage {
        operation: match operation {
            UploadAuditOperation::SessionCreated => StorageAuditOperation::SessionCreated,
            UploadAuditOperation::BackendInitialized => StorageAuditOperation::BackendInitialized,
            UploadAuditOperation::BackendFailed => StorageAuditOperation::BackendFailed,
            UploadAuditOperation::Complete => StorageAuditOperation::Complete,
            UploadAuditOperation::Abort => StorageAuditOperation::Abort,
            UploadAuditOperation::Download => StorageAuditOperation::Download,
            UploadAuditOperation::Reclaimed => StorageAuditOperation::Reclaimed,
        },
        upload_id,
        storage_path: StoragePath::new(storage_path).map_err(|error| {
            crate::service::internal_error(
                "storage audit path is invalid",
                serde_json::json!({ "reason": error.to_string() }),
            )
        })?,
        backend: match backend {
            StorageBackendKind::Local => StorageBackend::Local,
            StorageBackendKind::S3 => StorageBackend::S3,
            StorageBackendKind::Gcs => StorageBackend::Gcs,
            StorageBackendKind::Azure => StorageBackend::Azure,
        },
        status_code: u16::try_from(status_code).unwrap_or(u16::MAX),
        error_code: error_code.map(parse_error_code),
    };
    let event = AuditEvent::new(
        caller.request_id.clone(),
        None,
        format!("storage.{}", operation.as_wire_str()),
        storage_path.to_owned(),
        None,
        PrincipalId::new(caller.subject.principal_id),
        principal_kind(caller.subject.kind),
        if caller.subject.principal_id == wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL.as_uuid() {
            AuthMethod::Internal
        } else {
            AuthMethod::Jwt
        },
        if matches!(operation, UploadAuditOperation::Download) {
            "storage:read".to_owned()
        } else {
            "storage:write".to_owned()
        },
        AuditDecision::Allow,
        audit_result(status_code, error_code),
        format!("storage transition {}", operation.as_wire_str()),
    )
    .with_detail(detail);
    append_audit(conn, &event)
        .await
        .map(|_| ())
        .map_err(|error| crate::service::map_sql_error(&error))
}

fn principal_kind(kind: StoragePrincipalKind) -> PrincipalKindTag {
    match kind {
        StoragePrincipalKind::User => PrincipalKindTag::User,
        StoragePrincipalKind::Service => PrincipalKindTag::Service,
        StoragePrincipalKind::Agent => PrincipalKindTag::Agent,
    }
}

fn audit_result(status_code: i32, error_code: Option<&str>) -> AuditResult {
    if error_code.is_some() || status_code / 100 != 2 {
        AuditResult::Failure
    } else {
        AuditResult::Success
    }
}

fn parse_error_code(value: &str) -> AuditErrorCode {
    match value {
        "WYRD_PERMISSION_DENIED" | "WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT" => {
            AuditErrorCode::PermissionDenied
        }
        "WYRD_AUDIT_UNAVAILABLE" => AuditErrorCode::AuditUnavailable,
        "WYRD_INVALID_TOKEN" => AuditErrorCode::InvalidToken,
        "WYRD_NOT_FOUND"
        | "WYRD_STORAGE_404_OBJECT_NOT_FOUND"
        | "WYRD_STORAGE_404_UPLOAD_NOT_FOUND" => AuditErrorCode::NotFound,
        "WYRD_VALIDATION_FAILED"
        | "WYRD_STORAGE_400_TENANT_PATH_MISMATCH"
        | "WYRD_STORAGE_400_ARTIFACT_TOO_LARGE"
        | "WYRD_STORAGE_400_SHA256_INVALID"
        | "WYRD_STORAGE_400_SIZE_INVALID"
        | "WYRD_STORAGE_400_INVALID_UPLOAD_ID"
        | "WYRD_STORAGE_400_INVALID_URI"
        | "WYRD_STORAGE_400_TENANT_PREFIX_INVALID"
        | "tenant_path"
        | "capability_mismatch"
        | "config_parse" => AuditErrorCode::ValidationFailed,
        _ => AuditErrorCode::StorageBackendFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_kind_is_bare_wire_tag() {
        assert_eq!(
            principal_kind(StoragePrincipalKind::User),
            PrincipalKindTag::User
        );
        assert_eq!(
            principal_kind(StoragePrincipalKind::Service),
            PrincipalKindTag::Service
        );
        assert_eq!(
            principal_kind(StoragePrincipalKind::Agent),
            PrincipalKindTag::Agent
        );
    }

    #[test]
    fn reclamation_failure_classes_have_stable_typed_codes() {
        assert_eq!(
            parse_error_code("backend"),
            AuditErrorCode::StorageBackendFailure
        );
        assert_eq!(
            parse_error_code("sql"),
            AuditErrorCode::StorageBackendFailure
        );
        assert_eq!(
            parse_error_code("unclassified"),
            AuditErrorCode::StorageBackendFailure
        );
    }

    #[test]
    fn non_success_status_is_a_failure_even_without_an_error_code() {
        assert_eq!(audit_result(500, None), AuditResult::Failure);
        assert_eq!(audit_result(200, Some("backend")), AuditResult::Failure);
        assert_eq!(audit_result(200, None), AuditResult::Success);
    }
}
