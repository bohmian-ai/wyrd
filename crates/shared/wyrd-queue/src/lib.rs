//! Generic, reusable write-buffering substrate for Wyrd observation surfaces.
//!
//! `wyrd-queue` owns the DX write-buffering layer and nothing else: a bounded,
//! two-stage in-process queue; a background flush task; the [`BatchBuilder`]
//! that turns buffered JSON rows into one user-only Arrow IPC batch; the
//! [`BatchSink`] trait the sealed batch is drained into; the pure
//! `schema_to_fieldspec` mapping core; backpressure, drain-on-shutdown, and
//! metrics.
//!
//! The crate is PyO3-free and transport-free: it hands a [`SealedBatch`] to a
//! `dyn BatchSink` and knows nothing of gRPC/HTTP/Python or any specific
//! observation kind. A new observation kind is a new [`BatchSink`] impl in its
//! own surface crate, with zero change here.
//!
//! [`BatchBuilder`]: batch_builder::BatchBuilder
//! [`BatchSink`]: sink::BatchSink
//! [`SealedBatch`]: sink::SealedBatch

#![deny(missing_docs)]

pub mod batch_builder;
pub mod config;
pub mod error;
pub mod producer;
pub mod queue;
pub mod schema;
pub mod sink;

pub use batch_builder::BatchBuilder;
pub use config::QueueConfig;
pub use error::WyrdQueueError;
pub use producer::{Producer, ProducerMetrics};
pub use queue::{Flushable, RecordQueue, Row};
pub use schema::{
    arrow_schema_to_fieldspec, fieldspec_to_arrow, json_schema_to_arrow, json_schema_to_fieldspec,
};
pub use sink::{BatchSink, MockSink, SealedBatch};
