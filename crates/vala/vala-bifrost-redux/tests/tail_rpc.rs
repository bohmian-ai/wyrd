//! Phase-2 `FetchLiveTail` server tests — gated on `scribe-inspect`.
//!
//! Covers the four invariants called out in the plan:
//!
//! 1. A request targeting a different `(node_id, writer_epoch)` returns
//!    `WYRD_VALA_409_STREAM_MISMATCH` without touching WAL state.
//! 2. Only WAL data records with `LSN > after_lsn` on the current stream flow
//!    through the response.
//! 3. Records covered by a stream-scoped sealed range are excluded so the
//!    union of `file_list` + live-tail response covers each source row exactly
//!    once, at pre-freeze / mid-seal / post-retire phases.
//! 4. `SealedRangeIndex` scopes exclusion per stream — ranges from another
//!    `(node_id, writer_epoch)` never suppress this stream's rows (C4).

use std::sync::Arc;

use tempfile::TempDir;
use uuid::Uuid;
use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
use vala_bifrost_redux::scribe::tail_rpc::{
    FetchLiveTailRequest, FetchLiveTailService, SealedRange, SealedRangeIndex,
};
use vala_bifrost_redux::scribe::wal::{WalLsn, WalWriter};
use wyrd_spec::DataTenantId;

fn stream(seed: u8, epoch: i64) -> StreamIdentity {
    StreamIdentity::new(
        NodeId::new(Uuid::from_bytes([seed; 16])),
        WriterEpoch::new(epoch),
    )
}

fn make_wal(dir: &TempDir, stream: StreamIdentity) -> WalWriter {
    WalWriter::new(
        dir.path(),
        *stream.node_id.as_uuid().as_bytes(),
        stream.writer_epoch.as_i64(),
        DataTenantId::SYSTEM_OWNER,
        None,
    )
    .expect("WAL writer")
}

fn append(wal: &WalWriter, seed: u8) -> WalLsn {
    wal.append_and_fsync(
        [seed; 16],
        format!("audit-{seed}").into_bytes(),
        format!("data-{seed}").into_bytes(),
    )
    .expect("WAL append")
}

#[tokio::test]
async fn live_tail_rejects_wrong_stream() {
    let dir = TempDir::new().expect("temp dir");
    let this = stream(1, 1);
    let wal = make_wal(&dir, this);
    append(&wal, 0);

    let service = FetchLiveTailService::new(
        this,
        dir.path().to_path_buf(),
        Arc::new(SealedRangeIndex::new()),
    );

    // Different node → mismatch.
    let other_node = stream(2, 1);
    let err = service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: other_node,
            after_lsn: None,
        })
        .await
        .expect_err("mismatch");
    assert_eq!(
        err.code(),
        "WYRD_VALA_409_STREAM_MISMATCH",
        "different node_id returns stream mismatch"
    );

    // Same node, different epoch → still mismatch.
    let other_epoch = stream(1, 2);
    let err = service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: other_epoch,
            after_lsn: None,
        })
        .await
        .expect_err("mismatch");
    assert_eq!(
        err.code(),
        "WYRD_VALA_409_STREAM_MISMATCH",
        "different writer_epoch returns stream mismatch"
    );

    // Sanity: correct stream returns data.
    let ok = service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: this,
            after_lsn: None,
        })
        .await
        .expect("match");
    assert_eq!(ok.len(), 1, "one data record on WAL");
}

#[tokio::test]
async fn live_tail_returns_only_rows_above_after_lsn() {
    let dir = TempDir::new().expect("temp dir");
    let this = stream(3, 1);
    let wal = make_wal(&dir, this);

    let lsns: Vec<WalLsn> = (0u8..5).map(|i| append(&wal, i)).collect();
    let cutoff = lsns[1]; // after_lsn = LSN of the second append (LSN 1)

    let service = FetchLiveTailService::new(
        this,
        dir.path().to_path_buf(),
        Arc::new(SealedRangeIndex::new()),
    );

    let out = service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: this,
            after_lsn: Some(cutoff),
        })
        .await
        .expect("tail");

    assert!(
        out.iter().all(|b| b.lsn > cutoff),
        "no row with LSN ≤ {cutoff} may be returned; got {:?}",
        out.iter().map(|b| b.lsn).collect::<Vec<_>>()
    );
    // LSNs 2, 3, 4 are strictly greater than cutoff (=1), so exactly three rows.
    assert_eq!(out.len(), 3, "expected 3 records past the cutoff");
}

