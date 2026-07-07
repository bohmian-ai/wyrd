//! Vala client SDK — the client-tier Bifrost ingest surface.
//!
//! `vala-sdk` ships buffered JSON rows to the Wyrd ingest service as Record
//! batches. It owns three things:
//!
//! - [`BifrostIngestSink`] — the `wyrd-queue` [`BatchSink`] that maps one sealed
//!   batch onto the ingest RPC (`table`, `wyrd_batch_id`, Arrow IPC frames),
//!   preserving `batch_id` so the server's commit dedup holds across retries.
//! - [`Bifrost`] — the write handle that pools one [`Producer`] per destination,
//!   keyed by `(ClientScope, SinkKind, table)`.
//! - [`observe`] — fire-and-forget telemetry that never breaks its caller.
//!
//! [`ClientScope`] is a credential fingerprint the **token-opaque** client tier
//! computes without ever decoding a JWT: `(server_url, SHA-256 of the resolved
//! credential's secret material)`. Backpressure is asymmetric — [`Bifrost::insert`]
//! propagates queue-full to the caller, while [`observe::record`] swallows it,
//! counts the drop, and warns.
//!
//! [`BatchSink`]: wyrd_queue::BatchSink
//! [`Producer`]: wyrd_queue::Producer

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]

pub mod handle;
pub mod observe;
#[cfg(feature = "python")]
pub mod python;
pub mod scope;
pub mod sink;

pub use handle::{Bifrost, schema_from_json_schema};
pub use scope::{ClientScope, SinkKind};
pub use sink::{BifrostIngestSink, IngestTransport};

// C4a forward schema helpers, re-exported so SDK users build the user Arrow
// schema from a `FieldSpec` set or a JSON-Schema value without reaching into
// `wyrd-queue` directly.
pub use wyrd_queue::{fieldspec_to_arrow, json_schema_to_arrow};
