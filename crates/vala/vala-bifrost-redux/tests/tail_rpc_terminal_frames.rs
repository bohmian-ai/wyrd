use vala_bifrost_redux::scribe::tail_rpc::{ArrowIpcBatch, TailFrame};
use vala_bifrost_redux::scribe::wal::WalLsn;

#[test]
fn terminal_frame_shape_is_exactly_one_terminal() {
    let frames = [
        TailFrame::Batch(ArrowIpcBatch {
            lsn: WalLsn::new(7),
            batch_id: [7; 16],
            arrow_ipc: vec![1, 2, 3],
        }),
        TailFrame::Exhausted {
            resume_after_lsn: WalLsn::new(7),
        },
    ];
    assert_eq!(
        frames
            .iter()
            .filter(|frame| matches!(frame, TailFrame::Complete | TailFrame::Exhausted { .. }))
            .count(),
        1
    );
}
