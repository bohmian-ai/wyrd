//! Structured query error details.

/// Structured operands for a `WYRD_QUERY_400_INVALID_FIELD` payload.
#[derive(Debug, Default, Clone)]
pub struct QueryFieldErrorDetail<'a> {
    /// Machine reason tag.
    pub reason: &'a str,
    /// The offending field ref in canonical string form.
    pub field: Option<&'a str>,
    /// The surface that rejected it.
    pub surface: Option<&'a str>,
    /// The operator symbol.
    pub operator: Option<&'a str>,
    /// Expected value or field type.
    pub expected_type: Option<&'a str>,
    /// Actual value type supplied.
    pub actual_type: Option<&'a str>,
    /// Reserved fields the surface accepts.
    pub valid_fields: &'a [&'a str],
}

impl QueryFieldErrorDetail<'_> {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("reason".to_owned(), serde_json::json!(self.reason));
        if let Some(field) = self.field {
            map.insert("field".to_owned(), serde_json::json!(field));
        }
        if let Some(surface) = self.surface {
            map.insert("surface".to_owned(), serde_json::json!(surface));
        }
        if let Some(operator) = self.operator {
            map.insert("operator".to_owned(), serde_json::json!(operator));
        }
        if let Some(expected_type) = self.expected_type {
            map.insert("expected_type".to_owned(), serde_json::json!(expected_type));
        }
        if let Some(actual_type) = self.actual_type {
            map.insert("actual_type".to_owned(), serde_json::json!(actual_type));
        }
        if !self.valid_fields.is_empty() {
            map.insert(
                "valid_fields".to_owned(),
                serde_json::json!(self.valid_fields),
            );
        }
        serde_json::Value::Object(map)
    }
}
