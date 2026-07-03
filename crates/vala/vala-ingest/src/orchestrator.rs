//! Stream collection, the three pre-flush authorization gates, and the 2PC
//! commit that drives `vala-bifrost`'s writer.
//!
//! The heavy lifting is split so the bounds/decode and the card-scope gate are
//! testable without a database: [`collect_frames`] and [`validate_card_scope`]
//! take in-memory inputs and never touch the catalog.

use std::str::FromStr;
use std::time::Instant;

use arrow::array::{Array, StringArray};
use arrow::record_batch::RecordBatch;
use tokio::time::timeout;
use vala_bifrost::writer::BifrostWriteContext;
use vala_bifrost::{BifrostNamespace, TableScope, WyrdCatalog};
use wyrd_runtime::{Permission, PermissionCheck, Principal, RbacCheck};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::CARD_REF;
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
                return Err(IngestError::BatchTooLarge {
                    bytes: total_rows,
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
/// be a member of the principal's card scope (exact `CardRef` equality). A
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
    let scope = principal.card_scope();
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
            if !scope.contains(&card) {
                return Err(IngestError::CardScopeDenied {
                    card_ref: raw.to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Resolve a `namespace.name` fqn into a [`BifrostNamespace`] + table name.
///
/// # Errors
/// Returns [`IngestError::StreamProtocolViolation`] when the fqn does not name a
/// known Bifrost namespace or is not a single `namespace.name` pair.
pub fn resolve_fqn(fqn: &str) -> Result<(BifrostNamespace, String), IngestError> {
    for ns in [
        BifrostNamespace::System,
        BifrostNamespace::Bifrost,
        BifrostNamespace::Traces,
        BifrostNamespace::Eval,
    ] {
        let prefix = format!("{}.", ns.as_str());
        if let Some(name) = fqn.strip_prefix(&prefix) {
            if name.is_empty() || name.contains('.') {
                continue;
            }
            return Ok((ns, name.to_owned()));
        }
    }
    Err(IngestError::StreamProtocolViolation(format!(
        "unrecognized table fqn: {fqn}"
    )))
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

    let writer = catalog
        .writer(namespace, &name, TableScope::TenantOwned, auth.tenant)
        .await
        .map_err(IngestError::from_engine)?;

    for batch in collected.batches {
        writer
            .write(batch)
            .await
            .map_err(IngestError::from_engine)?;
    }

    let ctx = BifrostWriteContext {
        batch_id: collected.batch_id,
        origin: "ingest".to_owned(),
        actor: auth.principal.id.to_string(),
        request_id: auth.request_id.clone(),
        // Writer-identity card for C5 audit attribution (constant per commit).
        card_ref: auth.principal.card_ref().cloned(),
    };
    let _snapshot = writer.flush(ctx).await.map_err(IngestError::from_engine)?;

    Ok(collected.rows)
}
