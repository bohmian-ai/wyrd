//! Fire-and-forget telemetry over the one Bifrost client.
//!
//! The only difference from [`crate::Bifrost::insert`] is what happens when the
//! producer refuses: telemetry swallows the refusal and counts it, because
//! losing a signal row is better than breaking the instrumented caller.

use arrow_schema::SchemaRef;

use crate::bifrost::Bifrost;
use crate::table::Correlation;

/// Record one telemetry observation, fire-and-forget.
///
/// Enqueues onto the same pooled producers as [`Bifrost::insert`], but on
/// queue-full it swallows the error and bumps the client's drop counter — the
/// caller is never broken. That is the whole difference between the two, and it
/// is why telemetry has its own function rather than a flag on `insert`.
///
/// Telemetry names its own `table` and `schema` per call rather than using the
/// client's active binding: an instrumented process writes its signals
/// alongside whatever the application is writing, and must not disturb — or be
/// disturbed by — the table the application has bound.
pub fn record(
    bifrost: &Bifrost,
    table: &str,
    schema: &SchemaRef,
    json: Vec<u8>,
    correlation: Correlation,
) {
    let writer = bifrost.writer();
    if writer
        .insert(
            table,
            schema,
            json,
            correlation.card_ref,
            correlation.run_id,
        )
        .is_err()
    {
        writer.note_drop();
    }
}
