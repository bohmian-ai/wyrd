//! Shared helpers for OTLP journey tests.

use std::fmt::Debug;
use std::future::Future;

use vala_bifrost_redux::gate::limits::IngestLimits;
use wyrd_testing::WyrdTestServer;
use wyrd_tonic::tonic::{Code, Status};
use wyrd_tonic::tonic_types::StatusExt;

/// Build a small, production-shaped limits snapshot for public OTLP boundaries.
///
/// The snapshot lowers only operator-configurable ceilings. One event day is
/// sufficient because OTLP projection stamps a single receipt day, while the
/// one-mebibyte material ceiling keeps successful direct-write planning bounded.
#[must_use]
pub(crate) fn public_otlp_limits(records: usize, value_depth: usize) -> IngestLimits {
    let mut limits = IngestLimits::default();
    limits.max_frame_bytes = 1024 * 1024;
    limits.max_decoding_message_size = limits.max_frame_bytes + 64 * 1024;
    limits.rows = records;
    limits.otlp.request_bytes = limits.max_frame_bytes;
    limits.otlp.resources = records;
    limits.otlp.scopes = records;
    limits.otlp.records = records;
    limits.otlp.attributes = 64;
    limits.otlp.value_bytes = 64 * 1024;
    limits.otlp.value_depth = value_depth;
    limits.otlp.event_days = 1;
    limits.otlp.material_bytes = 1024 * 1024;
    limits
}

/// Assert that a public OTLP completion retained no request or projection owner.
///
/// This reads the production Scribe counters after a success or refusal. Active
/// memtable bytes may remain until sealing, but request-scoped item/byte charges
/// and ingress-lane work must settle before the public result is returned.
///
/// # Panics
/// Panics when the bound fixture has no Scribe or retains request-scoped work.
pub(crate) fn assert_otlp_owner_settled(srv: &WyrdTestServer) {
    let scribe = srv
        .bifrost_scribe()
        .expect("bound OTLP fixture owns Scribe");
    let runtime = scribe.runtime_snapshot();
    assert_eq!(runtime.admission.items, 0, "OTLP item owner settled");
    assert_eq!(runtime.admission.bytes, 0, "OTLP byte owner settled");
    assert_eq!(runtime.ingress.depth, 0, "OTLP projection lane settled");

    let lifecycle = scribe
        .inspection_snapshot()
        .expect("coherent OTLP owner inspection")
        .ingress_lifecycle;
    assert_eq!(lifecycle.active_attempts, 0, "OTLP attempt settled");
    assert_eq!(lifecycle.active_reservations, 0, "OTLP root settled");
    assert_eq!(
        lifecycle.active_reserved_bytes, 0,
        "OTLP root bytes settled"
    );
    assert_eq!(
        lifecycle.active_materializations, 0,
        "OTLP materialization settled"
    );
    assert_eq!(
        lifecycle.active_materialized_bytes, 0,
        "OTLP materialized bytes settled"
    );
    assert_eq!(
        lifecycle.active_shard_transfers, 0,
        "OTLP shard transfer settled"
    );
    assert_eq!(
        lifecycle.active_shard_transferred_bytes, 0,
        "OTLP shard-transferred root bytes settled"
    );
    assert_eq!(lifecycle.plans, lifecycle.reservations);
    assert_eq!(lifecycle.reservations, lifecycle.releases);
    assert_eq!(lifecycle.shard_transfers, lifecycle.reservations);
    assert_eq!(lifecycle.shard_transferred_bytes, lifecycle.reserved_bytes);
    assert_eq!(lifecycle.reserved_bytes, lifecycle.released_bytes);
    assert!(
        lifecycle.transfers > 0 && lifecycle.transfers <= lifecycle.materializations,
        "WAL transfers are a nonempty subset of materializations; post-COMMIT regeneration may materialize again: {lifecycle:?}"
    );
    assert!(
        lifecycle.transferred_bytes > 0
            && lifecycle.transferred_bytes <= lifecycle.materialized_bytes,
        "WAL-owned bytes are a nonempty subset of materialized bytes; deterministic regeneration remains root-owned: {lifecycle:?}"
    );
    assert_eq!(
        lifecycle.succeeded
            + lifecycle.refused
            + lifecycle.cancelled
            + lifecycle.panicked
            + lifecycle.shutdown,
        lifecycle.attempts,
        "every Scribe-reached OTLP attempt has one terminal result"
    );
    assert!(lifecycle.planned_bytes > 0, "OTLP root plan observed");
    assert!(lifecycle.planned_sources > 0, "OTLP source plan observed");
    assert!(
        lifecycle.planned_event_days > 0,
        "OTLP receipt-day plan observed"
    );
    assert!(lifecycle.planned_rows > 0, "OTLP row plan observed");
    assert!(
        lifecycle.materializations > 0,
        "OTLP direct materialization observed: {lifecycle:?}"
    );
    assert!(
        lifecycle.materialized_bytes > 0,
        "OTLP materialized bytes observed"
    );
    assert!(
        lifecycle.shard_transfers > 0,
        "OTLP shard transfer observed"
    );
}

