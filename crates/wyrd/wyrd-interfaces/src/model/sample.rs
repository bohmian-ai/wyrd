//! Python/Rust wrapper for Wyrd model sample inputs.

use wyrd_spec::card::model::{
    SampleInput as SampleInputSpec, SampleInputKind as SampleInputKindSpec,
};

#[cfg(feature = "python")]
use {
    crate::data::dtype::is_framework_class,
    crate::error::{CardPyResult, WyrdPyError},
    crate::model::interfaces::options::{parse_sample_input_kind, sample_input_kind_token},
    pyo3::prelude::*,
    pyo3::types::{PyAny, PyDict, PyList, PyModule, PyString, PyTuple, PyType},
    std::{fs, path::Path, path::PathBuf},
    wyrd_utils::py::{json_to_pyobject, pyobject_to_json},
};

/// Python-facing wrapper around `wyrd_spec::card::model::SampleInput`.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "SampleInput")
)]
pub struct SampleInput {
    kind: SampleInputKindSpec,
    #[cfg(feature = "python")]
    py_obj: Option<Py<PyAny>>,
}

impl SampleInput {
    /// Build a sample input wrapper from a kind tag.
    #[must_use]
    pub const fn from_kind(kind: SampleInputKindSpec) -> Self {
        Self {
            kind,
            #[cfg(feature = "python")]
            py_obj: None,
        }
    }

    /// Build a wrapper from a Wyrd sample-input spec.
    #[must_use]
    pub const fn from_inner(inner: &SampleInputSpec) -> Self {
        Self::from_kind(inner.kind)
    }

    /// Return the durable sample input kind.
    #[must_use]
    pub const fn kind(&self) -> SampleInputKindSpec {
        self.kind
    }

    /// Convert to the Rust spec type.
    #[must_use]
    pub const fn to_rust(&self) -> SampleInputSpec {
        SampleInputSpec { kind: self.kind }
    }

