//! Response-edge decoding of canonical protobuf payload columns into public JSON.
//!
//! The canonical signal tables store attribute collections and individual
//! semantic-convention values as pinned protobuf bytes, never as text. Turning
//! those bytes into the public JSON contract is a projection of an already
//! authorized, already collected batch: it happens after the plan chose to read
//! the column, so nothing here participates in planning, filtering, or
//! classification, and there is no DataFusion UDF or service in this path.

use base64::Engine as _;
use wyrd_spec::error::WyrdError;
use wyrd_tonic::otlp::common::v1::{AnyValue, KeyValueList, any_value::Value};
use wyrd_tonic::prost::Message as _;

/// How one stored `bytes_value` is projected into the public JSON contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ByteRule {
    /// Emit the standard OTLP JSON `{"bytesValue": "<base64>"}` envelope.
    ///
    /// Attribute payloads must stay lossless, and a byte string is the one
    /// `AnyValue` shape plain JSON cannot represent as itself.
    Tagged,
    /// Refuse the value.
    ///
    /// The structured GenAI message contract is plain JSON, so a stored byte
    /// string there is a value the public contract cannot express. It is
    /// rejected rather than stringified into something a caller would read as
    /// content.
    Reject,
}

/// Convert one decoded canonical `AnyValue` into its public JSON form.
///
/// Scalars, nulls, arrays, and key/value lists map to their natural JSON
/// counterparts so array order, object structure, scalar types, and nulls are
/// all preserved. An unset value is JSON `null`, which is the same shape the
/// protocol's absent value carries.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the value carries a byte string and
/// `rule` is [`ByteRule::Reject`], or when a `double_value` is not finite and
/// therefore has no JSON number.
fn any_value_to_json(value: &AnyValue, rule: ByteRule) -> Result<serde_json::Value, WyrdError> {
    let Some(inner) = value.value.as_ref() else {
        return Ok(serde_json::Value::Null);
    };
    let json = match inner {
        Value::StringValue(text) => serde_json::Value::String(text.clone()),
        Value::BoolValue(flag) => serde_json::Value::Bool(*flag),
        Value::IntValue(number) => serde_json::Value::from(*number),
        Value::DoubleValue(number) => serde_json::Number::from_f64(*number)
            .map(serde_json::Value::Number)
            .ok_or_else(|| unrepresentable("a non-finite double has no JSON number"))?,
        Value::ArrayValue(array) => {
            let mut items = Vec::with_capacity(array.values.len());
            for item in &array.values {
                items.push(any_value_to_json(item, rule)?);
            }
            serde_json::Value::Array(items)
        }
        Value::KvlistValue(list) => key_value_list_to_json(list, rule)?,
        Value::BytesValue(bytes) => match rule {
            ByteRule::Tagged => serde_json::json!({
                "bytesValue": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
            ByteRule::Reject => {
                return Err(unrepresentable(
                    "a stored byte string cannot be returned as structured JSON",
                ));
            }
        },
    };
    Ok(json)
}

/// Convert one decoded `KeyValueList` into a JSON object.
///
/// Later entries win, matching the protocol's last-occurrence lookup rule used
/// everywhere else in Bifrost.
///
/// # Errors
///
/// Propagates the failures documented on [`any_value_to_json`].
fn key_value_list_to_json(
    list: &KeyValueList,
    rule: ByteRule,
) -> Result<serde_json::Value, WyrdError> {
    let mut object = serde_json::Map::with_capacity(list.values.len());
    for entry in &list.values {
        let value = match entry.value.as_ref() {
            Some(value) => any_value_to_json(value, rule)?,
            None => serde_json::Value::Null,
        };
        object.insert(entry.key.clone(), value);
    }
    Ok(serde_json::Value::Object(object))
}

/// Decode one stored canonical attribute column value into a JSON object.
///
/// `bytes` are the exact pinned `KeyValueList` encoding the table layer wrote,
/// so a decode failure means the stored row is corrupt rather than that the
/// caller sent something wrong.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored bytes are not a
/// `KeyValueList` or carry a value with no JSON form.
pub(crate) fn attributes_to_json(bytes: &[u8]) -> Result<serde_json::Value, WyrdError> {
    let list =
        KeyValueList::decode(bytes).map_err(|_| corrupt("stored attributes are not decodable"))?;
    key_value_list_to_json(&list, ByteRule::Tagged)
}

/// Decode one stored canonical `AnyValue` into the structured message contract.
///
/// This is the `gen_ai.input.messages` / `gen_ai.output.messages` projection:
/// the value keeps its array order, object structure, scalar types, and nulls,
/// and a shape the public JSON contract cannot express is refused rather than
/// flattened into a string.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored bytes are not an `AnyValue`
/// or carry a byte string or non-finite double.
pub(crate) fn structured_message_to_json(bytes: &[u8]) -> Result<serde_json::Value, WyrdError> {
    let value =
        AnyValue::decode(bytes).map_err(|_| corrupt("stored GenAI messages are not decodable"))?;
    any_value_to_json(&value, ByteRule::Reject)
}

/// Look one canonical attribute up by key and project it as structured JSON.
///
/// Returns `None` when the key is absent. The final occurrence wins, matching
/// the ingest-side lookup rule.
///
/// # Errors
///
/// Propagates the failures documented on [`structured_message_to_json`].
pub(crate) fn structured_attribute(
    attributes: &KeyValueList,
    key: &str,
) -> Result<Option<serde_json::Value>, WyrdError> {
    let Some(entry) = attributes.values.iter().rev().find(|kv| kv.key == key) else {
        return Ok(None);
    };
    match entry.value.as_ref() {
        Some(value) => any_value_to_json(value, ByteRule::Reject).map(Some),
        None => Ok(Some(serde_json::Value::Null)),
    }
}

/// Decode one stored canonical attribute column into its protobuf form.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the stored bytes are not a `KeyValueList`.
pub(crate) fn decode_attributes(bytes: &[u8]) -> Result<KeyValueList, WyrdError> {
    KeyValueList::decode(bytes).map_err(|_| corrupt("stored attributes are not decodable"))
}

/// A stored canonical payload that this server wrote is not readable.
fn corrupt(detail: &'static str) -> WyrdError {
    WyrdError::Internal {
        message: "stored canonical payload is not decodable".to_owned(),
        details: serde_json::json!({ "detail": detail }),
    }
}

/// A stored canonical value has no representation in the public JSON contract.
fn unrepresentable(detail: &'static str) -> WyrdError {
    WyrdError::Internal {
        message: "stored canonical value is not representable in the public contract".to_owned(),
        details: serde_json::json!({ "detail": detail }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_tonic::otlp::common::v1::{ArrayValue, KeyValue};

    /// Build one `AnyValue` from its inner protocol value.
    fn any(value: Value) -> AnyValue {
        AnyValue { value: Some(value) }
    }

    /// Structured GenAI payloads keep every JSON shape and refuse byte strings.
    ///
    /// # Panics
    ///
    /// Panics when a canonical value does not project to its expected JSON or
    /// when a byte string is silently stringified instead of refused.
    #[test]
    fn structured_messages_preserve_shape_and_refuse_bytes() {
        let value = any(Value::ArrayValue(ArrayValue {
            values: vec![
                any(Value::KvlistValue(KeyValueList {
                    values: vec![
                        KeyValue {
                            key: "role".to_owned(),
                            value: Some(any(Value::StringValue("user".to_owned()))),
                        },
                        KeyValue {
                            key: "n".to_owned(),
                            value: Some(any(Value::IntValue(7))),
                        },
                        KeyValue {
                            key: "score".to_owned(),
                            value: Some(any(Value::DoubleValue(1.5))),
                        },
                        KeyValue {
                            key: "flag".to_owned(),
                            value: Some(any(Value::BoolValue(true))),
                        },
                        KeyValue {
                            key: "missing".to_owned(),
                            value: None,
                        },
                    ],
                })),
                AnyValue { value: None },
            ],
        }));
        let encoded = value.encode_to_vec();
        assert_eq!(
            structured_message_to_json(&encoded).expect("structured message decodes"),
            serde_json::json!([
                { "role": "user", "n": 7, "score": 1.5, "flag": true, "missing": null },
                null
            ])
        );

        let bytes_value = any(Value::BytesValue(vec![1, 2, 3])).encode_to_vec();
        assert!(
            structured_message_to_json(&bytes_value).is_err(),
            "a stored byte string must be refused, never stringified"
        );
    }

    /// Attribute payloads stay lossless, including byte-bearing values.
    ///
    /// # Panics
    ///
    /// Panics when the projected attribute object loses a value or drops the
    /// standard OTLP byte envelope.
    #[test]
    fn attribute_payloads_retain_byte_values() {
        let list = KeyValueList {
            values: vec![
                KeyValue {
                    key: "blob".to_owned(),
                    value: Some(any(Value::BytesValue(vec![0xde, 0xad]))),
                },
                KeyValue {
                    key: "dup".to_owned(),
                    value: Some(any(Value::StringValue("first".to_owned()))),
                },
                KeyValue {
                    key: "dup".to_owned(),
                    value: Some(any(Value::StringValue("last".to_owned()))),
                },
            ],
        };
        assert_eq!(
            attributes_to_json(&list.encode_to_vec()).expect("attributes decode"),
            serde_json::json!({ "blob": { "bytesValue": "3q0=" }, "dup": "last" })
        );
    }
}