/// Assert the stable gRPC refusal for an OTLP planning-limit excess.
///
/// # Panics
/// Panics when the public code or structured Wyrd error identity drifts.
pub(crate) fn assert_grpc_otlp_limit(error: &Status) {
    assert_eq!(error.code(), Code::InvalidArgument);
    assert_eq!(
        error
            .get_details_error_info()
            .expect("OTLP limit error info")
            .reason,
        "WYRD_VALA_400_OTLP_REQUEST_MALFORMED"
    );
}

/// Assert the stable HTTP refusal for an OTLP planning-limit excess.
///
/// # Panics
/// Panics when the response status or structured Wyrd error identity drifts.
pub(crate) async fn assert_http_otlp_limit(response: reqwest::Response) {
    assert_eq!(response.status(), 400);
    let problem: serde_json::Value = response.json().await.expect("OTLP problem JSON");
    assert_eq!(
        problem.get("code").and_then(serde_json::Value::as_str),
        Some("WYRD_VALA_400_OTLP_REQUEST_MALFORMED")
    );
}

/// Complete one successful OTLP export and cross its durable read barrier.
///
/// OTLP admission returns after Gate accepts work; the bound test server's
/// Scribe publishes that work asynchronously. Journey tests must therefore
/// flush the server-owned Scribe before querying the exported rows, matching
/// the documented write → flush → read workflow without changing production
/// backpressure or publication behavior.
///
/// # Panics
/// Panics when the export future returns an error or the server-owned Scribe
/// cannot flush the accepted batch.
pub(crate) async fn export_and_flush<T, E, F>(srv: &WyrdTestServer, export: F) -> T
where
    E: Debug,
    F: Future<Output = Result<T, E>>,
{
    let response = export.await.expect("OTLP export succeeds");
    srv.flush_bifrost()
        .await
        .expect("successful OTLP export flushes");
    response
}

/// Assert that a rejected OTLP request produced no durable span rows.
///
/// The flush is a proof barrier: it lets any erroneously admitted rejected
/// request reach the durable publication path before the tenant-scoped
/// file-list probe runs. A fresh bound server has no published span files when
/// the rejection was correctly fail-closed, so querying the empty catalog
/// through the public Oracle path would be an internal planning error rather
/// than a not-found result. The fixture's file-list probe keeps the assertion
/// deterministic without changing production query behavior.
///
/// # Panics
/// Panics when the flush, tenant connection, file-list query, or assertion
/// transaction cannot complete, or when any durable span row is present.
pub(crate) async fn assert_no_durable_spans(srv: &WyrdTestServer, trace_id: [u8; 16]) {
    srv.flush_bifrost()
        .await
        .expect("rejected OTLP request flush barrier");
    let mut conn = srv
        .tenant_conn_for(srv.data_tenant_id())
        .await
        .expect("tenant connection for durable-span assertion");
    let row_count: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(row_count), 0)::bigint
           FROM vala.file_list
          WHERE namespace = 'vala.traces' AND table_name = 'spans'",
    )
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("durable span file-list query");
    conn.commit()
        .await
        .expect("durable span assertion transaction commits");
    assert_eq!(
        row_count, 0,
        "no durable span rows may exist for trace {trace_id:02x?}"
    );
}
