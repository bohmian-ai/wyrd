//! Shared helpers for OTLP journey tests.

use std::fmt::Debug;
use std::future::Future;

use wyrd_testing::WyrdTestServer;

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
