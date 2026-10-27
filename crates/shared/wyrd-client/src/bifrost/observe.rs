//! Fire-and-forget telemetry over the one Bifrost client.
//!
//! The only difference from [`crate::bifrost::Bifrost::insert`] is what happens when the
//! producer refuses: telemetry swallows the refusal and counts it, because
//! losing a signal row is better than breaking the instrumented caller.

use arrow_schema::SchemaRef;

use crate::bifrost::facade::{Bifrost, RecordAdmission};
use crate::bifrost::table::{Correlation, WriterTable};

/// Record one telemetry observation, fire-and-forget.
///
/// Enqueues onto the same pooled producers as [`Bifrost::insert`], but on
/// queue-full it swallows the error and bumps the client's drop counter — the
/// caller is never broken. That is the whole difference between the two, and it
/// is why telemetry has its own function rather than a flag on `insert`.
///
/// Telemetry names its own destination per call rather than using the
/// client's active binding: an instrumented process writes its signals
/// alongside whatever the application is writing, and must not disturb — or be
/// disturbed by — the table the application has bound.
///
/// # Arguments
/// * `bifrost` - The client whose pooled producers and drop counter are used.
/// * `table` - The described destination, whose table version the row's batch
///   names; [`WriterTable::with_schema`] sets the schema the row is checked
///   against at seal.
/// * `json` - One JSON-encoded row.
/// * `correlation` - The card and run identities stamped onto the row.
pub fn record(bifrost: &Bifrost, table: &WriterTable, json: Vec<u8>, correlation: Correlation) {
    let writer = bifrost.writer();
    if writer
        .insert(
            table,
            json,
            correlation.subject_card_uid,
            correlation.run_id,
        )
        .is_err()
    {
        writer.note_drop();
    }
}

/// Describe `fqn` once, then [`record`] the observation against its table
/// version.
///
/// The cache-miss half of telemetry: a writer that has not yet described
/// `fqn` must learn its table version before any batch can name it. Racing
/// misses share one describe through [`Bifrost::writer_table`]'s gate, and a
/// failed describe drops and counts the row exactly as a full queue does.
///
/// # Arguments
/// * `bifrost` - The client whose describe cache, producers, and drop counter are used.
/// * `fqn` - The destination's `<namespace>.<name>`.
/// * `schema` - The schema the row is checked against at seal.
/// * `json` - One JSON-encoded row.
/// * `correlation` - The card and run identities stamped onto the row.
/// * `_admitted` - The [`Bifrost::admit_record`] the caller took before
///   starting this future; held until the row is enqueued or dropped, so a
///   flush or shutdown issued meanwhile waits for it.
///
/// # Cancellation
///
/// Abandoning the future records nothing, caches nothing, and settles the
/// admission.
pub async fn describe_and_record(
    bifrost: &Bifrost,
    fqn: &str,
    schema: SchemaRef,
    json: Vec<u8>,
    correlation: Correlation,
    _admitted: RecordAdmission,
) {
    match bifrost.resolve_writer_table(fqn).await {
        Ok(table) => record(bifrost, &table.with_schema(schema), json, correlation),
        Err(error) => {
            tracing::debug!(table = fqn, %error, "telemetry destination describe failed; row dropped");
            bifrost.writer().note_drop();
        }
    }
}
