//! Python-boundary card reference types: `CardRef` and `Kind`.

use pyo3::prelude::*;
use pyo3::types::PyAny;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::VersionBlock;
use wyrd_utils::py::wyrd_error_to_py_err;

/// Wyrd card kind exposed to Python.
#[pyclass(module = "wyrd.cards", name = "CardKind", eq, eq_int, from_py_object)]
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
    /// Audit Card.
    Audit,
    /// Artifact Card.
    Artifact,
    /// Trigger Card.
    Trigger,
    /// Operator Card.
    Operator,
    /// Source Card.
    Source,
    /// Unknown/external card kind.
    External,
}

impl Kind {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Data => "Data",
            Self::Model => "Model",
            Self::Experiment => "Experiment",
            Self::Prompt => "Prompt",
            Self::Agent => "Agent",
            Self::Workflow => "Workflow",
            Self::Eval => "Eval",
            Self::Drift => "Drift",
            Self::Service => "Service",
            Self::Policy => "Policy",
            Self::Mcp => "Mcp",
            Self::Audit => "Audit",
            Self::Artifact => "Artifact",
            Self::Trigger => "Trigger",
            Self::Operator => "Operator",
            Self::Source => "Source",
            Self::External => "External",
        }
    }

    fn into_card_kind(self) -> CardKind {
        match self {
            Self::Data => CardKind::Data,
            Self::Model => CardKind::Model,
            Self::Experiment => CardKind::Experiment,
            Self::Prompt => CardKind::Prompt,
            Self::Agent => CardKind::Agent,
            Self::Workflow => CardKind::Workflow,
            Self::Eval => CardKind::Eval,
            Self::Drift => CardKind::Drift,
            Self::Service => CardKind::Service,
            Self::Policy => CardKind::Policy,
            Self::Mcp => CardKind::Mcp,
            Self::Audit => CardKind::Audit,
            Self::Artifact => CardKind::Artifact,
            Self::Trigger => CardKind::Trigger,
            Self::Operator => CardKind::Operator,
            Self::Source => CardKind::Source,
            Self::External => CardKind::External,
        }
    }

    pub(crate) fn from_card_kind(kind: &CardKind) -> Self {
        match kind {
            CardKind::Data => Self::Data,
            CardKind::Model => Self::Model,
            CardKind::Experiment => Self::Experiment,
            CardKind::Prompt => Self::Prompt,
            CardKind::Agent => Self::Agent,
            CardKind::Workflow => Self::Workflow,
            CardKind::Eval => Self::Eval,
            CardKind::Drift => Self::Drift,
            CardKind::Service => Self::Service,
            CardKind::Policy => Self::Policy,
            CardKind::Mcp => Self::Mcp,
            CardKind::Audit => Self::Audit,
            CardKind::Artifact => Self::Artifact,
            CardKind::Trigger => Self::Trigger,
            CardKind::Operator => Self::Operator,
            CardKind::Source => Self::Source,
            CardKind::External => Self::External,
        }
    }
}

#[pymethods]
impl Kind {
    /// Native Wyrd kind wire name.
    #[getter]
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn name(&self) -> &'static str {
        self.wire_name()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn __repr__(&self) -> String {
        format!("CardKind.{}", self.wire_name())
    }
}

/// Python wrapper for a Wyrd card reference.
///
/// Exposed to Python as `wyrd.cards.CardRef`. Wraps the pure-Rust `CardRef`
/// contract type from `wyrd-spec`. Lives in `wyrd-cards` so the error
/// conversion helpers from `wyrd-utils` are available without a circular dep.
#[derive(Clone, PartialEq)]
#[pyclass(module = "wyrd.cards", name = "CardRef", frozen, eq, from_py_object)]
pub struct CardRefPy(pub CardRef);

#[pymethods]
impl CardRefPy {
    /// Build a `CardRef` from its identity components.
    ///
    /// `kind` accepts the native wire name of a registered card kind
    /// (one of `Data`, `Model`, `Experiment`, `Prompt`, `Agent`, `Workflow`,
    /// `Eval`, `Drift`, `Service`, `Policy`, `Mcp`, `Audit`, `Artifact`,
    /// `Trigger`, `Operator`, `Source`). External
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
                None | Some("") => None,
                Some(value) => Some(SpaceName::new(value).map_err(|error| {
                    wyrd_error_to_py_err(invalid_identity("space", value, error))
                })?),
            };
        let parsed_uid = match uid {
            None | Some("") => None,
            Some(value) => Some(
                CardUid::new(value)
                    .map_err(|error| wyrd_error_to_py_err(invalid_identity("uid", value, error)))?,
            ),
        };
        Ok(Self(CardRef {
            kind: parsed_kind,
            name: parsed_name,
            version: parsed_version,
            space: parsed_space,
            uid: parsed_uid,
        }))
    }

    /// Card kind.
    #[getter]
    fn kind(&self) -> Kind {
        Kind::from_card_kind(&self.0.kind)
    }

    /// Referenced card name.
    #[getter]
    fn name(&self) -> String {
        self.0.name.to_string()
    }

    /// Exact referenced card version.
    #[getter]
    fn version(&self) -> String {
        self.0.version.to_string()
    }

    /// Optional space; `None` means current/default space.
    #[getter]
    fn space(&self) -> Option<String> {
        self.0.space.as_ref().map(ToString::to_string)
    }

    /// Optional resolved UID.
    #[getter]
    fn uid(&self) -> Option<String> {
        self.0.uid.as_ref().map(ToString::to_string)
    }

    fn __repr__(&self) -> String {
        let space = self
            .0
            .space
            .as_ref()
            .map_or_else(|| "None".to_string(), |s| format!("'{s}'"));
        let uid = self
            .0
            .uid
            .as_ref()
            .map_or_else(|| "None".to_string(), |u| format!("'{u}'"));
        format!(
            "CardRef(kind='{}', name='{}', version='{}', space={}, uid={})",
            self.0.kind.wire_name(),
            self.0.name,
            self.0.version,
            space,
            uid,
        )
    }
}

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

fn native_kind_names() -> Vec<&'static str> {
    CardKind::native()
        .iter()
        .filter_map(CardKind::native_name)
        .collect()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_external_card_kind_maps_to_external_variant() {
        let result = Kind::from_card_kind(&CardKind::External);
        assert_eq!(result, Kind::External);
    }
}
