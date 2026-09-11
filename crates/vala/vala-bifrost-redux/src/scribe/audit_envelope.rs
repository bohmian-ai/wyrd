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

/// Non-retaining JSON byte counter used before audit material allocation.
#[derive(Default)]
struct JsonByteCounter {
    /// Checked serialized bytes observed from serde.
    bytes: usize,
}

impl std::io::Write for JsonByteCounter {
    /// Counts one serialized JSON fragment without retaining it.
    ///
    /// # Errors
    ///
    /// Returns an IO error when the exact byte count overflows.
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len())
            .ok_or_else(|| std::io::Error::other("audit JSON length overflow"))?;
        Ok(buffer.len())
    }

    /// Counting retains no buffered IO state.
    ///
    /// # Errors
    ///
    /// This operation cannot fail because no IO is performed.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Encode an `AuditEvent` to JSON bytes for a kind=1 WAL record.
///
/// # Errors
/// Returns [`ScribeError::Internal`] if JSON serialization fails (never expected
/// for a well-formed `AuditEvent`).
pub fn encode_audit_event(event: &AuditEvent) -> Result<Vec<u8>, ScribeError> {
    encode_audit_event_bounded(
        event,
        crate::gate::limits::BIFROST_WAL_WORKSPACE_LIMIT_BYTES,
    )
}

/// Encodes an audit event into one exact-capacity buffer under the admitted workspace.
///
/// # Errors
///
/// Returns a stable material refusal when exact JSON bytes exceed `workspace_bytes`,
/// or an internal error when the deterministic serde count/encode passes fail.
pub(crate) fn encode_audit_event_bounded(
    event: &AuditEvent,
    workspace_bytes: usize,
) -> Result<Vec<u8>, ScribeError> {
    let mut counter = JsonByteCounter::default();
    serde_json::to_writer(&mut counter, event).map_err(|error| ScribeError::Internal {
        detail: format!("failed to count AuditEvent JSON: {error}"),
    })?;
    if counter.bytes > workspace_bytes {
        return Err(ScribeError::DecodedPayloadTooLarge {
            bytes: counter.bytes,
            limit: workspace_bytes,
        });
    }
    let mut encoded = Vec::with_capacity(counter.bytes);
    serde_json::to_writer(&mut encoded, event).map_err(|error| ScribeError::Internal {
        detail: format!("failed to encode AuditEvent: {error}"),
    })?;
    if encoded.len() != counter.bytes || encoded.capacity() != counter.bytes {
        return Err(ScribeError::Internal {
            detail: "audit JSON exact-capacity invariant diverged".to_owned(),
        });
    }
    Ok(encoded)
}

/// Replaces already-fenced ingest audits with one distinct publication event.
///
/// The first ingest event supplies request, tenant-principal, and resource
/// correlation. File-list insertion is one durable generation transition, so
/// it emits exactly one audit regardless of the generation's slice count.
#[must_use]
pub(crate) fn publication_audit_events(events: &[AuditEvent]) -> Vec<AuditEvent> {
    let Some(source) = events.first() else {
        return Vec::new();
    };
    let mut event = source.clone();
    "bifrost.scribe.visibility.publish".clone_into(&mut event.operation);
    "published durable Scribe generation".clone_into(&mut event.payload_summary);
    event.detail = None;
    vec![event]
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
        assert_eq!(encoded.len(), encoded.capacity());
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

    /// Proves the admitted workspace refuses before allocating encoded JSON.
    #[test]
    fn audit_event_codec_refuses_workspace_cap_plus_one() {
        let event = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: None,
            operation: "test".to_owned(),
            resource: "test".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "test".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "bounded".to_owned(),
            detail: None,
        };
        let exact = encode_audit_event(&event).expect("exact audit bytes").len();
        let error = encode_audit_event_bounded(&event, exact - 1)
            .expect_err("one byte below exact JSON must refuse");
        assert!(matches!(
            error,
            ScribeError::DecodedPayloadTooLarge { bytes, limit }
                if bytes == exact && limit == exact - 1
        ));
    }

    /// Proves one file-list commit emits one distinct publication transition.
    #[test]
    fn publication_audit_is_distinct_and_deduplicated_per_generation() {
        let source = AuditEvent {
            request_id: RequestId::now_v7(),
            trace_id: Some("trace".to_owned()),
            operation: "bifrost.append".to_owned(),
            resource: "vala.bifrost.events".to_owned(),
            card_ref: None,
            principal_id: PrincipalId::new(uuid::Uuid::new_v4()),
            principal_kind: PrincipalKindTag::User,
            auth_method: AuthMethod::Jwt,
            permission: "bifrost:append".to_owned(),
            decision: AuditDecision::Allow,
            result: AuditResult::Success,
            payload_summary: "1 rows".to_owned(),
            detail: None,
        };
        let second = source.clone();
        let publication = publication_audit_events(&[source.clone(), second]);
        assert_eq!(publication.len(), 1);
        let event = &publication[0];
        assert_eq!(event.operation, "bifrost.scribe.visibility.publish");
        assert_eq!(event.request_id, source.request_id);
        assert_eq!(event.principal_id, source.principal_id);
        assert_eq!(event.resource, source.resource);
        assert_eq!(event.trace_id, source.trace_id);
        assert_eq!(event.detail, None);
        assert_ne!(source.operation, event.operation);
        assert_ne!(source.payload_summary, event.payload_summary);
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
