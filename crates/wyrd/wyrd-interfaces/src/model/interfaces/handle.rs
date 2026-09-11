use std::collections::BTreeMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::model::detect::{ModelInterfaceKind, detect_interface_variant};
use crate::model::interfaces::ModelInterface;
use crate::model::interfaces::kinds::{
    CatboostInterface, HuggingfaceInterface, LightgbmInterface, LightningInterface,
    SklearnInterface, TensorflowInterface, TorchInterface, XgboostInterface,
};
use wyrd_spec::card::model::{
    CustomMeta, HuggingFaceTask, ModelInterface as RustModelInterface, TfSaveFormat,
    TorchSaveFormat,
};
#[cfg(feature = "python")]
use wyrd_utils::py::WyrdPyResult;

/// Python-compatible dispatch wrapper for local model interface holders.
pub enum ModelInterfaceHandle {
    /// Sklearn model interface.
    Sklearn(SklearnInterface),
    /// Xgboost model interface.
    Xgboost(XgboostInterface),
    /// Lightgbm model interface.
    Lightgbm(LightgbmInterface),
    /// Catboost model interface.
    Catboost(CatboostInterface),
    /// Torch model interface.
    Torch(TorchInterface),
    /// Lightning model interface.
    Lightning(LightningInterface),
    /// Tensorflow model interface.
    Tensorflow(TensorflowInterface),
    /// Huggingface model interface.
    Huggingface(HuggingfaceInterface),
    /// Python subclass of the base `ModelInterface`.
    Subclass(Py<PyAny>),
}

impl ModelInterfaceHandle {
    /// Build a handle from an explicit Python `ModelInterface` object.
    ///
    /// # Errors
    /// Returns a validation error when the object is not a supported model
    /// interface.
    pub fn from_interface(interface: &Bound<'_, PyAny>) -> WyrdPyResult<Self> {
        macro_rules! extract_interface {
            ($type:ty, $variant:ident) => {
                if interface.is_instance_of::<$type>() {
                    let value = interface.extract::<PyRef<'_, $type>>()?;
                    return Ok(Self::$variant(value.clone_for_handle(interface.py())));
                }
            };
        }

        extract_interface!(SklearnInterface, Sklearn);
        extract_interface!(XgboostInterface, Xgboost);
        extract_interface!(LightgbmInterface, Lightgbm);
        extract_interface!(CatboostInterface, Catboost);
        extract_interface!(TorchInterface, Torch);
        extract_interface!(LightningInterface, Lightning);
        extract_interface!(TensorflowInterface, Tensorflow);
        extract_interface!(HuggingfaceInterface, Huggingface);
        if interface.is_instance_of::<ModelInterface>() {
            return Ok(Self::Subclass(interface.clone().unbind()));
        }

        Err(crate::error::model_validation("ModelCard requires a supported model interface").into())
    }

    /// Detect and build a default interface holder from a raw Python model.
    ///
    /// # Errors
    /// Returns a public Wyrd error when the model object is not supported or
    /// type metadata cannot be inspected.
    pub fn from_raw(py: Python<'_>, model: &Bound<'_, PyAny>) -> WyrdPyResult<Self> {
        let model_py = Arc::new(model.clone().unbind());
        let model_subtype = Some(crate::model::interfaces::helpers::qualname_of(py, model)?);
        match detect_interface_variant(py, model)? {
            ModelInterfaceKind::Huggingface => Ok(Self::Huggingface(HuggingfaceInterface {
                model: Some(Arc::clone(&model_py)),
                processor: None,
                framework_version: package_version(py, "transformers"),
                model_subtype,
                hf_task: HuggingFaceTask::Other,
                repo_id: None,
                revision: None,
            })),
            ModelInterfaceKind::Lightning => Ok(Self::Lightning(LightningInterface {
                model: Some(Arc::clone(&model_py)),
                trainer: None,
                preprocessor: None,
                framework_version: package_version(py, "pytorch-lightning"),
                model_subtype,
            })),
            ModelInterfaceKind::Torch => Ok(Self::Torch(TorchInterface {
                model: Some(Arc::clone(&model_py)),
                preprocessor: None,
                framework_version: package_version(py, "torch"),
                model_subtype,
                save_format: TorchSaveFormat::Pickle,
            })),
            ModelInterfaceKind::Tensorflow => Ok(Self::Tensorflow(TensorflowInterface {
                model: Some(Arc::clone(&model_py)),
                preprocessor: None,
                framework_version: package_version(py, "tensorflow"),
                model_subtype,
                save_format: TfSaveFormat::Keras,
            })),
            ModelInterfaceKind::Xgboost => Ok(Self::Xgboost(XgboostInterface {
                model: Some(Arc::clone(&model_py)),
                preprocessor: None,
                framework_version: package_version(py, "xgboost"),
                model_subtype,
            })),
            ModelInterfaceKind::Lightgbm => Ok(Self::Lightgbm(LightgbmInterface {
                model: Some(Arc::clone(&model_py)),
                preprocessor: None,
                framework_version: package_version(py, "lightgbm"),
                model_subtype,
            })),
            ModelInterfaceKind::Catboost => Ok(Self::Catboost(CatboostInterface {
                model: Some(Arc::clone(&model_py)),
                preprocessor: None,
                framework_version: package_version(py, "catboost"),
                model_subtype,
            })),
            ModelInterfaceKind::Sklearn => Ok(Self::Sklearn(SklearnInterface {
                model: Some(Arc::clone(&model_py)),
                preprocessor: None,
                framework_version: package_version(py, "scikit-learn"),
                model_subtype,
            })),
        }
    }

