//! The Drift authoring projection: caller object → canonical record → tall rows.
//!
//! `observe.drift(...)` owns this conversion so the shared Bifrost facade and
//! `wyrd-queue` never interpret a Verifier kind: by the time a row reaches the
//! producer it is an ordinary JSON row against a described schema.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray};
use arrow::compute::cast;
use arrow_schema::DataType;
use chrono::Utc;
use serde_json::{Map, Value, json};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::FeatureName;
use wyrd_spec::vala::drift::record::{DriftRecordObservation, FeatureValue};
use wyrd_spec::vala::ids::{RecordId, SessionId};

use crate::bifrost::WriterTable;
use crate::observe::{ProjectedColumn, invalid_observation, require_projection, row_bytes};

/// Largest magnitude an integer feature may carry.
///
/// `num_value` is `Float64`, so beyond 2^53 a distinct integer would silently
/// share a double with its neighbours and the analysis would compare a value
/// the caller never emitted.
const MAX_EXACT_INT: i64 = 1 << 53;

/// The user columns `drift(...)` projects, in `vala.drift.observations` order.
fn projection_columns() -> [ProjectedColumn; 6] {
    [
        ("record_id", DataType::Utf8, false),
        ("series", DataType::Utf8, false),
        ("num_value", DataType::Float64, true),
        ("str_value", DataType::Utf8, true),
        ("session_id", DataType::Utf8, true),
        (
            "created_at",
            DataType::Timestamp(arrow_schema::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]
}

/// Verify a described fixed Drift table accepts this projection.
///
/// Run at `start_bifrost`, so an incompatible system table fails startup rather
/// than dropping every projected row when the queue later seals a batch.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the described authored
/// fields are not exactly this projection's ordered name, type, and
/// nullability sequence.
pub(crate) fn require_projection_columns(table: &WriterTable) -> Result<(), WyrdError> {
    require_projection(table, &projection_columns())
}

/// Build the canonical Drift observation from one caller-supplied JSON value.
///
/// The root must be a flat object of scalars; it deserializes directly into the
/// existing `BTreeMap<FeatureName, FeatureValue>` so `FeatureName` validates
/// every key and no second feature-map type exists. The pre-pass exists only to
/// name the offending key: an untagged `FeatureValue` failure reports that no
/// variant matched, which is useless to the caller who mistyped one field.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the root is not an object,
/// is empty, carries a null, array, nested object, non-finite number, or an
/// integer that `Float64` cannot hold exactly, or when a key is not a valid
/// `FeatureName`.
pub(crate) fn observation(
    input: &Value,
    session_id: Option<SessionId>,
) -> Result<DriftRecordObservation, WyrdError> {
    let Value::Object(object) = input else {
        return Err(invalid_observation(
            "drift features must be a JSON object of scalar values",
            json!({ "received": value_kind(input) }),
        ));
    };
    if object.is_empty() {
        return Err(invalid_observation(
            "drift features must name at least one feature",
            Value::Null,
        ));
    }
    check_scalar_values(object)?;
    let features: BTreeMap<FeatureName, FeatureValue> =
        serde_json::from_value(Value::Object(object.clone())).map_err(|error| {
            invalid_observation(
                "drift features are not a valid feature map",
                json!({ "source": error.to_string() }),
            )
        })?;
    Ok(DriftRecordObservation {
        record_id: RecordId(uuid::Uuid::now_v7()),
        session_id,
        features,
        created_at: Utc::now(),
    })
}

/// Project one canonical observation into one tall row per feature.
///
/// Each row repeats the logical `record_id`, optional `session_id`, and
/// `created_at`; `series` is the validated feature name. `Cat` and `Bool` set
/// only `str_value`. `Int` and `Float` set `num_value` and the Arrow-canonical
/// string of the value in its own type, so a categorical baseline fitted over
/// that column's keys matches an originally numeric feature.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` for a non-finite float, an
/// integer beyond exact `Float64` range, or a value Arrow cannot render as a
/// canonical string.
pub(crate) fn rows(record: &DriftRecordObservation) -> Result<Vec<Vec<u8>>, WyrdError> {
    let created_at = serde_json::to_value(record.created_at).map_err(|error| {
        invalid_observation(
            "created_at is not JSON",
            json!({"source": error.to_string()}),
        )
    })?;
    let session_id = match &record.session_id {
        Some(session) => json!(session.0.to_string()),
        None => Value::Null,
    };
    record
        .features
        .iter()
        .map(|(series, value)| {
            let (num_value, str_value) = projected_value(series, value)?;
            row_bytes(&json!({
                "record_id": record.record_id.0.to_string(),
                "series": series.as_str(),
                "num_value": num_value,
                "str_value": str_value,
                "session_id": session_id,
                "created_at": created_at,
            }))
        })
        .collect()
}

/// Split one feature value into its `(num_value, str_value)` projection.
///
/// # Errors
/// As [`rows`].
fn projected_value(
    series: &FeatureName,
    value: &FeatureValue,
) -> Result<(Value, Value), WyrdError> {
    match value {
        FeatureValue::Cat(text) => Ok((Value::Null, json!(text))),
        FeatureValue::Bool(flag) => Ok((
            Value::Null,
            json!(canonical_string(
                Arc::new(BooleanArray::from(vec![*flag])),
                series
            )?),
        )),
        FeatureValue::Int(number) => {
            if number.unsigned_abs() > MAX_EXACT_INT.unsigned_abs() {
                return Err(invalid_observation(
                    "integer feature exceeds exact Float64 range",
                    json!({ "series": series.as_str(), "value": number }),
                ));
            }
            Ok((
                json!(*number as f64),
                json!(canonical_string(
                    Arc::new(Int64Array::from(vec![*number])),
                    series
                )?),
            ))
        }
        FeatureValue::Float(number) => {
            if !number.is_finite() {
                return Err(invalid_observation(
                    "float feature must be finite",
                    json!({ "series": series.as_str() }),
                ));
            }
            Ok((
                json!(number),
                json!(canonical_string(
                    Arc::new(Float64Array::from(vec![*number])),
                    series
                )?),
            ))
        }
    }
}

/// Render one value through Arrow's own cast kernel.
///
/// The categorical key a baseline fitter derives from a typed column is
/// `cast(column, Utf8)`, so the client must not use Rust's `Display`: a
/// `Float64` `82000.0` is `"82000.0"` to Arrow and `"82000"` to Rust, and the
/// two would land in different PSI buckets. Casting the single value through
/// the same kernel keeps one conversion authority.
///
/// # Errors
/// Returns `WYRD_SDK_400_INVALID_OBSERVATION` when the cast fails or yields no
/// value, neither of which is reachable for the three scalar types projected
/// here.
fn canonical_string(array: ArrayRef, series: &FeatureName) -> Result<String, WyrdError> {
    // ponytail: one-element cast per numeric feature; batch the cast if a
    // profile ever shows the per-observation allocation mattering.
    let cast = cast(&array, &DataType::Utf8).map_err(|error| {
        invalid_observation(
            "feature value has no canonical string form",
            json!({ "series": series.as_str(), "source": error.to_string() }),
        )
    })?;
    cast.as_any()
        .downcast_ref::<StringArray>()
        .filter(|strings| strings.len() == 1 && strings.is_valid(0))
        .map(|strings| strings.value(0).to_owned())
        .ok_or_else(|| {
            invalid_observation(
                "feature value produced no canonical string",
                json!({ "series": series.as_str() }),
            )
        })
}

/// Reject every input value a feature map cannot represent, naming its key.
///
/// # Errors
/// As [`observation`].
fn check_scalar_values(object: &Map<String, Value>) -> Result<(), WyrdError> {
    for (key, value) in object {
        match value {
            Value::Bool(_) | Value::String(_) => {}
            Value::Number(number) => {
                if let Some(integer) = number.as_i64() {
                    if integer.unsigned_abs() > MAX_EXACT_INT.unsigned_abs() {
                        return Err(invalid_observation(
                            "integer feature exceeds exact Float64 range",
                            json!({ "series": key, "value": number }),
                        ));
                    }
                } else if number.as_u64().is_some() {
                    return Err(invalid_observation(
                        "integer feature exceeds exact Float64 range",
                        json!({ "series": key, "value": number }),
                    ));
                } else if !number.as_f64().is_some_and(f64::is_finite) {
                    return Err(invalid_observation(
                        "float feature must be finite",
                        json!({ "series": key }),
                    ));
                }
            }
            Value::Null | Value::Array(_) | Value::Object(_) => {
                return Err(invalid_observation(
                    "feature values must be boolean, integer, finite float, or string",
                    json!({ "series": key, "received": value_kind(value) }),
                ));
            }
        }
    }
    Ok(())
}

/// Name a JSON value's kind for a structured refusal.
fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
