//! Stream collection, the three pre-flush authorization gates, and the 2PC
//! commit that drives `vala-bifrost`'s writer.
//!
//! The heavy lifting is split so the bounds/decode and the card-scope gate are
//! testable without a database: [`collect_frames`] and [`validate_card_scope`]
//! take in-memory inputs and never touch the catalog.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use tokio::time::timeout;
use vala_bifrost::writer::BifrostWriteContext;
use vala_bifrost::{BifrostNamespace, TableScope, WyrdCatalog};
use vala_bifrost_redux::catalog::TableRef as ReduxTableRef;
use vala_bifrost_redux::catalog::TenantTableBinding;
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeAppend, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace as ReduxNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use wyrd_runtime::{Permission, PermissionCheck, Principal, RbacCheck};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{AuditDecision, AuditEvent, AuditResult, AuthMethod};
use wyrd_spec::vala::{CARD_REF, CARD_UID, PRINCIPAL_ID};
use wyrd_tonic::tonic::Status;
use wyrd_tonic::wyrd::v1::InsertBatchRequest;

use crate::auth::AuthContext;
use crate::decode::decode_ipc;
use crate::error::IngestError;
use crate::limits::IngestLimits;

/// Source of inbound `InsertBatch` frames. Implemented for `tonic::Streaming`
/// on the server path and by an in-memory source in tests — so the streaming
/// bounds logic is exercised without a live transport.
pub trait FrameSource {
    /// Yield the next frame, `Ok(None)` on half-close, or a transport error.
    fn next_frame(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Option<InsertBatchRequest>, Status>> + Send;
}

impl FrameSource for wyrd_tonic::tonic::Streaming<InsertBatchRequest> {
    async fn next_frame(&mut self) -> Result<Option<InsertBatchRequest>, Status> {
        self.message().await
    }
}

/// The fully-read, decoded stream: the target table, the durable idempotency
/// key, and every decoded batch (`[user + run_id + card_ref]`, client-supplied).
#[derive(Debug)]
pub struct CollectedStream {
    /// Fully-qualified table name (`namespace.name`), from the first frame.
    pub table: String,
    /// The stream's `wyrd_batch_id` (16-byte UUIDv7), from the first frame.
    pub batch_id: [u8; 16],
    /// Decoded record batches, in stream order.
    pub batches: Vec<RecordBatch>,
    /// Total decoded row count.
    pub rows: u64,
    /// Server-measured Arrow IPC bytes received after transport decompression.
    pub wire_bytes: usize,
}

/// Read the whole stream, enforcing every aggregate bound as frames arrive.
///
/// Any breach, disconnect, or deadline aborts here — before any gate or commit —
/// so no precommit row and no Parquet is ever written for a rejected stream.
///
/// # Errors
/// Returns the mapped [`IngestError`] for a bound breach, an idle/total
/// deadline, a protocol violation (mismatched table/batch id, bad batch id
/// length, too many frames), or an Arrow decode failure.
pub async fn collect_frames<S: FrameSource>(
    mut source: S,
    limits: &IngestLimits,
) -> Result<CollectedStream, IngestError> {
    let started = Instant::now();
    let mut table: Option<String> = None;
    let mut batch_id: Option<[u8; 16]> = None;
    let mut batches: Vec<RecordBatch> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut total_rows: u64 = 0;
    let mut frame_count: u64 = 0;

    loop {
        let next = timeout(limits.idle_deadline, source.next_frame())
            .await
            .map_err(|_| IngestError::StreamIdle)?;
        let frame = match next {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(status) => {
                return Err(IngestError::StreamProtocolViolation(format!(
                    "stream aborted by client: {}",
                    status.message()
                )));
            }
        };

        if started.elapsed() > limits.total_deadline {
            return Err(IngestError::StreamIdle);
        }

        frame_count += 1;
        if frame_count > limits.max_stream_frames {
            return Err(IngestError::StreamProtocolViolation(format!(
                "stream exceeded {} frames",
                limits.max_stream_frames
            )));
        }

        total_bytes += frame.arrow_ipc.len() as u64;
        if total_bytes > limits.max_stream_bytes {
            return Err(IngestError::BatchTooLarge {
                bytes: total_bytes,
                limit: limits.max_stream_bytes,
            });
        }

        reconcile_header(&frame, &mut table, &mut batch_id)?;

        for decoded in decode_ipc(&frame.arrow_ipc)? {
            total_rows += decoded.num_rows() as u64;
            if total_rows > limits.max_stream_rows {
                return Err(IngestError::TooManyRows {
                    rows: total_rows,
                    limit: limits.max_stream_rows,
                });
            }
            batches.push(decoded);
        }
    }

    let table = table.ok_or_else(|| {
        IngestError::StreamProtocolViolation("stream carried no table".to_owned())
    })?;
    let batch_id = batch_id.ok_or_else(|| {
        IngestError::StreamProtocolViolation("stream carried no wyrd_batch_id".to_owned())
    })?;

    Ok(CollectedStream {
        table,
        batch_id,
        batches,
        rows: total_rows,
        wire_bytes: usize::try_from(total_bytes).map_err(|_| {
            IngestError::Internal("decoded wire byte count exceeds usize".to_owned())
        })?,
    })
}

/// Establish `table`/`batch_id` from the first frame and require every later
/// frame to repeat them identically (an empty field on a later frame is treated
/// as "unchanged").
fn reconcile_header(
    frame: &InsertBatchRequest,
    table: &mut Option<String>,
    batch_id: &mut Option<[u8; 16]>,
) -> Result<(), IngestError> {
    match table {
        None => {
            if frame.table.is_empty() {
                return Err(IngestError::StreamProtocolViolation(
                    "first frame is missing table".to_owned(),
                ));
            }
            *table = Some(frame.table.clone());
        }
        Some(established) if !frame.table.is_empty() && &frame.table != established => {
            return Err(IngestError::StreamProtocolViolation(format!(
                "table changed mid-stream: {} != {established}",
                frame.table
            )));
        }
        Some(_) => {}
    }

    if !frame.wyrd_batch_id.is_empty() {
        let id: [u8; 16] = frame.wyrd_batch_id.as_slice().try_into().map_err(|_| {
            IngestError::StreamProtocolViolation(
                "wyrd_batch_id must be 16 bytes (UUIDv7)".to_owned(),
            )
        })?;
        match batch_id {
            None => *batch_id = Some(id),
            Some(established) if &id != established => {
                return Err(IngestError::StreamProtocolViolation(
                    "wyrd_batch_id changed mid-stream".to_owned(),
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

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

/// Drive one ingest stream end-to-end: RBAC gate → collect → system-table gate
/// → card-scope gate → buffered write → one 2PC `flush(ctx)`.
///
/// # Errors
/// Returns the first gate/limit/engine failure as an [`IngestError`]; a rejected
/// stream writes no precommit row and no Parquet.
pub async fn run_ingest<S: FrameSource>(
    catalog: &WyrdCatalog,
    limits: &IngestLimits,
    auth: &AuthContext,
    source: S,
) -> Result<u64, IngestError> {
    // Gate (a): record-write capability. Coarse/operation-scoped; tenant
    // isolation is the data-partition boundary, enforced separately.
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;

    let collected = collect_frames(source, limits).await?;

    // Gate (b): destination-table boundary — no external write to reserved
    // system tables (`vala.system.*`).
    let (namespace, name) = resolve_fqn(&collected.table)?;
    if namespace == BifrostNamespace::System {
        return Err(IngestError::SystemTableWriteDenied {
            table: collected.table.clone(),
        });
    }

    // Gate (c): attribution boundary — every per-row card must be in scope.
    validate_card_scope(&collected.batches, &auth.principal)?;

    // Resolve card_ref → card_uid and stamp principal_id server-side (M-11).
    let stamped = stamp_correlation_columns(collected.batches, &auth.principal)?;

    let writer = catalog
        .writer(namespace, &name, TableScope::TenantOwned, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;

    let ctx = BifrostWriteContext {
        batch_id: collected.batch_id,
        origin: "ingest".to_owned(),
        actor: auth.principal.id.to_string(),
        request_id: auth.request_id.clone(),
        // Writer-identity card for C5 audit attribution (constant per commit).
        card_ref: auth.principal.card_ref().cloned(),
    };
    // All stamped batches are one commit unit under `collected.batch_id`;
    // `commit_one` closes the writer so the coordinator drains and commits.
    let _snapshot = writer
        .commit_one(auth.tenant, stamped, ctx)
        .await
        .map_err(IngestError::from_engine)?;

    Ok(collected.rows)
}

/// Drive one native stream into the queued Redux Scribe seam.
///
/// The complete stamped request becomes one canonical `RecordBatch` and one
/// `ScribeAppend`. The measured bytes come from server-owned frame bodies; no
/// client-provided size field participates in admission.
pub async fn run_ingest_to_scribe<S, F>(
    catalog: &WyrdCatalog,
    scribe: &F,
    limits: &IngestLimits,
    auth: &AuthContext,
    source: S,
) -> Result<u64, IngestError>
where
    S: FrameSource,
    F: Scribe + ?Sized,
{
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;

    let collected = collect_frames(source, limits).await?;
    let (namespace, name) = resolve_fqn(&collected.table)?;
    if namespace == BifrostNamespace::System {
        return Err(IngestError::SystemTableWriteDenied {
            table: collected.table,
        });
    }
    let registered_fingerprint = catalog
        .table_schema_fingerprint(namespace, &name, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;
    let source_fingerprint = collected
        .batches
        .first()
        .map(|batch| source_schema_fingerprint(batch.schema().as_ref()))
        .ok_or_else(|| IngestError::StreamProtocolViolation("stream carried no rows".to_owned()))?;
    if source_fingerprint.0 != registered_fingerprint {
        return Err(IngestError::SchemaMismatch {
            table: collected.table,
        });
    }
    validate_card_scope(&collected.batches, &auth.principal)?;
    let stamped = stamp_correlation_columns(collected.batches, &auth.principal)?;
    let Some(first) = stamped.first() else {
        return Ok(0);
    };
    let rows = arrow::compute::concat_batches(&first.schema(), &stamped)
        .map_err(|error| IngestError::Internal(error.to_string()))?;
    let schema_fingerprint = SchemaFingerprint::from_arrow_schema(rows.schema().as_ref());
    let redux_namespace = ReduxNamespace::from_wire(namespace.as_str()).ok_or_else(|| {
        IngestError::Internal(format!("Redux namespace is not registered: {namespace:?}"))
    })?;
    scribe
        .append(ScribeAppend {
            principal: auth.principal.clone(),
            table: ReduxTableRef::new(redux_namespace, name),
            rows,
            schema_fingerprint,
            request_id: auth.request_id.clone(),
            batch_id: uuid::Uuid::from_bytes(collected.batch_id),
            measured_wire_bytes: collected.wire_bytes,
        })
        .await
        .map_err(IngestError::from_scribe)?;
    Ok(collected.rows)
}

/// Process one already-framed native payload and admit it to Scribe.
pub async fn run_ingest_frame_to_scribe<F>(
    catalog: &WyrdCatalog,
    scribe: &F,
    limits: &IngestLimits,
    auth: &AuthContext,
    frame: InsertBatchRequest,
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
    let (namespace, name) = resolve_fqn(&frame.table)?;
    if namespace == BifrostNamespace::System {
        return Err(IngestError::SystemTableWriteDenied { table: frame.table });
    }
    RbacCheck
        .check(&auth.principal, &Permission::bifrost_record_write())
        .into_result()
        .map_err(IngestError::from_rbac)?;
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

/// Aggregate stream bounds reject before any commit (no DB required): the
/// bounds are enforced entirely inside `collect_frames`, so an oversized/idle
/// stream aborts before the writer is ever opened.
#[cfg(test)]
mod oversized_stream {
    use std::collections::VecDeque;
    use std::time::Duration;

    use wyrd_tonic::tonic::Status;

    use crate::InsertBatchRequest;
    use crate::error::IngestError;
    use crate::limits::IngestLimits;
    use crate::orchestrator::{FrameSource, collect_frames};

    /// In-memory frame source for driving `collect_frames` without a transport.
    struct VecSource {
        frames: VecDeque<Result<InsertBatchRequest, Status>>,
    }

    impl VecSource {
        fn new(frames: Vec<Result<InsertBatchRequest, Status>>) -> Self {
            Self {
                frames: frames.into(),
            }
        }
    }

    impl FrameSource for VecSource {
        async fn next_frame(&mut self) -> Result<Option<InsertBatchRequest>, Status> {
            match self.frames.pop_front() {
                Some(Ok(frame)) => Ok(Some(frame)),
                Some(Err(status)) => Err(status),
                None => Ok(None),
            }
        }
    }

    /// A frame source whose first read never completes, to trip the idle deadline.
    struct SlowSource;

    impl FrameSource for SlowSource {
        async fn next_frame(&mut self) -> Result<Option<InsertBatchRequest>, Status> {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(None)
        }
    }

    fn frame(bytes: usize) -> InsertBatchRequest {
        InsertBatchRequest {
            table: "vala.bifrost.events".to_owned(),
            arrow_ipc: vec![0u8; bytes],
            wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
            frame_sequence: 0,
        }
    }

    #[tokio::test]
    async fn oversized_stream_byte_cap_rejects_before_decode() {
        let limits = IngestLimits {
            max_stream_bytes: 16,
            ..IngestLimits::default()
        };
        // 100 bytes of non-Arrow payload: the byte cap trips before the decoder runs.
        let source = VecSource::new(vec![Ok(frame(100))]);

        let err = collect_frames(source, &limits)
            .await
            .expect_err("byte cap trips");
        assert!(
            matches!(err, IngestError::BatchTooLarge { .. }),
            "expected BatchTooLarge, got {err:?}"
        );
        assert_eq!(err.wyrd_code(), "WYRD_VALA_413_INGEST_OVERSIZED");
    }

    #[tokio::test]
    async fn oversized_stream_frame_cap_rejects() {
        let limits = IngestLimits {
            max_stream_frames: 0,
            ..IngestLimits::default()
        };
        let source = VecSource::new(vec![Ok(frame(4))]);

        let err = collect_frames(source, &limits)
            .await
            .expect_err("frame cap trips");
        assert!(
            matches!(err, IngestError::StreamProtocolViolation(_)),
            "expected StreamProtocolViolation, got {err:?}"
        );
        assert_eq!(err.wyrd_code(), "WYRD_VALA_400_INGEST_PROTO");
    }

    #[tokio::test]
    async fn oversized_stream_idle_deadline_rejects() {
        let limits = IngestLimits {
            idle_deadline: Duration::from_millis(20),
            ..IngestLimits::default()
        };

        let err = collect_frames(SlowSource, &limits)
            .await
            .expect_err("idle deadline trips");
        assert!(
            matches!(err, IngestError::StreamIdle),
            "expected StreamIdle, got {err:?}"
        );
        assert_eq!(err.wyrd_code(), "WYRD_VALA_408_INGEST_IDLE_TIMEOUT");
    }

    #[tokio::test]
    async fn oversized_stream_client_abort_rejects() {
        let source = VecSource::new(vec![Err(Status::cancelled("client went away"))]);

        let err = collect_frames(source, &IngestLimits::default())
            .await
            .expect_err("client abort surfaces");
        assert!(
            matches!(err, IngestError::StreamProtocolViolation(_)),
            "expected StreamProtocolViolation, got {err:?}"
        );
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
