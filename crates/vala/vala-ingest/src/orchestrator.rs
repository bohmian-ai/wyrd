//! Transport-neutral frame validation and projection helpers.

use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use vala_bifrost::{BifrostNamespace, WyrdCatalog};
use vala_bifrost_redux::catalog::TableRef as ReduxTableRef;
use vala_bifrost_redux::catalog::TenantTableBinding;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace as ReduxNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use wyrd_runtime::{Permission, PermissionCheck, Principal, RbacCheck};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_spec::vala::{CARD_REF, CARD_UID, PRINCIPAL_ID};
use wyrd_tonic::wyrd::v1::InsertBatchRequest;

use crate::auth::AuthContext;
use crate::error::IngestError;
use crate::limits::IngestLimits;

/// The anti-forgery gate: every per-row `card_ref` in the decoded batches must
/// be a member of the principal's card-ref scope (exact `CardRef` equality). A
/// missing column, a null cell, or an unparseable value rejects the whole
/// stream — there is nothing valid to attribute.
///
/// # Errors
/// Returns [`IngestError::CardScopeDenied`] for any out-of-scope, null, absent,
/// or unparseable `card_ref`.
pub fn validate_card_scope(
    batches: &[RecordBatch],
    principal: &Principal,
) -> Result<(), IngestError> {
    let Some(scope) = principal.card_ref_scope() else {
        return Err(IngestError::CardScopeDenied {
            card_ref: "<principal-without-card-scope>".to_owned(),
        });
    };
    for batch in batches {
        let column =
            batch
                .column_by_name(CARD_REF)
                .ok_or_else(|| IngestError::CardScopeDenied {
                    card_ref: "<absent>".to_owned(),
                })?;
        let cards = column
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| IngestError::CardScopeDenied {
                card_ref: "<non-utf8-column>".to_owned(),
            })?;
        for i in 0..cards.len() {
            if cards.is_null(i) {
                return Err(IngestError::CardScopeDenied {
                    card_ref: "<null>".to_owned(),
                });
            }
            let raw = cards.value(i);
            let card = CardRef::from_str(raw).map_err(|_| IngestError::CardScopeDenied {
                card_ref: raw.to_owned(),
            })?;
            if !scope.authorizes(&card) {
                return Err(IngestError::CardScopeDenied {
                    card_ref: raw.to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Stamp server-resolved `card_uid` and `principal_id` correlation columns onto
/// every batch, removing the wire-only `card_ref` column (M-11 / Task 12).
///
/// Rules (fail-closed for authenticated writes):
/// - If `card_ref` column is present: resolve each value to `card_uid` via the
///   principal's bound card reference uid. If uid is unavailable (not yet
///   resolved by the registry), rejects with `CardUnresolved`.
/// - If `card_ref` column is absent or all-null: `card_uid` is stamped NULL
///   (card-less observation, permitted for authenticated writes).
/// - `principal_id` is always stamped from `principal.id` (never NULL for
///   authenticated writes).
///
/// # Errors
/// Returns [`IngestError::CardUnresolved`] when a `card_ref` is present but
/// its uid cannot be resolved via the principal's bound card.
pub fn stamp_correlation_columns(
    batches: Vec<RecordBatch>,
    principal: &Principal,
) -> Result<Vec<RecordBatch>, IngestError> {
    let principal_id_str = principal.id.to_string();
    let bound_card_uid: Option<String> = principal
        .card_ref()
        .and_then(|cr| cr.uid.as_ref())
        .map(|uid| uid.to_string());

    let mut out = Vec::with_capacity(batches.len());
    for batch in batches {
        let nrows = batch.num_rows();
        let card_ref_col_idx = batch.schema().index_of(CARD_REF).ok();

        // Resolve card_uid per-row when card_ref column is present.
        let card_uid_values: Vec<Option<String>> = if let Some(idx) = card_ref_col_idx {
            let col = batch.column(idx);
            let arr = col.as_any().downcast_ref::<StringArray>().ok_or_else(|| {
                IngestError::CardScopeDenied {
                    card_ref: "<non-utf8-card_ref>".to_owned(),
                }
            })?;
            let mut uids = Vec::with_capacity(nrows);
            for i in 0..nrows {
                if arr.is_null(i) {
                    uids.push(None);
                } else {
                    let raw = arr.value(i);
                    let card =
                        CardRef::from_str(raw).map_err(|_| IngestError::CardScopeDenied {
                            card_ref: raw.to_owned(),
                        })?;
                    // Try resolving uid from principal's bound card (same identity).
                    let uid = if let Some(bound) = principal.card_ref() {
                        if bound.same_identity(&card) {
                            bound_card_uid
                                .clone()
                                .ok_or_else(|| IngestError::CardUnresolved {
                                    card_ref: raw.to_owned(),
                                })?
                        } else {
                            // Card in scope but not the principal's bound card — no uid available.
                            return Err(IngestError::CardUnresolved {
                                card_ref: raw.to_owned(),
                            });
                        }
                    } else {
                        // User principal: card_ref present but no bound card → cannot resolve.
                        return Err(IngestError::CardUnresolved {
                            card_ref: raw.to_owned(),
                        });
                    };
                    uids.push(Some(uid));
                }
            }
            uids
        } else {
            vec![None; nrows]
        };

        // Build new schema: drop card_ref, add card_uid + principal_id.
        let mut new_fields: Vec<Field> = batch
            .schema()
            .fields()
            .iter()
            .filter(|f| f.name() != CARD_REF)
            .map(|f| f.as_ref().clone())
            .collect();
        new_fields.push(Field::new(CARD_UID, DataType::Utf8, true));
        new_fields.push(Field::new(PRINCIPAL_ID, DataType::Utf8, false));

        let mut new_cols: Vec<Arc<dyn Array>> = batch
            .schema()
            .fields()
            .iter()
            .enumerate()
            .filter(|(_, f)| f.name() != CARD_REF)
            .map(|(i, _)| Arc::clone(batch.column(i)))
            .collect();

        let card_uid_arr = Arc::new(StringArray::from(card_uid_values)) as Arc<dyn Array>;
        let principal_id_arr =
            Arc::new(StringArray::from(vec![principal_id_str.as_str(); nrows])) as Arc<dyn Array>;
        new_cols.push(card_uid_arr);
        new_cols.push(principal_id_arr);

        let new_schema = Arc::new(Schema::new(new_fields));
        out.push(
            RecordBatch::try_new(new_schema, new_cols)
                .map_err(|e| IngestError::Internal(e.to_string()))?,
        );
    }
    Ok(out)
}

/// Resolve a `namespace.name` fqn into a [`BifrostNamespace`] + table name.
///
/// # Errors
/// Returns [`IngestError::StreamProtocolViolation`] when the fqn does not name a
/// known Bifrost namespace or is not a single `namespace.name` pair.
pub fn resolve_fqn(fqn: &str) -> Result<(BifrostNamespace, String), IngestError> {
    BifrostNamespace::split_fqn(fqn).ok_or_else(|| {
        IngestError::StreamProtocolViolation(format!("unrecognized table fqn: {fqn}"))
    })
}

/// Process one already-framed native payload and admit it to Scribe.
pub async fn run_ingest_frame_to_scribe<F>(
    catalog: &WyrdCatalog,
    scribe: &F,
    limits: &IngestLimits,
    auth: &AuthContext,
    frame: InsertBatchRequest,
    stream_rows_before: u64,
) -> Result<u64, IngestError>
where
    F: Scribe + ?Sized,
{
    if frame.arrow_ipc.len() > limits.max_frame_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: frame.arrow_ipc.len() as u64,
            limit: limits.max_frame_bytes as u64,
        });
    }
    let batch_id: [u8; 16] = frame.wyrd_batch_id.as_slice().try_into().map_err(|_| {
        IngestError::StreamProtocolViolation("wyrd_batch_id must be exactly 16 bytes".to_owned())
    })?;
    let batch_id_uuid = uuid::Uuid::from_bytes(batch_id);
    if batch_id_uuid.get_version() != Some(uuid::Version::SortRand) {
        return Err(IngestError::StreamProtocolViolation(
            "wyrd_batch_id must be UUIDv7".to_owned(),
        ));
    }
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;
    let (namespace, name) = resolve_fqn(&frame.table)?;
    if namespace == BifrostNamespace::System {
        return Err(IngestError::SystemTableWriteDenied { table: frame.table });
    }
    let registered_fingerprint = catalog
        .table_schema_fingerprint(namespace, &name, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;
    let table_ref = ReduxTableRef::new(
        ReduxNamespace::from_wire(namespace.as_str()).ok_or_else(|| {
            IngestError::Internal(format!("Redux namespace is not registered: {namespace:?}"))
        })?,
        name,
    );
    let binding = TenantTableBinding::resolve((auth.tenant, table_ref.clone()))
        .map_err(|error| IngestError::Internal(error.to_string()))?;
    let audit_event = AuditEvent {
        request_id: auth.request_id.clone(),
        trace_id: None,
        operation: "bifrost.ingest_frame".to_owned(),
        resource: table_ref.fqn(),
        card_ref: auth.principal.card_ref().cloned(),
        principal_id: auth.principal.id,
        principal_kind: auth.principal.kind.tag(),
        auth_method: AuthMethod::Jwt,
        permission: "bifrost:record:write".to_owned(),
        decision: AuditDecision::Allow,
        result: AuditResult::Success,
        payload_summary: "one bounded native frame".to_owned(),
        detail: None,
    };
    let admission = scribe
        .ingest_frame(ScribeIngressFrame {
            principal: auth.principal.clone(),
            binding,
            expected_schema_fingerprint: SchemaFingerprint(registered_fingerprint),
            request_id: auth.request_id.clone(),
            batch_id: batch_id_uuid,
            frame_sequence: frame.frame_sequence,
            audit_event,
            measured_wire_bytes: frame.arrow_ipc.len(),
            stream_rows_before,
            stream_rows_limit: limits.max_stream_rows,
            payload: IngressPayload::ArrowIpc(Bytes::from(frame.arrow_ipc)),
        })
        .await
        .map_err(IngestError::from_scribe)?;
    Ok(admission.rows_accepted)
}

pub fn source_schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    let fields: Vec<Field> = schema
        .fields()
        .iter()
        .filter(|field| {
            !matches!(
                field.name().as_str(),
                CARD_REF | CARD_UID | PRINCIPAL_ID | "run_id" | "data_tenant_id"
            ) && !field.name().starts_with("wyrd_")
        })
        .map(|field| field.as_ref().clone())
        .collect();
    SchemaFingerprint::from_arrow_schema(&Schema::new(fields))
}

/// The card-scope anti-forgery gate (no DB required): `validate_card_scope`
/// authorizes every per-row `card_ref` against the principal's scope.
#[cfg(test)]
mod card_scope {
    use std::str::FromStr;
    use std::sync::Arc;

    use arrow::array::{RecordBatch, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use crate::error::IngestError;
    use crate::orchestrator::validate_card_scope;

    const IN_SCOPE: &str = "prod/Service/billing@1.0.0";
    const OUT_OF_SCOPE: &str = "prod/Service/shipping@1.0.0";

    fn card(canonical: &str) -> CardRef {
        CardRef::from_str(canonical).expect("canonical card ref parses")
    }

    fn principal_with_scope(cards: &[&str]) -> Principal {
        let card_ref = card(IN_SCOPE);
        let card_ref_scope =
            CardRefScope::from_root_and_members(&card_ref, cards.iter().map(|c| card(c)));
        Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref,
                card_ref_scope,
            },
            DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_record_write()]),
        )
    }

    fn batch_with_card_refs(values: Vec<Option<&str>>) -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "card_ref",
            DataType::Utf8,
            true,
        )]));
        let column = Arc::new(StringArray::from(values)) as Arc<dyn arrow::array::Array>;
        RecordBatch::try_new(schema, vec![column]).expect("batch builds")
    }

    fn batch_without_card_ref() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![Field::new("val", DataType::Int64, false)]));
        let column = Arc::new(arrow::array::Int64Array::from(vec![1_i64, 2]))
            as Arc<dyn arrow::array::Array>;
        RecordBatch::try_new(schema, vec![column]).expect("batch builds")
    }

    #[test]
    fn card_scope_accepts_all_in_scope_rows() {
        let principal = principal_with_scope(&[IN_SCOPE]);
        let batch = batch_with_card_refs(vec![Some(IN_SCOPE), Some(IN_SCOPE)]);

        validate_card_scope(&[batch], &principal).expect("all in-scope cards pass");
    }

    #[test]
    fn card_scope_rejects_out_of_scope_card() {
        let principal = principal_with_scope(&[IN_SCOPE]);
        let batch = batch_with_card_refs(vec![Some(IN_SCOPE), Some(OUT_OF_SCOPE)]);

        let err = validate_card_scope(&[batch], &principal).expect_err("out-of-scope card rejects");
        assert!(matches!(err, IngestError::CardScopeDenied { .. }));
        assert_eq!(err.wyrd_code(), "WYRD_VALA_403_BIFROST_CARD_SCOPE");
    }

    #[test]
    fn card_scope_rejects_null_card() {
        let principal = principal_with_scope(&[IN_SCOPE]);
        let batch = batch_with_card_refs(vec![Some(IN_SCOPE), None]);

        let err = validate_card_scope(&[batch], &principal).expect_err("null card rejects");
        assert!(matches!(err, IngestError::CardScopeDenied { .. }));
    }

    #[test]
    fn card_scope_rejects_absent_column() {
        let principal = principal_with_scope(&[IN_SCOPE]);

        let err = validate_card_scope(&[batch_without_card_ref()], &principal)
            .expect_err("absent rejects");
        assert!(matches!(err, IngestError::CardScopeDenied { .. }));
    }

    #[test]
    fn card_scope_rejects_unparseable_card() {
        let principal = principal_with_scope(&[IN_SCOPE]);
        let batch = batch_with_card_refs(vec![Some("not-a-card-ref")]);

        let err = validate_card_scope(&[batch], &principal).expect_err("garbage rejects");
        assert!(matches!(err, IngestError::CardScopeDenied { .. }));
    }

    #[test]
    fn card_scope_empty_user_principal_cannot_write() {
        // A User principal has an empty card scope, so no supplied card is in scope.
        let user = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            DataTenantId::new_v7(),
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_record_write()]),
        );
        let batch = batch_with_card_refs(vec![Some(IN_SCOPE)]);

        let err = validate_card_scope(&[batch], &user).expect_err("empty scope rejects all");
        assert!(matches!(err, IngestError::CardScopeDenied { .. }));
    }
}

