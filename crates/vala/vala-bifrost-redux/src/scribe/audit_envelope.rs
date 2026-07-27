//! Audit envelope codec — JSON over `wyrd_spec::vala::api::AuditEvent`.
//!
//! Scribe uses the canonical `AuditEvent` type directly (CONTRACTS §11 forbids
//! parallel audit surfaces). The kind=1 WAL record payload is a JSON-serialized
//! `AuditEvent`; the Scribe-local metadata (`wal_lsn_min`, `wal_lsn_max`, `batch_id`,
//! `rows_accepted`, `seal_key`) is derived from WAL record headers and travels on
//! `ScribeAppendMeta` — not inside the `AuditEvent`.
//!
//! JSON was chosen over bincode for:
//! - A stable canonical encoding for durable WAL state
//! - Self-describing format (debuggable WAL files)
//! - `AuditEvent` already has JSON-compatible derives
//! - Size difference negligible compared to Arrow IPC data records

use wyrd_spec::vala::api::AuditEvent;

use crate::contracts::ScribeError;

/// Encode an `AuditEvent` to JSON bytes for a kind=1 WAL record.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if JSON serialization fails (never expected
/// for a well-formed `AuditEvent`).
pub fn encode_audit_event(event: &AuditEvent) -> Result<Vec<u8>, ScribeError> {
    serde_json::to_vec(event).map_err(|e| ScribeError::Internal {
        detail: format!("failed to encode AuditEvent: {e}"),
    })
}

/// Decode an `AuditEvent` from JSON bytes (kind=1 WAL record payload).
///
/// # Errors
/// Returns [`ScribeError::Internal`] if:
/// - The payload is truncated or corrupt
/// - The payload does not match the `AuditEvent` schema
pub fn decode_audit_event(bytes: &[u8]) -> Result<AuditEvent, ScribeError> {
    serde_json::from_slice(bytes).map_err(|e| ScribeError::Internal {
        detail: format!("failed to decode AuditEvent: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{AuditDecision, AuditResult, AuthMethod};

    #[test]
    fn audit_event_codec_roundtrips_canonical_shape() {
        let event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: Some("test-trace-id".to_string()),
            operation: "bifrost.register_table".to_string(),
            resource: "vala.bifrost.test_table".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:register".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "register table".to_string(),
            detail: None,
        };

        let encoded = encode_audit_event(&event).expect("encode");
        let decoded = decode_audit_event(&encoded).expect("decode");

        assert_eq!(decoded.request_id, event.request_id);
        assert_eq!(decoded.trace_id, event.trace_id);
        assert_eq!(decoded.operation, event.operation);
        assert_eq!(decoded.resource, event.resource);
        assert_eq!(decoded.card_ref, event.card_ref);
        assert_eq!(decoded.principal_id, event.principal_id);
        assert_eq!(decoded.principal_kind, event.principal_kind);
        assert_eq!(decoded.auth_method, event.auth_method);
        assert_eq!(decoded.permission, event.permission);
        assert_eq!(decoded.decision, event.decision);
        assert_eq!(decoded.result, event.result);
        assert_eq!(decoded.payload_summary, event.payload_summary);
        assert_eq!(decoded.detail, event.detail);
    }

    #[test]
    fn audit_event_codec_rejects_truncated_payload() {
        let event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_string(),
            resource: "test".to_string(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "test".to_string(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "test".to_string(),
            detail: None,
        };

        let mut encoded = encode_audit_event(&event).expect("encode");
        encoded.truncate(encoded.len() - 1);

        let result = decode_audit_event(&encoded);
        assert!(result.is_err());
    }
}