    /// Classify and retain a live Python sample input object.
    ///
    /// # Errors
    /// Returns a Wyrd Python-boundary error when the object shape is not a
    /// supported sample input kind.
    #[cfg(feature = "python")]
    pub fn from_python_object(py: Python<'_>, obj: &Bound<'_, PyAny>) -> CardPyResult<Self> {
        if obj.is_none() {
            return Ok(Self {
                kind: SampleInputKindSpec::None,
                py_obj: None,
            });
        }
        Ok(Self {
            kind: classify(py, obj)?,
            py_obj: Some(obj.clone().unbind()),
        })
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl SampleInput {
    /// Create a sample input shell from an explicit kind token.
    #[new]
    #[pyo3(signature = (*, kind="none", value=None))]
    fn __new__(kind: &str, value: Option<Py<PyAny>>) -> CardPyResult<Self> {
        Ok(Self {
            kind: parse_sample_input_kind(kind)?,
            py_obj: value,
        })
    }

    /// Return the canonical sample kind token.
    #[getter]
    fn kind_token(&self) -> &'static str {
        sample_input_kind_token(self.kind)
    }

    /// Return whether this shell holds a live Python sample value.
    #[getter]
    fn has_value(&self) -> bool {
        self.py_obj.is_some()
    }

    /// Return the Rust metadata representation as a Python dictionary.
    fn to_dict(&self, py: Python<'_>) -> CardPyResult<Py<PyAny>> {
        Ok(json_to_pyobject(
            py,
            &serde_json::to_value(self.to_rust())?,
        )?)
    }

    /// Classify a live Python object as a sample input.
    #[classmethod]
    #[pyo3(name = "from_python_object", signature = (value))]
    fn py_from_python_object(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> CardPyResult<Self> {
        Self::from_python_object(py, value)
    }

    /// Write the held sample input beside the local model artifact.
    #[pyo3(signature = (path, save_kwargs=None))]
    #[allow(clippy::needless_pass_by_value)]
    fn save(
        &self,
        py: Python<'_>,
        path: PathBuf,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let _ = save_kwargs;
        let value = self.py_obj.as_ref().map(|obj| obj.bind(py));
        write_to(py, &path, self.kind, value)
    }

    /// Read the kind-derived sample input artifact and retain it.
    #[pyo3(signature = (path, load_kwargs=None))]
    #[allow(clippy::needless_pass_by_value)]
    fn load(
        &mut self,
        py: Python<'_>,
        path: PathBuf,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let _ = load_kwargs;
        self.py_obj = read_from(py, &path, self.kind)?;
        Ok(())
    }
}

/// Register model sample wrapper classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SampleInput>()?;
    Ok(())
}

#[cfg(feature = "python")]
fn classify(py: Python<'_>, obj: &Bound<'_, PyAny>) -> CardPyResult<SampleInputKindSpec> {
    if is_framework_class(py, obj, "pandas", "DataFrame")? {
        return Ok(SampleInputKindSpec::Pandas);
    }
    if is_framework_class(py, obj, "polars", "DataFrame")? {
        return Ok(SampleInputKindSpec::Polars);
    }
    if is_framework_class(py, obj, "pyarrow", "Table")? {
        return Ok(SampleInputKindSpec::Arrow);
    }
    if is_framework_class(py, obj, "numpy", "ndarray")? {
        return Ok(SampleInputKindSpec::Numpy);
    }
    if is_framework_class(py, obj, "torch", "Tensor")? {
        return Ok(SampleInputKindSpec::Torch);
    }
    if is_framework_class(py, obj, "tensorflow", "Tensor")? {
        return Ok(SampleInputKindSpec::Tf);
    }
    if obj.is_instance_of::<PyDict>() {
        return Ok(SampleInputKindSpec::Dict);
    }
    if obj.is_instance_of::<PyList>() {
        return Ok(SampleInputKindSpec::List);
    }
    if obj.is_instance_of::<PyTuple>() {
        return Ok(SampleInputKindSpec::Tuple);
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(SampleInputKindSpec::Str);
    }
    let type_name: String = obj.get_type().getattr("__qualname__")?.extract()?;
    Err(WyrdPyError::model_validation_with_details(
        "unsupported sample input shape; pass a DataFrame, Table, ndarray, tensor, dict, list, tuple, str, or None",
        serde_json::json!({ "kind": type_name }),
    ))
}

#[cfg(feature = "python")]
fn write_to(
    py: Python<'_>,
    path: &Path,
    kind: SampleInputKindSpec,
    py_obj: Option<&Bound<'_, PyAny>>,
) -> CardPyResult<()> {
    match (kind, py_obj) {
        (SampleInputKindSpec::None, _) => Ok(()),
        (_, None) => Err(WyrdPyError::model_validation(format!(
            "SampleInput kind {kind:?} requires a Python object at save time"
        ))),
        (SampleInputKindSpec::Pandas, Some(obj)) => write_pandas(py, path, obj),
        (SampleInputKindSpec::Polars, Some(obj)) => write_polars(path, obj),
        (SampleInputKindSpec::Arrow, Some(obj)) => write_arrow(py, path, obj),
        (SampleInputKindSpec::Numpy, Some(obj)) => write_numpy(py, path, obj),
        (SampleInputKindSpec::Torch, Some(obj)) => write_torch(py, path, obj),
        (SampleInputKindSpec::Tf, Some(obj)) => write_tf(py, path, obj),
        (
            SampleInputKindSpec::Dict | SampleInputKindSpec::List | SampleInputKindSpec::Tuple,
            Some(obj),
        ) => write_json(path, obj),
        (SampleInputKindSpec::Str, Some(obj)) => write_str(path, obj),
    }
}

#[cfg(feature = "python")]
fn write_pandas(py: Python<'_>, path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Pandas)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("compression", "snappy")?;
    obj.call_method("to_parquet", (target_path_str(&target)?,), Some(&kwargs))?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_polars(path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Polars)?;
    obj.call_method1("write_parquet", (target_path_str(&target)?,))?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_arrow(py: Python<'_>, path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Arrow)?;
    let parquet = py
        .import("pyarrow.parquet")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[arrow]: {error}")))?;
    parquet.call_method1("write_table", (obj.clone(), target_path_str(&target)?))?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_numpy(py: Python<'_>, path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Numpy)?;
    let numpy = py
        .import("numpy")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[numpy]: {error}")))?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("allow_pickle", false)?;
    numpy
        .getattr("save")?
        .call((target_path_str(&target)?, obj.clone()), Some(&kwargs))?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_torch(py: Python<'_>, path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Torch)?;
    let safetensors = py
        .import("safetensors.torch")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[torch]: {error}")))?;
    let tensors = PyDict::new(py);
    tensors.set_item("sample", obj.clone())?;
    safetensors
        .getattr("save_file")?
        .call1((tensors, target_path_str(&target)?))?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_tf(py: Python<'_>, path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Tf)?;
    let numpy = py.import("numpy").map_err(|error| {
        WyrdPyError::serializer_unavailable(&format!("wyrd[tensorflow]: {error}"))
    })?;
    let value = obj.call_method0("numpy")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("allow_pickle", false)?;
    numpy
        .getattr("save")?
        .call((target_path_str(&target)?, value), Some(&kwargs))?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_json(path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Dict)?;
    let value = pyobject_to_json(obj)?;
    fs::write(target, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

#[cfg(feature = "python")]
fn write_str(path: &Path, obj: &Bound<'_, PyAny>) -> CardPyResult<()> {
    let target = sample_path(path, SampleInputKindSpec::Str)?;
    fs::write(target, obj.extract::<String>()?)?;
    Ok(())
}

#[cfg(feature = "python")]
fn read_from(
    py: Python<'_>,
    path: &Path,
    kind: SampleInputKindSpec,
) -> CardPyResult<Option<Py<PyAny>>> {
    let value = match kind {
        SampleInputKindSpec::None => return Ok(None),
        SampleInputKindSpec::Pandas => read_pandas(py, path)?,
        SampleInputKindSpec::Polars => read_polars(py, path)?,
        SampleInputKindSpec::Arrow => read_arrow(py, path)?,
        SampleInputKindSpec::Numpy | SampleInputKindSpec::Tf => read_numpy(py, path)?,
        SampleInputKindSpec::Torch => read_torch(py, path)?,
        SampleInputKindSpec::Dict | SampleInputKindSpec::List => read_json(py, path)?,
        SampleInputKindSpec::Tuple => read_tuple(py, path)?,
        SampleInputKindSpec::Str => read_str(py, path)?,
    };
    Ok(Some(value.unbind()))
}

#[cfg(feature = "python")]
fn read_pandas<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let pandas = py
        .import("pandas")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[pandas]: {error}")))?;
    let target = sample_path(path, SampleInputKindSpec::Pandas)?;
    Ok(pandas
        .getattr("read_parquet")?
        .call1((target_path_str(&target)?,))?)
}

#[cfg(feature = "python")]
fn read_polars<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let polars = py
        .import("polars")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[polars]: {error}")))?;
    let target = sample_path(path, SampleInputKindSpec::Polars)?;
    Ok(polars
        .getattr("read_parquet")?
        .call1((target_path_str(&target)?,))?)
}

#[cfg(feature = "python")]
fn read_arrow<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let parquet = py
        .import("pyarrow.parquet")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[arrow]: {error}")))?;
    let target = sample_path(path, SampleInputKindSpec::Arrow)?;
    Ok(parquet
        .getattr("read_table")?
        .call1((target_path_str(&target)?,))?)
}

#[cfg(feature = "python")]
fn read_numpy<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let numpy = py
        .import("numpy")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[numpy]: {error}")))?;
    let target = sample_path(path, SampleInputKindSpec::Numpy)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("allow_pickle", false)?;
    Ok(numpy
        .getattr("load")?
        .call((target_path_str(&target)?,), Some(&kwargs))?)
}

#[cfg(feature = "python")]
fn read_torch<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let safetensors = py
        .import("safetensors.torch")
        .map_err(|error| WyrdPyError::serializer_unavailable(&format!("wyrd[torch]: {error}")))?;
    let target = sample_path(path, SampleInputKindSpec::Torch)?;
    let values = safetensors
        .getattr("load_file")?
        .call1((target_path_str(&target)?,))?;
    Ok(values.get_item("sample")?)
}

#[cfg(feature = "python")]
fn read_json<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let target = sample_path(path, SampleInputKindSpec::Dict)?;
    let value = serde_json::from_slice(&fs::read(target)?)?;
    Ok(json_to_pyobject(py, &value)?.into_bound(py))
}

#[cfg(feature = "python")]
fn read_tuple<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let value = read_json(py, path)?;
    Ok(py.import("builtins")?.getattr("tuple")?.call1((value,))?)
}

