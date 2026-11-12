//! Response-format authoring helpers.

use serde_json::{Map, Value};
use skald_spec::{NamedResponseFormat, ResponseType};

use crate::coerce::schema_object;
use crate::error::PromptBuilderResult;

/// Provider-independent authoring choice that is immediately emitted into native fields.
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "wyrd.prompt", name = "ResponseFormat", skip_from_py_object)
)]
#[derive(Debug, Clone, PartialEq)]
pub struct ResponseFormat {
    kind: ResponseFormatKind,
}

/// Response-format variants accepted by the Prompt builder.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseFormatKind {
    /// Plain text response.
    Text,
    /// Provider-native JSON object mode.
    JsonObject,
    /// Provider-native named JSON Schema mode.
    JsonSchema {
        /// Schema name sent to providers that require one.
        name: String,
        /// JSON Schema object.
        schema: Value,
    },
    /// A Wyrd-named format such as `verdict`, which sets no native field
    /// itself; the Prompt compiler resolves its schema once the request is
    /// built.
    Named(NamedResponseFormat),
}

impl Default for ResponseFormat {
    fn default() -> Self {
        Self::text()
    }
}

impl ResponseFormat {
    /// Builds a plain text response format.
    pub const fn text() -> Self {
        Self {
            kind: ResponseFormatKind::Text,
        }
    }

    /// Builds OpenAI-compatible JSON object response format.
    pub const fn json_object() -> Self {
        Self {
            kind: ResponseFormatKind::JsonObject,
        }
    }

    /// Builds a named JSON Schema response format.
    pub fn json_schema(name: impl Into<String>, schema: Value) -> PromptBuilderResult<Self> {
        schema_object(&schema)?;
        Ok(Self {
            kind: ResponseFormatKind::JsonSchema {
                name: name.into(),
                schema,
            },
        })
    }

    /// Builds the canonical Verdict format: `outcome`, `reason`, and
    /// `citations`, plus a required object-valued `data` matching the JSON
    /// Schema `data` when one is given. The native Verdict schema builder
    /// validates `data` when the Prompt is built.
    pub fn verdict(data: Option<Map<String, Value>>) -> Self {
        Self {
            kind: ResponseFormatKind::Named(match data {
                None => NamedResponseFormat::Name("verdict".to_owned()),
                Some(data) => NamedResponseFormat::Expanded {
                    kind: "verdict".to_owned(),
                    data: Some(data),
                },
            }),
        }
    }

    /// Returns the response-format kind.
    pub const fn kind(&self) -> &ResponseFormatKind {
        &self.kind
    }

    /// Converts this builder value to the native prompt response type; a
    /// named format's type is resolved when its Prompt is built.
    pub fn response_type(&self) -> ResponseType {
        match &self.kind {
            ResponseFormatKind::Text
            | ResponseFormatKind::JsonObject
            | ResponseFormatKind::Named(_) => ResponseType::Text,
            ResponseFormatKind::JsonSchema { name, schema } => ResponseType::JsonSchema {
                name: name.clone(),
                schema: schema.clone().into(),
            },
        }
    }
}

#[cfg(feature = "python")]
#[pyo3::pymethods]
impl ResponseFormat {
    /// Python constructor for plain text responses.
    #[staticmethod]
    #[pyo3(name = "text")]
    pub fn text_py() -> Self {
        Self::text()
    }

    /// Python constructor for OpenAI-compatible JSON object responses.
    #[staticmethod]
    #[pyo3(name = "json_object")]
    pub fn json_object_py() -> Self {
        Self::json_object()
    }

    /// Python constructor for named JSON Schema responses.
    #[staticmethod]
    #[pyo3(name = "json_schema")]
    pub fn json_schema_py(
        name: String,
        schema: &pyo3::Bound<'_, pyo3::types::PyAny>,
    ) -> wyrd_utils::py::WyrdPyResult<Self> {
        let value = crate::coerce::schema_from_py(schema)?;
        Ok(Self::json_schema(name, value)?)
    }

    /// Return a JSON-serializable dict for inspection; a named format is
    /// its expanded `{"kind": ..., "data"?: ...}` mapping.
    ///
    /// # Errors
    ///
    /// Raises `WyrdError` when the value cannot be converted to Python.
    pub fn to_dict(
        &self,
        py: pyo3::Python<'_>,
    ) -> wyrd_utils::py::WyrdPyResult<pyo3::Py<pyo3::PyAny>> {
        let value = match &self.kind {
            ResponseFormatKind::Text => serde_json::json!({ "type": "text" }),
            ResponseFormatKind::JsonObject => serde_json::json!({ "type": "json_object" }),
            ResponseFormatKind::JsonSchema { name, schema } => {
                serde_json::json!({ "type": "json_schema", "name": name, "schema": schema })
            }
            ResponseFormatKind::Named(named) => serde_json::json!(NamedResponseFormat::Expanded {
                kind: named.name().to_owned(),
                data: named.data().cloned(),
            }),
        };
        Ok(wyrd_utils::py::json_to_pyobject(py, &value)?)
    }

    /// Return a concise Python representation.
    pub fn __repr__(&self) -> String {
        match &self.kind {
            ResponseFormatKind::Text => "ResponseFormat.text()".to_owned(),
            ResponseFormatKind::JsonObject => "ResponseFormat.json_object()".to_owned(),
            ResponseFormatKind::JsonSchema { name, .. } => {
                format!("ResponseFormat.json_schema(name={name:?})")
            }
            ResponseFormatKind::Named(NamedResponseFormat::Expanded { data: Some(_), .. }) => {
                "ResponseFormat.verdict(data=...)".to_owned()
            }
            ResponseFormatKind::Named(_) => "ResponseFormat.verdict()".to_owned(),
        }
    }

    /// Return a pretty JSON string for interactive inspection.
    pub fn __str__(&self) -> String {
        match &self.kind {
            ResponseFormatKind::Text => {
                wyrd_utils::json::pretty_json_string(&serde_json::json!({ "type": "text" }))
            }
            ResponseFormatKind::JsonObject => {
                wyrd_utils::json::pretty_json_string(&serde_json::json!({ "type": "json_object" }))
            }
            ResponseFormatKind::JsonSchema { name, schema } => {
                wyrd_utils::json::pretty_json_string(
                    &serde_json::json!({ "type": "json_schema", "name": name, "schema": schema }),
                )
            }
            ResponseFormatKind::Named(named) => {
                wyrd_utils::json::pretty_json_string(&serde_json::json!(named))
            }
        }
    }
}