/// Orchestrator gate: the ingest path maps `BifrostError::IngestBusy` (local
/// buffer backpressure, Q5) to `IngestError::WriterBusy`
/// (`WYRD_VALA_429_INGEST_BUSY`).  No partial_success; no silent drop.
///
/// This test does NOT drive a live database; it verifies the error taxonomy
/// without a commit path (pure unit test).
#[cfg(test)]
mod tests {
    use crate::error::IngestError;
    use vala_bifrost::BifrostError;

    #[test]
    fn from_engine_maps_ingest_busy_to_writer_busy() {
        let engine_err = BifrostError::IngestBusy("test.events".to_string());
        let ingest_err = IngestError::from_engine(engine_err);
        assert!(
            matches!(ingest_err, IngestError::WriterBusy),
            "expected WriterBusy, got {ingest_err:?}"
        );
        assert_eq!(ingest_err.wyrd_code(), "WYRD_VALA_429_INGEST_BUSY");
    }

    #[test]
    fn writer_busy_grpc_code_is_resource_exhausted() {
        use wyrd_tonic::tonic::Code;
        let err = IngestError::WriterBusy;
        assert_eq!(err.grpc_code(), Code::ResourceExhausted);
    }

    #[test]
    fn writer_busy_ack_deferred_contract() {
        // The ack-after-commit contract (Q1) is enforced by the coordinator
        // actor: the reply channel is only written AFTER run_commit returns.
        // We verify the IngestError taxonomy is correct and that WriterBusy
        // exposes the stable code (no partial_success path in the taxonomy).
        let err = IngestError::WriterBusy;
        assert_eq!(err.wyrd_code(), "WYRD_VALA_429_INGEST_BUSY");
        // The error maps to ResourceExhausted (429) — not to partial_success (not 200).
        let status = err.into_status();
        assert_eq!(status.code(), wyrd_tonic::tonic::Code::ResourceExhausted);
    }
}