#[tokio::test]
async fn live_tail_during_mid_seal_does_not_double_count() {
    let dir = TempDir::new().expect("temp dir");
    let this = stream(4, 1);
    let wal = make_wal(&dir, this);

    let lsns: Vec<WalLsn> = (0u8..5).map(|i| append(&wal, i)).collect();

    // Split the WAL into a "sealed" portion (first three) and a "live tail"
    // portion (last two). At each phase, the union of file_list + live-tail
    // must cover every source LSN exactly once.

    // Phase 1 — pre-freeze: nothing sealed yet. Live tail carries all rows.
    let pre = FetchLiveTailService::new(
        this,
        dir.path().to_path_buf(),
        Arc::new(SealedRangeIndex::new()),
    );
    let pre_out = pre
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: this,
            after_lsn: None,
        })
        .await
        .expect("pre-freeze tail");
    let pre_seen: Vec<WalLsn> = pre_out.iter().map(|b| b.lsn).collect();
    assert_eq!(pre_seen, lsns, "pre-freeze: live tail covers every row");

    // Phase 2 — mid-seal: LSNs [0..=2] are covered by a sealed range.
    let sealed = Arc::new(SealedRangeIndex::from_ranges([SealedRange {
        stream: this,
        wal_lsn_min: lsns[0],
        wal_lsn_max: lsns[2],
    }]));
    let file_list_mid: Vec<WalLsn> = lsns[0..=2].to_vec();
    let mid_service =
        FetchLiveTailService::new(this, dir.path().to_path_buf(), Arc::clone(&sealed));
    let mid_out = mid_service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: this,
            after_lsn: None,
        })
        .await
        .expect("mid-seal tail");
    let mid_seen: Vec<WalLsn> = mid_out.iter().map(|b| b.lsn).collect();
    assert_eq!(
        mid_seen,
        lsns[3..].to_vec(),
        "mid-seal: live tail excludes sealed range"
    );
    let mut mid_union: Vec<WalLsn> = file_list_mid.iter().copied().chain(mid_seen).collect();
    mid_union.sort();
    assert_eq!(
        mid_union, lsns,
        "mid-seal: union covers every row exactly once"
    );

    // Phase 3 — post-retire: every LSN is covered by a sealed range.
    let sealed_all = Arc::new(SealedRangeIndex::from_ranges([SealedRange {
        stream: this,
        wal_lsn_min: lsns[0],
        wal_lsn_max: *lsns.last().expect("nonempty"),
    }]));
    let file_list_all: Vec<WalLsn> = lsns.clone();
    let post_service =
        FetchLiveTailService::new(this, dir.path().to_path_buf(), Arc::clone(&sealed_all));
    let post_out = post_service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: this,
            after_lsn: None,
        })
        .await
        .expect("post-retire tail");
    assert!(post_out.is_empty(), "post-retire: live tail is empty");
    let mut post_union: Vec<WalLsn> = file_list_all
        .into_iter()
        .chain(post_out.iter().map(|b| b.lsn))
        .collect();
    post_union.sort();
    assert_eq!(
        post_union, lsns,
        "post-retire: union covers every row exactly once"
    );
}

#[tokio::test]
async fn live_tail_sealed_range_index_ignores_other_streams() {
    let dir = TempDir::new().expect("temp dir");
    let this = stream(5, 1);
    let other = stream(6, 1);
    let wal = make_wal(&dir, this);

    let lsns: Vec<WalLsn> = (0u8..3).map(|i| append(&wal, i)).collect();

    // Simulate a `file_list` snapshot containing rows from two producing
    // streams: one attributed to `this` (covers LSN[0]) and one attributed to
    // `other` that happens to name the same LSN space. C4 was the collision
    // bug where the "other" range could suppress this stream's LSN 1.
    let sealed = Arc::new(SealedRangeIndex::from_ranges([
        SealedRange {
            stream: this,
            wal_lsn_min: lsns[0],
            wal_lsn_max: lsns[0],
        },
        SealedRange {
            stream: other,
            wal_lsn_min: lsns[1],
            wal_lsn_max: lsns[2],
        },
    ]));

    // Sanity — `contains` scopes per stream.
    assert!(
        sealed.contains(this, lsns[0]),
        "this-stream LSN 0 is sealed"
    );
    assert!(
        !sealed.contains(this, lsns[1]),
        "other-stream range must not suppress this-stream LSN 1"
    );
    assert!(
        !sealed.contains(this, lsns[2]),
        "other-stream range must not suppress this-stream LSN 2"
    );
    assert!(
        sealed.contains(other, lsns[1]),
        "other-stream LSN 1 is sealed for other"
    );

    let service = FetchLiveTailService::new(this, dir.path().to_path_buf(), sealed);
    let out = service
        .fetch_live_tail(FetchLiveTailRequest {
            target_stream: this,
            after_lsn: None,
        })
        .await
        .expect("tail");

    let seen: Vec<WalLsn> = out.iter().map(|b| b.lsn).collect();
    assert_eq!(
        seen,
        vec![lsns[1], lsns[2]],
        "only this-stream's sealed range excludes rows; other-stream range must not"
    );
}
