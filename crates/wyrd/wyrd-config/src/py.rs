//! PyO3 surface for WyrdConfig. Opt-in only — the Python SDK never
//! mutates user-constructed cards implicitly (Q4 in the overview).

use std::path::PathBuf;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};

use wyrd_spec::envelope::{CardKind, Metadata};
use wyrd_spec::error::WyrdError;

use crate::config::WyrdConfig;
use crate::error::WyrdConfigError;
use crate::merge::apply_defaults as core_apply_defaults;

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
    fn load(_cls: &Bound<'_, PyType>, path: Option<PathBuf>) -> PyResult<Self> {
        let cfg = WyrdConfig::load(path.as_deref()).map_err(to_py_err)?;
        Ok(Self { inner: cfg })
    }

    /// Apply defaults to a metadata dict in-place, dispatched on kind.
    fn apply_defaults<'py>(
        &self,
        py: Python<'py>,
        metadata: &Bound<'py, PyDict>,
        kind: &Bound<'py, PyAny>,
    ) -> PyResult<()> {
        let card_kind = extract_card_kind(kind)?;

        if metadata.get_item("name").ok().flatten().is_none() {
            return Err(to_py_err(WyrdConfigError::Schema {
                message: "metadata dict is missing required key `name`".to_string(),
                path: std::path::PathBuf::new(),
            }));
        }

        let json_value = dict_to_json(py, metadata)?;
        let mut meta: Metadata = serde_json::from_value(json_value).map_err(|e| {
            to_py_err(WyrdConfigError::Schema {
                message: format!("metadata dict invalid: {e}"),
                path: std::path::PathBuf::new(),
            })
        })?;

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
            .map(|s| s.as_str())
            .unwrap_or("");
        let mut kinds: Vec<&'static str> = self
            .inner
            .kind_overrides
            .keys()
            .filter_map(|k| k.native_name())
            .collect();
        kinds.sort();
        format!("WyrdConfig(space='{space}', kinds={kinds:?})")
    }
}

fn extract_card_kind(value: &Bound<'_, PyAny>) -> PyResult<CardKind> {
    if let Ok(s) = value.extract::<String>() {
        return CardKind::from_wire_name(&s).ok_or_else(|| {
            to_py_err(WyrdConfigError::Schema {
                message: format!("unknown CardKind: {s}"),
                path: std::path::PathBuf::new(),
            })
        });
    }
    let name: String = value.getattr("name").and_then(|n| n.extract())?;
    CardKind::from_wire_name(&name).ok_or_else(|| {
        to_py_err(WyrdConfigError::Schema {
            message: format!("unknown CardKind: {name}"),
            path: std::path::PathBuf::new(),
        })
    })
}

fn dict_to_json<'py>(py: Python<'py>, d: &Bound<'py, PyDict>) -> PyResult<serde_json::Value> {
    let s: String = py
        .import("json")?
        .getattr("dumps")?
        .call1((d,))?
        .extract()?;
    serde_json::from_str(&s).map_err(|e| {
        to_py_err(WyrdConfigError::Schema {
            message: format!("json: {e}"),
            path: std::path::PathBuf::new(),
        })
    })
}

fn write_merge_outputs<'py>(
    py: Python<'py>,
    target: &Bound<'py, PyDict>,
    meta: &Metadata,
) -> PyResult<()> {
    let json = py.import("json")?;
    let loads = json.getattr("loads")?;
    let payload = serde_json::to_string(&serde_json::json!({
        "space": meta.space,
        "labels": meta.labels,
        "annotations": meta.annotations,
    }))
    .map_err(|e| {
        to_py_err(WyrdConfigError::Schema {
            message: format!("metadata reserialize failed: {e}"),
            path: std::path::PathBuf::new(),
        })
    })?;
    let parsed = loads.call1((payload,))?;
    let dict = parsed.cast::<PyDict>()?;
    for (k, v) in dict.iter() {
        if v.is_none() {
            continue;
        }
        target.set_item(k, v)?;
    }
    Ok(())
}

/// Convert a `WyrdConfigError` to a typed `wyrd.errors.Cfg*` Python exception.
fn to_py_err(err: WyrdConfigError) -> PyErr {
    let wyrd_err: WyrdError = err.into();
    wyrd_utils::py::wyrd_error_to_py_err(wyrd_err)
}

/// Register the `wyrd.config` submodule on the py-wyrd aggregator.
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
