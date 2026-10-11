//! `vala.eval.observations` — the fixed Eval input table.
//!
//! Owns the authored column sequence the SDK's Eval projection must match
//! exactly at `start_bifrost`, plus the table's sensitivity and layout.

use std::io::Cursor;

use arrow::array::{Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::Field;
use arrow::ipc::reader::StreamReader;
use chrono::{DateTime, Utc};
use wyrd_runtime::Principal;
use wyrd_spec::ids::CardUid;

use wyrd_types::variant::variant_field;

use crate::contracts::ScribeError;
use crate::scribe::execution_lanes::resolve_card_uids;

use crate::tables::fields::{fixed_binary, ts_us_utc, utf8};
use crate::tables::{CorrelationPolicy, DomainTable, PayloadClass, daily_layout, sort_desc};
use wyrd_spec::vala::api::PhysicalLayoutWire;

/// `vala.eval.observations` — one row per committed Eval input record.
///
/// A committed row is what later activates every matching `observations_ready`
/// binding, so the raw input is stored once and attributed to its observed
/// subject through the managed `card_uid`; it is never duplicated per binding
/// and carries no Verifier or binding identity. `trace_id` and `span_id` use
/// the same fixed-width binary identities as `vala.traces.spans` so an Eval
/// record joins directly to the span that produced it. `context` and `media`
/// are Variants holding the record's JSON values and are classified sensitive, so projecting them
/// requires the elevated payload permission.
pub struct ObservationsTable;

impl DomainTable for ObservationsTable {
    /// Lives in `vala.eval`, the namespace the catalog registers it under.
    const NAMESPACE: &'static str = "eval";
    /// Table segment of the fixed `vala.eval.observations` FQN.
    const NAME: &'static str = "observations";
    /// The server appends and stamps the managed `run_id`, observed `card_uid`, and `principal_id`.
    const CORRELATION_POLICY: CorrelationPolicy = CorrelationPolicy::Observation;
    /// Raw Eval input makes projecting this table require the elevated payload permission.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Sensitive;
    /// The raw input columns gated behind the elevated payload permission.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &["context", "media"];

    /// Authored columns in the exact order the SDK Eval projection must match at `start_bifrost`.
    fn arrow_fields() -> Vec<Field> {
        vec![
            utf8("record_id", false),
            utf8("session_id", true),
            variant_field("context", false),
            fixed_binary("trace_id", 16, true),
            fixed_binary("span_id", 8, true),
            ts_us_utc("created_at", false),
            variant_field("media", true),
        ]
    }

    /// Daily partitions sorted newest first; rows are read by record, not by a pruned key.
    fn physical_layout() -> PhysicalLayoutWire {
        daily_layout("created_at", vec![sort_desc("created_at")], &[])
    }
}

/// The run-activation key of one acknowledged Eval observation row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcknowledgedObservation {
    /// Exact producer-owned record identity.
    pub record_id: String,
    /// Observed subject Scribe stamped as the row's managed `card_uid`.
    pub card_uid: CardUid,
    /// The row's `created_at`, the table's time column.
    pub created_at: DateTime<Utc>,
}

impl ObservationsTable {
    /// Derive each acknowledged row's activation key from its admitted frame.
    ///
    /// Re-derives the subject UID Scribe stamped through the same signed-scope
    /// resolution, and reads each row's `created_at`. Rows without a subject
    /// activate nothing and are omitted.
    ///
    /// # Errors
    /// Returns [`ScribeError::InvalidFrame`] when the frame does not decode or
    /// lacks a well-typed `record_id` or `created_at`, and
    /// [`ScribeError::CardUnresolved`] when a subject reference no longer
    /// resolves.
    pub fn acknowledged(
        frame: &[u8],
        principal: &Principal,
    ) -> Result<Vec<AcknowledgedObservation>, ScribeError> {
        let reader = StreamReader::try_new(Cursor::new(frame), None)
            .map_err(|_| ScribeError::InvalidFrame)?;
        let mut keys = Vec::new();
        for rows in reader {
            let rows = rows.map_err(|_| ScribeError::InvalidFrame)?;
            let card_uids = resolve_card_uids(&rows, principal, rows.num_rows())?;
            let records = rows
                .column_by_name("record_id")
                .and_then(|column| column.as_any().downcast_ref::<StringArray>())
                .ok_or(ScribeError::InvalidFrame)?;
            let created = rows
                .column_by_name("created_at")
                .and_then(|column| column.as_any().downcast_ref::<TimestampMicrosecondArray>())
                .ok_or(ScribeError::InvalidFrame)?;
            for (row, card_uid) in card_uids.into_iter().enumerate() {
                let Some(card_uid) = card_uid else { continue };
                keys.push(AcknowledgedObservation {
                    record_id: records.value(row).to_owned(),
                    card_uid: card_uid.parse().map_err(|_| ScribeError::CardUnresolved)?,
                    created_at: DateTime::from_timestamp_micros(created.value(row))
                        .ok_or(ScribeError::InvalidFrame)?,
                });
            }
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{Field, Schema};
    use arrow::ipc::writer::StreamWriter;
    use arrow::record_batch::RecordBatch;
    use uuid::Uuid;
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::CardUid;
    use wyrd_spec::reference::{CardRef, CardRefScope};

    use super::ObservationsTable;

    /// Encode `columns` as one Arrow IPC stream frame.
    ///
    /// # Panics
    /// Panics when the batch or stream cannot be built.
    fn frame(columns: Vec<(&str, ArrayRef)>) -> Vec<u8> {
        let fields: Vec<Field> = columns
            .iter()
            .map(|(name, column)| Field::new(*name, column.data_type().clone(), true))
            .collect();
        let schema = Arc::new(Schema::new(fields));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            columns.into_iter().map(|(_, column)| column).collect(),
        )
        .expect("batch builds");
        let mut bytes = Vec::new();
        let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("writer opens");
        writer.write(&batch).expect("batch writes");
        writer.finish().expect("stream finishes");
        drop(writer);
        bytes
    }

    /// Keys carry the signed subject UID and the row's `created_at`, and a row
    /// without a subject yields no key.
    ///
    /// # Panics
    /// Panics when a derived key differs from what Scribe commits.
    #[test]
    fn acknowledged_keys_carry_subject_and_created_at() {
        let uid = CardUid::new(Uuid::now_v7().to_string()).expect("card uid");
        let card = CardRef {
            uid: Some(uid.clone()),
            ..CardRef::from_str("test/Service/svc@1.0.0").expect("card")
        };
        let principal = Principal::new(
            PrincipalId::new(Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: Some(card.clone()),
                card_ref_scope: CardRefScope::own(&card),
            },
            crate::test_support::tenant(),
            Vec::new(),
            PermissionSet::new(),
        );
        let card_text = card.to_string();
        let created = 1_700_000_000_000_000;
        let keys = ObservationsTable::acknowledged(
            &frame(vec![
                ("record_id", Arc::new(StringArray::from(vec!["r1", "r2"]))),
                (
                    "card_ref",
                    Arc::new(StringArray::from(vec![Some(card_text.as_str()), None])),
                ),
                (
                    "created_at",
                    Arc::new(
                        TimestampMicrosecondArray::from(vec![created, created + 1])
                            .with_timezone("+00:00"),
                    ),
                ),
            ]),
            &principal,
        )
        .expect("frame decodes");
        assert_eq!(keys.len(), 1, "a row without a subject activates nothing");
        assert_eq!(keys[0].record_id, "r1");
        assert_eq!(keys[0].card_uid, uid);
        assert_eq!(keys[0].created_at.timestamp_micros(), created);
    }
}
