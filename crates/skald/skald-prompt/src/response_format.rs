//! Response-format authoring helpers.

use serde_json::Value;
use skald_spec::ResponseType;

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

    /// Returns the response-format kind.
    pub const fn kind(&self) -> &ResponseFormatKind {
        &self.kind
    }

    /// Converts this builder value to the native prompt response type.
    pub fn response_type(&self) -> ResponseType {
        match &self.kind {
            ResponseFormatKind::Text | ResponseFormatKind::JsonObject => ResponseType::Text,
            ResponseFormatKind::JsonSchema { name, schema } => ResponseType::JsonSchema {
                name: name.clone(),
                schema: schema.clone(),
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

    /// Return a JSON-serializable dict for inspection.
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
        }
    }
}
