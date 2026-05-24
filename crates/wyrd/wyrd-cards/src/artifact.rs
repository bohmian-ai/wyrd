//! Minimal `ArtifactCard` holder used by `DataCard` artifact references.

use serde::{Deserialize, Serialize};
use wyrd_interfaces::error::{CardPyResult, WyrdPyError};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::VersionBlock;

#[cfg(feature = "python")]
use pyo3::prelude::*;

/// Minimal local `ArtifactCard` holder.
///
/// This type is intentionally small in the `DataCard` slice. It is only the
/// Python `isinstance` target and `CardRef` carrier needed when a `DataCard` points
/// at an already existing artifact. Local `DataCard` save never constructs one.
#[cfg_attr(feature = "python", pyclass(module = "wyrd.data", from_py_object))]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactCard {
    /// Artifact card space.
    pub space: String,
    /// Artifact card name.
    pub name: String,
    /// Artifact card version.
    pub version: String,
    /// Artifact card UID.
    pub uid: String,
}

impl ArtifactCard {
    /// Convert this local artifact holder into a Wyrd `CardRef`.
    ///
    /// # Errors
    /// Returns a validation error when the identity fields are not valid Wyrd
    /// identifiers.
    pub fn as_card_ref(&self) -> CardPyResult<CardRef> {
        Ok(CardRef {
            kind: CardKind::Artifact,
            name: card_name("name", &self.name)?,
            version: version_block(&self.version)?,
            space: Some(space_name("space", &self.space)?),
            uid: Some(card_uid("uid", &self.uid)?),
        })
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl ArtifactCard {
    /// Create a minimal `ArtifactCard` reference holder.
    ///
    /// # Arguments
    /// * `space` - Artifact card space. Defaults to `default`.
    /// * `name` - Artifact card name. Defaults to `artifact`.
    /// * `version` - Artifact card version. Defaults to `0.1.0`.
    /// * `uid` - Artifact card UID. Defaults to a generated `UUIDv7`.
    #[new]
    #[pyo3(signature = (space=None, name=None, version=None, uid=None))]
    fn __new__(
        space: Option<&str>,
        name: Option<&str>,
        version: Option<&str>,
        uid: Option<&str>,
    ) -> Self {
        Self {
            space: space.unwrap_or("default").to_string(),
            name: name.unwrap_or("artifact").to_string(),
            version: version.unwrap_or("0.1.0").to_string(),
            uid: uid.map_or_else(wyrd_utils::uuid7, str::to_string),
        }
    }

    /// Return the artifact card space.
    #[getter]
    fn space(&self) -> &str {
        &self.space
    }

    /// Set the artifact card space.
    #[setter]
    fn set_space(&mut self, value: String) {
        self.space = value;
    }

    /// Return the artifact card name.
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }

    /// Set the artifact card name.
    #[setter]
    fn set_name(&mut self, value: String) {
        self.name = value;
    }

    /// Return the artifact card version.
    #[getter]
    fn version(&self) -> &str {
        &self.version
    }

    /// Set the artifact card version.
    #[setter]
    fn set_version(&mut self, value: String) {
        self.version = value;
    }

    /// Return the artifact card UID.
    #[getter]
    fn uid(&self) -> &str {
        &self.uid
    }

    /// Set the artifact card UID.
    #[setter]
    fn set_uid(&mut self, value: String) {
        self.uid = value;
    }
}

fn card_name(field: &str, value: &str) -> CardPyResult<CardName> {
    CardName::new(value).map_err(|error| invalid_identity(field, value, error))
}

fn space_name(field: &str, value: &str) -> CardPyResult<SpaceName> {
    SpaceName::new(value).map_err(|error| invalid_identity(field, value, error))
}

fn card_uid(field: &str, value: &str) -> CardPyResult<CardUid> {
    CardUid::new(value).map_err(|error| invalid_identity(field, value, error))
}

fn version_block(value: &str) -> CardPyResult<VersionBlock> {
    VersionBlock::parse(value).map_err(|error| {
        WyrdPyError::validation_with_details(
            format!("invalid artifact card version: {value}"),
            serde_json::json!({
                "field": "version",
                "value": value,
                "source": error.to_string(),
            }),
        )
    })
}

fn invalid_identity(field: &str, value: &str, error: impl std::fmt::Display) -> WyrdPyError {
    WyrdPyError::validation_with_details(
        format!("invalid artifact card {field}: {value}"),
        serde_json::json!({
            "field": field,
            "value": value,
            "source": error.to_string(),
        }),
    )
}
