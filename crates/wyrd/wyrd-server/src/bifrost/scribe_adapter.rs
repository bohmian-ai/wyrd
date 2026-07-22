//! Server-owned native and OTLP projections into the queued Scribe contract.

use arrow::record_batch::RecordBatch;
use uuid::Uuid;
use vala_bifrost_redux::catalog::TableRef;
use vala_bifrost_redux::contracts::ScribeAppend;
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use wyrd_runtime::Principal;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::prost::Message;
use wyrd_tonic::wyrd::v1::InsertBatchRequest;

/// Compute native HTTP bytes after the HTTP layer has decompressed the body.
#[must_use]
pub fn native_http_wire_bytes(decompressed_ipc: &[u8]) -> usize {
    decompressed_ipc.len()
}

/// Compute native gRPC bytes from the decoded Arrow IPC fields in the stream.
#[must_use]
pub fn native_grpc_wire_bytes(frames: &[InsertBatchRequest]) -> usize {
    frames.iter().map(|frame| frame.arrow_ipc.len()).sum()
}

/// Compute the canonical OTLP source size before projection or fan-out.
#[must_use]
pub fn otlp_wire_bytes<T: Message>(request: &T) -> usize {
    request.encoded_len()
}

/// Project a server-authenticated native batch into one canonical Scribe append.
///
/// The `wire_bytes` argument must come from [`native_http_wire_bytes`] or
/// [`native_grpc_wire_bytes`], never from a client field.
pub fn native_append(
    principal: Principal,
    table: &str,
    rows: RecordBatch,
    request_id: RequestId,
    batch_id: Uuid,
    wire_bytes: usize,
) -> Result<ScribeAppend, String> {
    append(principal, table, rows, request_id, batch_id, wire_bytes)
}

/// Project one OTLP fan-out destination into the same canonical Scribe append.
///
/// Callers pass the original decoded protobuf size for every destination so a
/// multi-table projection cannot understate pod-global admission pressure.
pub fn otlp_append(
    principal: Principal,
    table: &str,
    rows: RecordBatch,
    request_id: RequestId,
    batch_id: Uuid,
    source_wire_bytes: usize,
) -> Result<ScribeAppend, String> {
    append(
        principal,
        table,
        rows,
        request_id,
        batch_id,
        source_wire_bytes,
    )
}

fn append(
    principal: Principal,
    table: &str,
    rows: RecordBatch,
    request_id: RequestId,
    batch_id: Uuid,
    measured_wire_bytes: usize,
) -> Result<ScribeAppend, String> {
    let table = TableRef::parse_fqn(table)
        .ok_or_else(|| format!("invalid registered Bifrost table: {table}"))?;
    let schema_fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
    Ok(ScribeAppend {
        principal,
        table,
        rows,
        schema_fingerprint,
        request_id,
        batch_id,
        measured_wire_bytes,
    })
}

/// Resolve a Redux namespace without accepting arbitrary user-controlled paths.
#[must_use]
pub fn namespace_from_wire(value: &str) -> Option<BifrostNamespace> {
    BifrostNamespace::from_wire(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{ArrayRef, Int64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;
    use wyrd_runtime::{PrincipalKind, permission::PermissionSet};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

    fn rows() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "value",
                DataType::Int64,
                false,
            )])),
            vec![Arc::new(Int64Array::from(vec![1])) as ArrayRef],
        )
        .expect("rows")
    }

    fn principal() -> Principal {
        Principal {
            id: PrincipalId::new(Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: DataTenantId::SYSTEM_OWNER,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        }
    }

    #[test]
    fn native_and_otlp_converge() {
        let request_id = RequestId::now_v7();
        let batch_id = Uuid::now_v7();
        let native = native_append(
            principal(),
            "vala.bifrost.events",
            rows(),
            request_id.clone(),
            batch_id,
            12,
        )
        .expect("native append");
        let otlp = otlp_append(
            principal(),
            "vala.bifrost.events",
            rows(),
            request_id,
            batch_id,
            12,
        )
        .expect("otlp append");
        assert_eq!(native.table, otlp.table);
        assert_eq!(native.rows.schema(), otlp.rows.schema());
        assert_eq!(native.measured_wire_bytes, otlp.measured_wire_bytes);
    }

    #[test]
    fn native_wire_size_is_server_measured() {
        let frame = InsertBatchRequest {
            table: "vala.bifrost.events".to_owned(),
            wyrd_batch_id: vec![0; 16],
            arrow_ipc: vec![1; 37],
        };
        assert_eq!(native_grpc_wire_bytes(&[frame]), 37);
        assert_eq!(native_http_wire_bytes(&[1; 41]), 41);
    }

    #[test]
    fn otlp_fanout_uses_full_source_size() {
        let source = ExportTraceServiceRequest::default();
        let bytes = otlp_wire_bytes(&source);
        let first = otlp_append(
            principal(),
            "vala.traces.spans",
            rows(),
            RequestId::now_v7(),
            Uuid::now_v7(),
            bytes,
        )
        .expect("first fanout");
        let second = otlp_append(
            principal(),
            "vala.traces.spans",
            rows(),
            RequestId::now_v7(),
            Uuid::now_v7(),
            bytes,
        )
        .expect("second fanout");
        assert_eq!(first.measured_wire_bytes, bytes);
        assert_eq!(second.measured_wire_bytes, bytes);
    }
}
