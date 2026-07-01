//! Aggregate stream bounds reject before any commit (no DB required): the
//! bounds are enforced entirely inside `collect_frames`, so an oversized/idle
//! stream aborts before the writer is ever opened.

use std::collections::VecDeque;
use std::time::Duration;

use vala_ingest::InsertBatchRequest;
use vala_ingest::error::IngestError;
use vala_ingest::limits::IngestLimits;
use vala_ingest::orchestrator::{FrameSource, collect_frames};
use wyrd_tonic::tonic::Status;

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

    let err = collect_frames(source, &limits).await.expect_err("byte cap trips");
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
