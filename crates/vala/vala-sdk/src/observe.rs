//! Fire-and-forget telemetry: enqueue onto the pooled producer, **swallowing**
//! queue-full so a saturated buffer never breaks the caller.

use arrow_schema::SchemaRef;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::handle::Bifrost;
use crate::scope::SinkKind;

/// Record one telemetry observation, fire-and-forget.
///
/// Enqueues onto the same pooled producer as [`Bifrost::insert`], but on
/// queue-full it swallows the error and bumps the handle's drop counter —
/// the caller is never broken.
pub fn record(
    bifrost: &Bifrost,
    kind: SinkKind,
    table: &str,
    schema: &SchemaRef,
    json: Vec<u8>,
    card_ref: CardRef,
    run_id: Option<RunId>,
) {
    if bifrost
        .insert(kind, table, schema, json, card_ref, run_id)
        .is_err()
    {
        bifrost.note_drop();
    }
}
