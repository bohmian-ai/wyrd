//! The stateful query decoder's contract, including partial delivery.
//!
//! Module of the `oracle` group; shared fixtures live in `support.rs`.

use std::sync::Arc;
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, Oracle, OracleConfig, QueryIpcDecoder, TailTransportDirectory,
    TestPostgresOracleAudit,
};
use wyrd_spec::vala::api::{
    BifrostQueryRequest, FreshnessPolicy, QueryStreamFrame, QueryTerminalOutcome, VisibilityMode,
};

use super::support::*;
use futures_util::StreamExt;

/// Proves an empty successful result is a schema frame and a closing terminal.
///
/// An empty result has no batch frame at all, so the terminal's end-of-stream
/// delta is the only evidence the stream completed rather than being truncated.
///
/// # Panics
///
/// Panics when the query fails, emits a batch, or does not close its stream.
async fn assert_empty_success_is_schema_then_close(
    oracle: &Oracle,
    context: AuthorizedQueryContext,
    table: &str,
) {
    let mut empty = oracle
        .query_sql(
            context,
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {table} WHERE false"),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("empty Oracle SQL");
    let mut empty_ipc = QueryIpcDecoder::new();
    let mut empty_rows = 0_usize;
    let mut empty_terminal = None;
    while let Some(frame) = empty.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(frame) => {
                empty_ipc
                    .accept_schema(&frame.arrow_ipc_schema)
                    .expect("schema prefix decodes");
            }
            QueryStreamFrame::Batch(frame) => {
                empty_rows += empty_ipc
                    .accept_batch(&frame.arrow_ipc_batch)
                    .expect("batch delta decodes")
                    .num_rows();
            }
            QueryStreamFrame::Terminal(frame) => {
                empty_ipc
                    .accept_eos(&frame.arrow_ipc_eos)
                    .expect("empty success still closes the stream");
                empty_terminal = Some(frame);
            }
        }
    }
    assert_eq!(empty_rows, 0);
    assert_eq!(
        empty_terminal.expect("empty terminal").row_count,
        0,
        "an empty success is schema then a closing terminal"
    );
}

/// One Oracle query stream is one split Arrow IPC stream closed by its terminal.
///
/// Proves the wire contract end to end against a real query: exactly one schema
/// prefix, one bare continuation delta per batch that is strictly smaller than
/// a standalone re-encode of the same batch, and exactly one end-of-stream
/// delta carried by the terminal. It also proves an empty successful result is
/// schema-then-terminal, with no batch frame and a real close.
#[tokio::test]
async fn stateful_query_decoder_contract() {
    let fixture = OracleFixture::new("oracle_stateful_ipc").await;
    let _seeded = fixture
        .seed_hot_rows_at_epoch(
            "scribe-018f7ca27a4d7cc198a797fdd1f15101-epoch-1-shard-0-wal-1-1-00000.parquet",
            &[(11, fixture.tenant), (22, fixture.tenant)],
            1,
        )
        .await;
    let oracle = fixture
        .oracle(
            Arc::new(TestPostgresOracleAudit::new(
                fixture.pg.vala_postgres().clone(),
            )),
            Arc::new(TailTransportDirectory::default()),
            OracleConfig::default(),
        )
        .await;

    let mut stream = oracle
        .query_sql(
            fixture.context(),
            BifrostQueryRequest {
                sql: format!("SELECT value FROM {} ORDER BY value", fixture.table.fqn()),
                visibility: VisibilityMode::PublishedOnly,
                freshness: FreshnessPolicy::Strict,
                deadline_ms: Some(5_000),
            },
        )
        .await
        .expect("Oracle SQL");
    let mut ipc = QueryIpcDecoder::new();
    let mut schema_frames = 0_usize;
    let mut fragments = Vec::new();
    let mut rows = 0_u64;
    let mut terminal = None;
    while let Some(frame) = stream.frames.next().await {
        match frame.expect("query frame") {
            QueryStreamFrame::Schema(frame) => {
                schema_frames += 1;
                ipc.accept_schema(&frame.arrow_ipc_schema)
                    .expect("schema prefix decodes");
            }
            QueryStreamFrame::Batch(frame) => {
                let decoded = ipc
                    .accept_batch(&frame.arrow_ipc_batch)
                    .expect("batch delta decodes");
                let mut standalone = Vec::new();
                let mut writer = arrow::ipc::writer::StreamWriter::try_new(
                    &mut standalone,
                    decoded.schema().as_ref(),
                )
                .expect("standalone writer starts");
                writer.write(&decoded).expect("standalone batch writes");
                writer.finish().expect("standalone writer finishes");
                drop(writer);
                assert!(
                    frame.arrow_ipc_batch.len() < standalone.len(),
                    "a continuation delta must be smaller than a standalone stream"
                );
                rows += u64::try_from(decoded.num_rows()).expect("row count fits u64");
                fragments.push(frame.arrow_ipc_batch.len());
            }
            QueryStreamFrame::Terminal(frame) => {
                assert!(terminal.is_none(), "query emitted duplicate terminal");
                assert_eq!(frame.outcome, QueryTerminalOutcome::Success);
                ipc.accept_eos(&frame.arrow_ipc_eos)
                    .expect("terminal closes the stream");
                terminal = Some(frame);
            }
        }
    }
    assert_eq!(schema_frames, 1, "the stream carries exactly one schema");
    assert!(ipc.eos_accepted(), "the stream is explicitly closed");
    let terminal = terminal.expect("query terminal");
    assert_eq!(terminal.row_count, rows);
    assert_eq!(rows, 2);
    let largest = fragments.into_iter().max().expect("query returned batches");
    assert!(
        ipc.peak_pending_frame_bytes() <= largest,
        "the decoder retains at most one fragment at a time"
    );

    assert_empty_success_is_schema_then_close(&oracle, fixture.context(), &fixture.table.fqn())
        .await;

    shutdown_oracle(&oracle).await;
}
