//! `PyO3` surface for `WyrdConfig`. Opt-in only — the Python SDK never
//! mutates user-constructed cards implicitly (Q4 in the overview).

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};

use wyrd_spec::envelope::{CardKind, Metadata};
use wyrd_spec::error::WyrdError;

use crate::config::WyrdConfig;
use crate::error::WyrdConfigError;
use crate::merge::apply_defaults as core_apply_defaults;
use wyrd_utils::py::{WyrdPyError, WyrdPyResult};

const METADATA_SENTINEL: &str = "<metadata-dict>";

/// Python-visible wrapper around `WyrdConfig`.
#[pyclass(module = "wyrd.config", name = "WyrdConfig")]
#[derive(Debug)]
pub struct WyrdConfigPy {
    inner: WyrdConfig,
}

#[pymethods]
impl WyrdConfigPy {
    /// Load `wyrd.toml`.
    #[classmethod]
    #[pyo3(signature = (path=None))]
    // justification: pyo3 boundary; the extractor produces an owned value (PathBuf/PyRef/newtype), taking it by reference would require a caller-side clone
    #[allow(clippy::needless_pass_by_value)]
    fn load(_cls: &Bound<'_, PyType>, path: Option<PathBuf>) -> WyrdPyResult<Self> {
        let cfg = WyrdConfig::load(path.as_deref()).map_err(WyrdPyError::from)?;
        Ok(Self { inner: cfg })
    }

    /// Apply defaults to a metadata dict in-place, dispatched on kind.
    fn apply_defaults<'py>(
        &self,
        py: Python<'py>,
        metadata: &Bound<'py, PyDict>,
        kind: &Bound<'py, PyAny>,
    ) -> WyrdPyResult<()> {
        let card_kind = extract_card_kind(kind)?;

        if metadata.get_item("name").ok().flatten().is_none() {
            return Err(schema_error(
                "metadata dict is missing required key `name`".to_string(),
            ));
        }

        let json_value = wyrd_utils::py::pydict_to_json_value(metadata)?;
        let mut meta: Metadata = serde_json::from_value(json_value)
            .map_err(|e| schema_error(format!("metadata dict invalid: {e}")))?;

        core_apply_defaults(&mut meta, &card_kind, &self.inner);

        write_merge_outputs(py, metadata, &meta)?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        let space = self
            .inner
            .defaults
            .space
            .as_ref()
            .map_or("", wyrd_spec::ids::SpaceName::as_str);
        let kinds: Vec<&'static str> = self
            .inner
            .kind_overrides
            .keys()
            .filter_map(CardKind::native_name)
            .collect();
        format!("WyrdConfig(space='{space}', kinds={kinds:?})")
    }
}

fn extract_card_kind(value: &Bound<'_, PyAny>) -> WyrdPyResult<CardKind> {
    if let Ok(s) = value.extract::<String>() {
        return CardKind::from_wire_name(&s)
            .ok_or_else(|| schema_error(format!("unknown CardKind: {s}")));
    }
    let name: String = value.getattr("name").and_then(|n| n.extract())?;
    CardKind::from_wire_name(&name).ok_or_else(|| schema_error(format!("unknown CardKind: {name}")))
}

fn write_merge_outputs<'py>(
    py: Python<'py>,
    target: &Bound<'py, PyDict>,
    meta: &Metadata,
) -> WyrdPyResult<()> {
    let mut payload = serde_json::Map::new();
    if let Some(s) = &meta.space {
        payload.insert(
            "space".into(),
            serde_json::to_value(s)
                .map_err(|e| schema_error(format!("metadata reserialize failed: {e}")))?,
        );
    }
    if !meta.labels.is_empty() {
        payload.insert(
            "labels".into(),
            serde_json::to_value(&meta.labels)
                .map_err(|e| schema_error(format!("metadata reserialize failed: {e}")))?,
        );
    }
    if !meta.annotations.is_empty() {
        payload.insert(
            "annotations".into(),
            serde_json::to_value(&meta.annotations)
                .map_err(|e| schema_error(format!("metadata reserialize failed: {e}")))?,
        );
    }
    for (k, v) in &payload {
        let py_val = wyrd_utils::py::json_to_pyobject(py, v)?;
        target.set_item(k, py_val.bind(py))?;
    }
    Ok(())
}

impl From<WyrdConfigError> for WyrdPyError {
    /// Project a configuration failure onto the shared Wyrd boundary adapter.
    fn from(error: WyrdConfigError) -> Self {
        Self::from(WyrdError::from(error))
    }
}

/// Build a configuration schema failure for the synthetic metadata-dict path.
///
/// Every boundary failure in this module originates from the caller's metadata
/// dictionary rather than a file on disk, so they share the same sentinel path.
fn schema_error(message: String) -> WyrdPyError {
    WyrdConfigError::Schema {
        message,
        path: PathBuf::from(METADATA_SENTINEL),
    }
    .into()
}

/// Register the `wyrd.config` submodule on the Python SDK aggregator.
pub fn register(py: Python<'_>, parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let config = PyModule::new(py, "config")?;
    wyrd_interfaces::error::register_exceptions(&config)?;
    config.add_class::<WyrdConfigPy>()?;
    parent.add_submodule(&config)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("wyrd.config", &config)?;
    Ok(())
}
