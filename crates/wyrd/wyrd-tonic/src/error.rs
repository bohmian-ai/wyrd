//! gRPC error mapping from `WyrdError` to `tonic::Status`.
//!
//! Mirrors the HTTP mapper at `crates/wyrd/wyrd-server/src/error.rs`. The HTTP
//! envelope is the canonical wire form: gRPC carries the same bytes in a binary
//! metadata entry so agent clients see the same `code` / `remediation` /
//! `details` regardless of transport.

use tonic::metadata::{BinaryMetadataValue, MetadataValue};
use tonic::{Code, Status};
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;

/// Binary metadata key carrying the problem+json bytes. Must end with `-bin`
/// per gRPC convention.
pub const WYRD_ERROR_HEADER: &str = "wyrd-error-bin";

/// ASCII metadata key carrying the per-request id, mirroring the HTTP header.
pub const WYRD_REQUEST_ID_HEADER: &str = "wyrd-request-id";

/// Render a `WyrdError` as a `tonic::Status`.
///
/// The status code comes from `WyrdError::status()` mapped to `tonic::Code`;
/// the message is the error's `Display`; the problem+json bytes are attached as
/// `wyrd-error-bin` binary metadata. When `request_id` is provided, both the
/// `instance` field in the problem+json body and a `wyrd-request-id` ASCII
/// metadata entry are populated.
///
/// Provided as a free function, not a `From<WyrdError> for tonic::Status`
/// impl, because both types are foreign to this crate and the orphan rule
/// blocks the impl.
#[must_use]
pub fn wyrd_error_to_status(error: WyrdError, request_id: Option<&RequestId>) -> Status {
    let code = http_status_to_grpc_code(error.status());
    let message = error.to_string();
    let mut body = error.as_problem_json();
    if let (serde_json::Value::Object(ref mut map), Some(id)) = (&mut body, request_id) {
        map.insert(
            "instance".to_owned(),
            serde_json::Value::String(format!("urn:wyrd:request:{id}")),
        );
    }
    let mut status = Status::new(code, message);
    if let Ok(bytes) = serde_json::to_vec(&body) {
        status
            .metadata_mut()
            .insert_bin(WYRD_ERROR_HEADER, BinaryMetadataValue::from_bytes(&bytes));
    }
    if let Some(id) = request_id {
        if let Ok(value) = MetadataValue::try_from(id.to_string()) {
            status.metadata_mut().insert(WYRD_REQUEST_ID_HEADER, value);
        }
    }
    status
}

fn http_status_to_grpc_code(status: u16) -> Code {
    match status {
        400 => Code::InvalidArgument,
        401 => Code::Unauthenticated,
        403 => Code::PermissionDenied,
        404 => Code::NotFound,
        409 => Code::AlreadyExists,
        412 => Code::FailedPrecondition,
        413 => Code::ResourceExhausted,
        429 => Code::ResourceExhausted,
        499 => Code::Cancelled,
        500 => Code::Internal,
        501 => Code::Unimplemented,
        503 => Code::Unavailable,
        504 => Code::DeadlineExceeded,
        507 => Code::ResourceExhausted,
        _ => Code::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::request_id::RequestId;

    use super::{wyrd_error_to_status, WYRD_ERROR_HEADER, WYRD_REQUEST_ID_HEADER};

    fn make_id() -> RequestId {
        RequestId::parse("01966f13-b6c7-7c04-b9e7-8c1f8f1e5a3c").expect("valid uuid v7")
    }

    #[test]
    fn permission_denied_maps_to_correct_code() {
        let err = WyrdError::PermissionDeniedRbac {
            message: "denied".to_owned(),
            details: serde_json::json!({}),
        };
        let status = wyrd_error_to_status(err, None);
        assert_eq!(status.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn status_carries_problem_json_binary_metadata() {
        let err = WyrdError::NotFound {
            message: "not found".to_owned(),
            details: serde_json::json!({}),
        };
        let status = wyrd_error_to_status(err, None);
        assert!(
            status.metadata().get_bin(WYRD_ERROR_HEADER).is_some(),
            "wyrd-error-bin metadata must be present"
        );
    }

    #[test]
    fn request_id_populates_metadata_and_instance() {
        let id = make_id();
        let err = WyrdError::NotFound {
            message: "not found".to_owned(),
            details: serde_json::json!({}),
        };
        let status = wyrd_error_to_status(err, Some(&id));
        assert!(
            status.metadata().get(WYRD_REQUEST_ID_HEADER).is_some(),
            "wyrd-request-id metadata must be present when request_id is Some"
        );
        let bytes = status
            .metadata()
            .get_bin(WYRD_ERROR_HEADER)
            .expect("wyrd-error-bin present")
            .to_bytes()
            .expect("valid bytes");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        let instance = json["instance"].as_str().expect("instance field present");
        assert!(
            instance.contains(id.as_str()),
            "instance field must contain the request id"
        );
    }

    #[test]
    fn timeout_maps_to_deadline_exceeded() {
        let err = WyrdError::RequestTimeout {
            message: "timed out".to_owned(),
            details: serde_json::json!({}),
        };
        let status = wyrd_error_to_status(err, None);
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
    }

    #[test]
    fn service_unavailable_maps_to_unavailable() {
        let err = WyrdError::ServiceUnavailable {
            message: "load shedding".to_owned(),
            details: serde_json::json!({}),
        };
        let status = wyrd_error_to_status(err, None);
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    #[test]
    fn wal_disk_full_maps_to_resource_exhausted() {
        let err = WyrdError::from(wyrd_spec::vala::error::BifrostError::WalDiskFull);
        let status = wyrd_error_to_status(err, None);
        assert_eq!(status.code(), tonic::Code::ResourceExhausted);
    }
}