    /// Convert this holder into Rust-only interface metadata.
    ///
    /// # Errors
    /// Returns a Python-boundary error when the held configuration cannot be
    /// converted.
    pub fn to_spec_interface(&self, py: Python<'_>) -> WyrdPyResult<RustModelInterface> {
        match self {
            Self::Sklearn(value) => value.to_spec_interface(py),
            Self::Xgboost(value) => value.to_spec_interface(py),
            Self::Lightgbm(value) => value.to_spec_interface(py),
            Self::Catboost(value) => value.to_spec_interface(py),
            Self::Torch(value) => value.to_spec_interface(py),
            Self::Lightning(value) => value.to_spec_interface(py),
            Self::Tensorflow(value) => value.to_spec_interface(py),
            Self::Huggingface(value) => value.to_spec_interface(py),
            Self::Subclass(value) => {
                let ty = value.bind(py).get_type();
                let loader_module = ty.getattr("__module__")?.extract::<String>()?;
                let loader_class = ty.getattr("__qualname__")?.extract::<String>()?;
                Ok(RustModelInterface::Custom(CustomMeta {
                    framework_version: "custom".to_string(),
                    model_subtype: Some(format!("{loader_module}.{loader_class}")),
                    loader_module,
                    loader_class,
                    extra: BTreeMap::new(),
                }))
            }
        }
    }

    /// Convert this handle back into a Python interface object.
    ///
    /// # Errors
    /// Returns a Python-boundary error when the interface object cannot be
    /// allocated.
    pub fn into_py_any(self, py: Python<'_>) -> WyrdPyResult<Py<PyAny>> {
        macro_rules! into_py {
            ($value:expr, $kind:literal) => {
                Ok(Py::new(py, ($value, ModelInterface::marker($kind)))?.into_any())
            };
        }
        match self {
            Self::Sklearn(value) => into_py!(value, "Sklearn"),
            Self::Xgboost(value) => into_py!(value, "Xgboost"),
            Self::Lightgbm(value) => into_py!(value, "Lightgbm"),
            Self::Catboost(value) => into_py!(value, "Catboost"),
            Self::Torch(value) => into_py!(value, "Torch"),
            Self::Lightning(value) => into_py!(value, "Lightning"),
            Self::Tensorflow(value) => into_py!(value, "Tensorflow"),
            Self::Huggingface(value) => into_py!(value, "Huggingface"),
            Self::Subclass(value) => Ok(value),
        }
    }

    /// Borrow the held live model object, if one exists.
    #[must_use]
    pub fn model_ref(&self) -> Option<&Py<PyAny>> {
        match self {
            Self::Sklearn(value) => value.model.as_deref(),
            Self::Xgboost(value) => value.model.as_deref(),
            Self::Lightgbm(value) => value.model.as_deref(),
            Self::Catboost(value) => value.model.as_deref(),
            Self::Torch(value) => value.model.as_deref(),
            Self::Lightning(value) => value.model.as_deref(),
            Self::Tensorflow(value) => value.model.as_deref(),
            Self::Huggingface(value) => value.model.as_deref(),
            Self::Subclass(_) => None,
        }
    }

    /// Return the held live model as a Python object.
    ///
    /// Custom interfaces may expose an optional `model` attribute.
    ///
    /// # Errors
    /// Returns a Python error when a custom attribute getter fails.
    pub fn model_py(&self, py: Python<'_>) -> WyrdPyResult<Option<Py<PyAny>>> {
        match self {
            Self::Subclass(value) => optional_subclass_attribute(value.bind(py), "model"),
            _ => Ok(self.model_ref().map(|model| model.clone_ref(py))),
        }
    }

    /// Return the held preprocessing object as a Python object.
    ///
    /// Hugging Face interfaces use `processor` instead. Custom interfaces may
    /// expose an optional `preprocessor` attribute.
    ///
    /// # Errors
    /// Returns a Python error when a custom attribute getter fails.
    pub fn preprocessor_py(&self, py: Python<'_>) -> WyrdPyResult<Option<Py<PyAny>>> {
        let preprocessor = match self {
            Self::Sklearn(value) => value.preprocessor.as_deref(),
            Self::Xgboost(value) => value.preprocessor.as_deref(),
            Self::Lightgbm(value) => value.preprocessor.as_deref(),
            Self::Catboost(value) => value.preprocessor.as_deref(),
            Self::Torch(value) => value.preprocessor.as_deref(),
            Self::Lightning(value) => value.preprocessor.as_deref(),
            Self::Tensorflow(value) => value.preprocessor.as_deref(),
            Self::Huggingface(_) => None,
            Self::Subclass(value) => {
                return optional_subclass_attribute(value.bind(py), "preprocessor");
            }
        };
        Ok(preprocessor.map(|value| value.clone_ref(py)))
    }

    /// Return the held Hugging Face processor as a Python object.
    ///
    /// Custom interfaces may expose an optional `processor` attribute.
    ///
    /// # Errors
    /// Returns a Python error when a custom attribute getter fails.
    pub fn processor_py(&self, py: Python<'_>) -> WyrdPyResult<Option<Py<PyAny>>> {
        match self {
            Self::Huggingface(value) => Ok(value
                .processor
                .as_deref()
                .map(|processor| processor.clone_ref(py))),
            Self::Subclass(value) => optional_subclass_attribute(value.bind(py), "processor"),
            _ => Ok(None),
        }
    }
}

fn optional_subclass_attribute(
    interface: &Bound<'_, PyAny>,
    name: &str,
) -> WyrdPyResult<Option<Py<PyAny>>> {
    if !interface.hasattr(name)? {
        return Ok(None);
    }
    let value = interface.getattr(name)?;
    if value.is_none() {
        return Ok(None);
    }
    Ok(Some(value.unbind()))
}

fn package_version(py: Python<'_>, package: &str) -> String {
    wyrd_utils::py::module_version(py, package)
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}