#[cfg(feature = "python")]
fn read_str<'py>(py: Python<'py>, path: &Path) -> CardPyResult<Bound<'py, PyAny>> {
    let target = sample_path(path, SampleInputKindSpec::Str)?;
    Ok(PyString::new(py, &fs::read_to_string(target)?).into_any())
}

#[cfg(feature = "python")]
fn sample_path(path: &Path, kind: SampleInputKindSpec) -> CardPyResult<PathBuf> {
    fs::create_dir_all(path)?;
    Ok(path.join(sample_input_filename(kind)))
}

#[cfg(feature = "python")]
fn target_path_str(path: &Path) -> CardPyResult<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| WyrdPyError::model_validation("sample input path is not valid UTF-8"))
}

#[cfg(feature = "python")]
fn sample_input_filename(kind: SampleInputKindSpec) -> &'static str {
    match kind {
        SampleInputKindSpec::None => "",
        SampleInputKindSpec::Pandas | SampleInputKindSpec::Polars | SampleInputKindSpec::Arrow => {
            "sample_input.parquet"
        }
        SampleInputKindSpec::Numpy | SampleInputKindSpec::Tf => "sample_input.npy",
        SampleInputKindSpec::Torch => "sample_input.safetensors",
        SampleInputKindSpec::Dict | SampleInputKindSpec::List | SampleInputKindSpec::Tuple => {
            "sample_input.json"
        }
        SampleInputKindSpec::Str => "sample_input.txt",
    }
}

#[cfg(all(test, feature = "python"))]
mod tests {
    use super::sample_input_filename;
    use wyrd_spec::card::model::SampleInputKind;

    #[test]
    fn sample_input_filenames_follow_locked_table() {
        assert_eq!(
            sample_input_filename(SampleInputKind::Pandas),
            "sample_input.parquet"
        );
        assert_eq!(
            sample_input_filename(SampleInputKind::Torch),
            "sample_input.safetensors"
        );
        assert_eq!(
            sample_input_filename(SampleInputKind::Tf),
            "sample_input.npy"
        );
        assert_eq!(
            sample_input_filename(SampleInputKind::Tuple),
            "sample_input.json"
        );
        assert_eq!(
            sample_input_filename(SampleInputKind::Str),
            "sample_input.txt"
        );
    }
}
