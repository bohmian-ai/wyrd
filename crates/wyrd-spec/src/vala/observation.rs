//! Observation forward contract for the Vala ingest surface.
//!
//! Stage-3 ships a single observation kind: `Record` — a columnar record batch
//! routed to a named Bifrost table. Drift and Eval variants arrive in later
//! stages via explicit versioned contract changes.
//!
//! The envelope has **no Stage-3 wire consumer**: schemas are codegen'd for
//! downstream tooling, but no transport path is defined here. The C1 ingest
//! wire carries `card_ref` and `run_id` as per-row Arrow correlation columns.
//!
//! Per `architecture/wyrd-design.md`, `wyrd-spec` is IO/async/PyO3-free.

use serde::{Deserialize, Serialize};

use crate::reference::CardRef;
use crate::vala::api::BifrostTableName;
use crate::vala::ids::RunId;

/// Envelope carried on every observation from an agent run.
///
/// `card_ref` is required; `run_id` is optional and omitted from the wire when
/// absent. On the C1 ingest wire both fields travel as per-row Arrow
/// correlation columns alongside the columnar payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ObservationEnvelope {
    /// Card that produced this observation.
    pub card_ref: CardRef,
    /// Run identifier grouping all observations from one agent invocation.
    ///
    /// Optional; omitted from the wire when not present. On the C1 ingest wire
    /// this travels as a per-row Arrow correlation column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
}

/// Closed observation-kind taxonomy for Stage 3.
///
/// Variants map one-to-one to the `BatchSink`/gRPC-service pairs that ingest
/// them (`Record` → `BifrostIngestSink` → `wyrd.v1.BifrostIngestService`).
/// Drift and Eval variants arrive in later stages; this enum is intentionally
/// **not** `#[non_exhaustive]` — new variants require an explicit versioned
/// contract change (review M-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObservationKind {
    /// Columnar record batch destined for a Bifrost table.
    Record(RecordObservation),
}

/// Metadata descriptor for a columnar record observation.
///
/// The columnar payload travels out-of-band as an Arrow batch on the C1 ingest
/// wire; this struct identifies only the destination table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct RecordObservation {
    /// Destination Bifrost table for the record batch.
    pub table: BifrostTableName,
}
