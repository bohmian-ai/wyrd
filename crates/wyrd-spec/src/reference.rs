//! Card references authored inside specs.

use serde::{Deserialize, Serialize};

#[cfg(feature = "python")]
use pyo3::{
    IntoPyObjectExt,
    exceptions::PyRuntimeError,
    prelude::*,
    types::{PyDict, PyList},
};

use crate::envelope::CardKind;
#[cfg(feature = "python")]
use crate::error::WyrdError;
use crate::ids::{CardName, CardUid, SpaceName};
use crate::version::VersionBlock;

/// Reference to a registered Card by kind, name, version, optional space, and optional UID.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.cards", name = "CardRef", frozen, eq, from_py_object)
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CardRef {
    /// Referenced Card kind.
    pub kind: CardKind,
    /// Referenced Card name.
    pub name: CardName,
    /// Exact referenced Card version.
    pub version: VersionBlock,
    /// Optional space; omitted means current/default space.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<SpaceName>,
    /// Optional resolved UID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<CardUid>,
}

/// Native Wyrd card kind accepted by Python CardRef construction.
#[cfg(feature = "python")]
#[pyclass(module = "wyrd.cards", name = "Kind", eq, eq_int, from_py_object)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Data Card.
    Data,
    /// Model Card.
    Model,
    /// Experiment Card.
    Experiment,
    /// Prompt Card.
    Prompt,
    /// Tool Card.
    Tool,
    /// Agent Card.
    Agent,
    /// Workflow Card.
    Workflow,
    /// Evaluation Card.
    Eval,
    /// Drift Card.
    Drift,
    /// Service Card.
    Service,
    /// Policy Card.
    Policy,
    /// MCP Card.
    Mcp,
    /// Skill Card.
    Skill,
    /// Sub-agent Card.
    SubAgent,
    /// Audit Card.
    Audit,
    /// Artifact Card.
    Artifact,
    /// Trigger Card.
    Trigger,
    /// Operator Card.
    Operator,
}

#[cfg(feature = "python")]
impl Kind {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Data => "Data",
            Self::Model => "Model",
            Self::Experiment => "Experiment",
            Self::Prompt => "Prompt",
            Self::Tool => "Tool",
            Self::Agent => "Agent",
            Self::Workflow => "Workflow",
            Self::Eval => "Eval",
            Self::Drift => "Drift",
            Self::Service => "Service",
            Self::Policy => "Policy",
            Self::Mcp => "Mcp",
            Self::Skill => "Skill",
            Self::SubAgent => "SubAgent",
            Self::Audit => "Audit",
            Self::Artifact => "Artifact",
            Self::Trigger => "Trigger",
            Self::Operator => "Operator",
        }
    }

    fn into_card_kind(self) -> CardKind {
        match self {
            Self::Data => CardKind::Data,
            Self::Model => CardKind::Model,
            Self::Experiment => CardKind::Experiment,
            Self::Prompt => CardKind::Prompt,
            Self::Tool => CardKind::Tool,
            Self::Agent => CardKind::Agent,
            Self::Workflow => CardKind::Workflow,
            Self::Eval => CardKind::Eval,
            Self::Drift => CardKind::Drift,
            Self::Service => CardKind::Service,
            Self::Policy => CardKind::Policy,
            Self::Mcp => CardKind::Mcp,
            Self::Skill => CardKind::Skill,
            Self::SubAgent => CardKind::SubAgent,
            Self::Audit => CardKind::Audit,
            Self::Artifact => CardKind::Artifact,
            Self::Trigger => CardKind::Trigger,
            Self::Operator => CardKind::Operator,
        }
    }

    fn from_card_kind(kind: &CardKind) -> Result<Self, WyrdError> {
        match kind {
            CardKind::Data => Ok(Self::Data),
            CardKind::Model => Ok(Self::Model),
            CardKind::Experiment => Ok(Self::Experiment),
            CardKind::Prompt => Ok(Self::Prompt),
            CardKind::Tool => Ok(Self::Tool),
            CardKind::Agent => Ok(Self::Agent),
            CardKind::Workflow => Ok(Self::Workflow),
            CardKind::Eval => Ok(Self::Eval),
            CardKind::Drift => Ok(Self::Drift),
            CardKind::Service => Ok(Self::Service),
            CardKind::Policy => Ok(Self::Policy),
            CardKind::Mcp => Ok(Self::Mcp),
            CardKind::Skill => Ok(Self::Skill),
            CardKind::SubAgent => Ok(Self::SubAgent),
            CardKind::Audit => Ok(Self::Audit),
            CardKind::Artifact => Ok(Self::Artifact),
            CardKind::Trigger => Ok(Self::Trigger),
            CardKind::Operator => Ok(Self::Operator),
            CardKind::External { name, .. } => Err(WyrdError::Validation {
                message: format!("external card kind is not exposed as wyrd.cards.Kind: {name}"),
                details: serde_json::json!({
                    "field": "kind",
                    "value": name,
                    "allowed": native_kind_names(),
                }),
            }),
        }
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl Kind {
    /// Native Wyrd kind wire name.
    #[getter]
    fn name(&self) -> &'static str {
        self.wire_name()
    }

    fn __repr__(&self) -> String {
        format!("Kind.{}", self.wire_name())
    }
}

/// Reference to an agent prompt, either inline or by Prompt Card reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum PromptRef {
    /// Inline native prompt payload.
    Inline(Box<skald_spec::Prompt>),
    /// Reference to a registered Prompt Card.
    Card(CardRef),
}

impl From<skald_spec::Prompt> for PromptRef {
    fn from(prompt: skald_spec::Prompt) -> Self {
        Self::Inline(Box::new(prompt))
    }
}

impl From<CardRef> for PromptRef {
    fn from(card_ref: CardRef) -> Self {
        Self::Card(card_ref)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl CardRef {
    /// Build a CardRef from its identity components.
    ///
    /// `kind` accepts the native wire name of a registered card kind
    /// (one of `Data`, `Model`, `Experiment`, `Prompt`, `Tool`, `Agent`,
    /// `Workflow`, `Eval`, `Drift`, `Service`, `Policy`, `Mcp`, `Skill`,
    /// `SubAgent`, `Audit`, `Artifact`, `Trigger`, `Operator`). External
    /// kinds are not constructable from Python in v1.
    #[new]
    #[pyo3(signature = (kind, name, version, *, space=None, uid=None))]
    fn __new__(
        kind: &Bound<'_, PyAny>,
        name: &str,
        version: &str,
        space: Option<&str>,
        uid: Option<&str>,
    ) -> PyResult<Self> {
        let parsed_kind = parse_kind_input(kind).map_err(wyrd_error_to_py_err)?;
        let parsed_name = CardName::new(name)
            .map_err(|error| wyrd_error_to_py_err(invalid_identity("name", name, error)))?;
        let parsed_version = VersionBlock::parse(version)
            .map_err(|error| wyrd_error_to_py_err(invalid_identity("version", version, error)))?;
        let parsed_space =
            match space {
                None => None,
                Some("") => None,
                Some(value) => Some(SpaceName::new(value).map_err(|error| {
                    wyrd_error_to_py_err(invalid_identity("space", value, error))
                })?),
            };
        let parsed_uid = match uid {
            None => None,
            Some("") => None,
            Some(value) => Some(
                CardUid::new(value)
                    .map_err(|error| wyrd_error_to_py_err(invalid_identity("uid", value, error)))?,
            ),
        };
        Ok(Self {
            kind: parsed_kind,
            name: parsed_name,
            version: parsed_version,
            space: parsed_space,
            uid: parsed_uid,
        })
    }

    /// Native kind of the referenced card.
    #[getter]
    fn kind(&self) -> PyResult<Kind> {
        Kind::from_card_kind(&self.kind).map_err(wyrd_error_to_py_err)
    }

    /// Referenced card name.
    #[getter]
    fn name(&self) -> String {
        self.name.to_string()
    }

    /// Exact referenced card version.
    #[getter]
    fn version(&self) -> String {
        self.version.to_string()
    }

    /// Optional space; `None` means current/default space.
    #[getter]
    fn space(&self) -> Option<String> {
        self.space.as_ref().map(ToString::to_string)
    }

    /// Optional resolved UID.
    #[getter]
    fn uid(&self) -> Option<String> {
        self.uid.as_ref().map(ToString::to_string)
    }

    fn __repr__(&self) -> String {
        let space = self
            .space
            .as_ref()
            .map_or_else(|| "None".to_string(), |s| format!("'{s}'"));
        let uid = self
            .uid
            .as_ref()
            .map_or_else(|| "None".to_string(), |u| format!("'{u}'"));
        format!(
            "CardRef(kind='{}', name='{}', version='{}', space={}, uid={})",
            self.kind.wire_name(),
            self.name,
            self.version,
            space,
            uid,
        )
    }
}

#[cfg(feature = "python")]
fn parse_kind_input(value: &Bound<'_, PyAny>) -> Result<CardKind, WyrdError> {
    if let Ok(kind) = value.extract::<Kind>() {
        return Ok(kind.into_card_kind());
    }
    if let Ok(kind) = value.extract::<String>() {
        return parse_native_kind(&kind);
    }
    Err(WyrdError::Validation {
        message: "card ref kind must be a wyrd.cards.Kind or native kind string".to_string(),
        details: serde_json::json!({
            "field": "kind",
            "allowed": native_kind_names(),
        }),
    })
}

#[cfg(feature = "python")]
fn parse_native_kind(value: &str) -> Result<CardKind, WyrdError> {
    for native in CardKind::native() {
        if native.wire_name() == value {
            return Ok(native);
        }
    }
    Err(WyrdError::Validation {
        message: format!("unknown card kind: {value}"),
        details: serde_json::json!({
            "field": "kind",
            "value": value,
            "allowed": native_kind_names(),
        }),
    })
}

#[cfg(feature = "python")]
fn native_kind_names() -> Vec<&'static str> {
    CardKind::native()
        .iter()
        .filter_map(CardKind::native_name)
        .collect()
}

#[cfg(feature = "python")]
fn invalid_identity(field: &str, value: &str, error: impl std::fmt::Display) -> WyrdError {
    WyrdError::Validation {
        message: format!("invalid card ref {field}: {value}"),
        details: serde_json::json!({
            "field": field,
            "value": value,
            "source": error.to_string(),
        }),
    }
}

#[cfg(feature = "python")]
fn wyrd_error_to_py_err(error: WyrdError) -> PyErr {
    Python::attach(|py| match build_wyrd_py_exception(py, &error) {
        Ok(exception) => PyErr::from_value(exception),
        Err(source) => PyRuntimeError::new_err(format!(
            "failed to construct structured Wyrd Python error: {source}"
        )),
    })
}

#[cfg(feature = "python")]
fn build_wyrd_py_exception<'py>(py: Python<'py>, error: &WyrdError) -> PyResult<Bound<'py, PyAny>> {
    let display = error.to_string();
    let problem = error.as_problem_json();
    let code = problem_string(&problem, "code", error.code()).to_owned();
    let title = problem_string(&problem, "title", error.title()).to_owned();
    let status = problem
        .get("status")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(|| u64::from(error.status()));
    let message = problem_string(&problem, "detail", &display).to_owned();
    let remediation = problem_string(&problem, "remediation", error.remediation()).to_owned();
    let problem_type = problem
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let details = problem
        .get("details")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let exception_name = if code.starts_with("WYRD_AGENT_") || code.starts_with("SKALD_AGENT_") {
        "AgentError"
    } else if code.starts_with("WYRD_TOOL_") || code.starts_with("SKALD_TOOL_") {
        "ToolError"
    } else if code.starts_with("WYRD_SESSION_") || code.starts_with("SKALD_SESSION_") {
        "SessionError"
    } else {
        "WyrdError"
    };
    let exception_type = py.import("wyrd._wyrd")?.getattr(exception_name)?;
    let exception = exception_type.call1((message.clone(),))?;
    exception.setattr("code", code)?;
    exception.setattr("message", message)?;
    exception.setattr("details", json_to_pyobject(py, &details)?.bind(py))?;
    exception.setattr("remediation", remediation)?;
    exception.setattr("status", status)?;
    exception.setattr("title", title)?;
    exception.setattr("type", problem_type)?;
    exception.setattr("problem", json_to_pyobject(py, &problem)?.bind(py))?;
    Ok(exception)
}

#[cfg(feature = "python")]
fn problem_string<'a>(problem: &'a serde_json::Value, key: &str, fallback: &'a str) -> &'a str {
    problem
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
}

#[cfg(feature = "python")]
fn json_to_pyobject(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(value) => value.into_py_any(py),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                value.into_py_any(py)
            } else if let Some(value) = value.as_u64() {
                value.into_py_any(py)
            } else if let Some(value) = value.as_f64() {
                value.into_py_any(py)
            } else {
                Err(PyRuntimeError::new_err("invalid JSON number"))
            }
        }
        serde_json::Value::String(value) => value.into_py_any(py),
        serde_json::Value::Array(values) => {
            let list = PyList::empty(py);
            for value in values {
                list.append(json_to_pyobject(py, value)?)?;
            }
            Ok(list.into_any().unbind())
        }
        serde_json::Value::Object(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                dict.set_item(key, json_to_pyobject(py, value)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ref() -> CardRef {
        CardRef {
            kind: CardKind::Artifact,
            name: CardName::new("weights").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: Some(SpaceName::new("prod").expect("static space is valid")),
            uid: None,
        }
    }

    #[test]
    fn card_ref_serde_roundtrips() {
        let card_ref = sample_ref();
        let json = serde_json::to_string(&card_ref).expect("serialize");
        let parsed: CardRef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(card_ref, parsed);
    }

    #[cfg(feature = "python")]
    #[test]
    fn kind_from_external_card_kind_returns_validation_error() {
        let external = CardKind::External {
            name: "vendor.plugin".to_string(),
            schema_hash: [0u8; 32],
        };
        let result = Kind::from_card_kind(&external);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("vendor.plugin"));
    }

    #[test]
    fn card_ref_skips_none_fields() {
        let card_ref = CardRef {
            kind: CardKind::Model,
            name: CardName::new("churn").expect("static name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: None,
            uid: None,
        };
        let json = serde_json::to_string(&card_ref).expect("serialize");
        assert!(!json.contains("space"), "space=None must skip");
        assert!(!json.contains("uid"), "uid=None must skip");
    }
}
