use crate::model::interfaces::ModelInterface;
use wyrd_spec::card::model::{HuggingFaceTask, TfSaveFormat, TorchSaveFormat};

#[cfg(feature = "python")]
use {
    crate::error::{CardPyResult, WyrdPyError},
    crate::model::interfaces::options::{
        huggingface_task_token, parse_huggingface_task, parse_tf_save_format,
        parse_torch_save_format, tf_save_format_token, torch_save_format_token,
    },
    pyo3::prelude::*,
    pyo3::types::PyDict,
    std::path::PathBuf,
    wyrd_utils::py::module_version,
};

macro_rules! model_interface_struct {
    ($(#[$meta:meta])+ $name:ident { $($field:ident : $field_ty:ty),+ $(,)? }) => {
        $(#[$meta])+
        pub struct $name {
            $(
                #[cfg(feature = "python")]
                pub(super) $field: $field_ty,
            )+
        }
    };
}

model_interface_struct!(
/// Python builder for the Sklearn model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "SklearnInterface", extends = ModelInterface)
)]
SklearnInterface {
    model: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
});

model_interface_struct!(
/// Python builder for the Xgboost model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "XgboostInterface", extends = ModelInterface)
)]
XgboostInterface {
    model: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
});

model_interface_struct!(
/// Python builder for the Lightgbm model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "LightgbmInterface", extends = ModelInterface)
)]
LightgbmInterface {
    model: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
});

model_interface_struct!(
/// Python builder for the Catboost model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "CatboostInterface", extends = ModelInterface)
)]
CatboostInterface {
    model: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
});

model_interface_struct!(
/// Python builder for the Torch model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "TorchInterface", extends = ModelInterface)
)]
TorchInterface {
    model: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
    save_format: TorchSaveFormat,
});

model_interface_struct!(
/// Python builder for the Lightning model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "LightningInterface", extends = ModelInterface)
)]
LightningInterface {
    model: Option<Py<PyAny>>,
    trainer: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
});

model_interface_struct!(
/// Python builder for the Tensorflow model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "TensorflowInterface", extends = ModelInterface)
)]
TensorflowInterface {
    model: Option<Py<PyAny>>,
    preprocessor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
    save_format: TfSaveFormat,
});

model_interface_struct!(
/// Python builder for the Huggingface model interface.
#[cfg_attr(
    feature = "python",
    pyclass(module = "wyrd.model", name = "HuggingfaceInterface", extends = ModelInterface)
)]
HuggingfaceInterface {
    model: Option<Py<PyAny>>,
    processor: Option<Py<PyAny>>,
    framework_version: String,
    model_subtype: Option<String>,
    hf_task: HuggingFaceTask,
    repo_id: Option<String>,
    revision: Option<String>,
});

#[cfg(feature = "python")]
macro_rules! impl_model_interface_methods {
    ($type:ty { $($item:item)* }) => {
        #[pymethods]
        impl $type {
            $($item)*

            /// Return whether this interface currently holds a live model.
            #[getter]
            fn has_model(&self) -> bool {
                self.model.is_some()
            }

            /// Return the captured framework version.
            #[getter]
            fn framework_version(&self) -> &str {
                &self.framework_version
            }

            /// Return the captured model subtype, when known.
            #[getter]
            fn model_subtype(&self) -> Option<&str> {
                self.model_subtype.as_deref()
            }

            /// Save is deferred to a later `ModelCard` stage.
            #[pyo3(signature = (path, save_kwargs=None))]
            fn save(
                &self,
                path: PathBuf,
                save_kwargs: Option<&Bound<'_, PyDict>>,
            ) -> CardPyResult<()> {
                let _ = (path, save_kwargs);
                Err(WyrdPyError::validation(
                    "model interface save is not implemented in this stage",
                ))
            }

            /// Load is deferred to a later `ModelCard` stage.
            #[pyo3(signature = (path, load_kwargs=None))]
            fn load(
                &mut self,
                path: PathBuf,
                load_kwargs: Option<&Bound<'_, PyDict>>,
            ) -> CardPyResult<()> {
                let _ = (path, load_kwargs);
                Err(WyrdPyError::validation(
                    "model interface load is not implemented in this stage",
                ))
            }
        }
    };
}

#[cfg(feature = "python")]
impl_model_interface_methods!(SklearnInterface {
    /// Create a Sklearn model interface.
    #[new]
    #[pyo3(signature = (*, model=None, preprocessor=None))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                preprocessor,
                framework_version: package_version(py, "scikit-learn")?,
            },
            ModelInterface::marker("Sklearn"),
        ))
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(XgboostInterface {
    /// Create an Xgboost model interface.
    #[new]
    #[pyo3(signature = (*, model=None, preprocessor=None))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                preprocessor,
                framework_version: package_version(py, "xgboost")?,
            },
            ModelInterface::marker("Xgboost"),
        ))
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(LightgbmInterface {
    /// Create a Lightgbm model interface.
    #[new]
    #[pyo3(signature = (*, model=None, preprocessor=None))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                preprocessor,
                framework_version: package_version(py, "lightgbm")?,
            },
            ModelInterface::marker("Lightgbm"),
        ))
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(CatboostInterface {
    /// Create a Catboost model interface.
    #[new]
    #[pyo3(signature = (*, model=None, preprocessor=None))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                preprocessor,
                framework_version: package_version(py, "catboost")?,
            },
            ModelInterface::marker("Catboost"),
        ))
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(TorchInterface {
    /// Create a Torch model interface.
    #[new]
    #[pyo3(signature = (*, model=None, preprocessor=None, save_format="safetensors"))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
        save_format: &str,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                preprocessor,
                framework_version: package_version(py, "torch")?,
                save_format: parse_torch_save_format(save_format)?,
            },
            ModelInterface::marker("Torch"),
        ))
    }

    /// Return the canonical save-format token.
    #[getter]
    fn save_format(&self) -> &'static str {
        torch_save_format_token(self.save_format)
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(LightningInterface {
    /// Create a Lightning model interface.
    #[new]
    #[pyo3(signature = (*, model=None, trainer=None, preprocessor=None))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        trainer: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                trainer,
                preprocessor,
                framework_version: package_version(py, "pytorch-lightning")?,
            },
            ModelInterface::marker("Lightning"),
        ))
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(TensorflowInterface {
    /// Create a Tensorflow model interface.
    #[new]
    #[pyo3(signature = (*, model=None, preprocessor=None, save_format="keras"))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        preprocessor: Option<Py<PyAny>>,
        save_format: &str,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                preprocessor,
                framework_version: package_version(py, "tensorflow")?,
                save_format: parse_tf_save_format(save_format)?,
            },
            ModelInterface::marker("Tensorflow"),
        ))
    }

    /// Return the canonical save-format token.
    #[getter]
    fn save_format(&self) -> &'static str {
        tf_save_format_token(self.save_format)
    }
});

#[cfg(feature = "python")]
impl_model_interface_methods!(HuggingfaceInterface {
    /// Create a Huggingface model interface.
    #[new]
    #[pyo3(signature = (*, model=None, hf_task, processor=None, repo_id=None, revision=None))]
    fn __new__(
        py: Python<'_>,
        model: Option<Py<PyAny>>,
        hf_task: &str,
        processor: Option<Py<PyAny>>,
        repo_id: Option<String>,
        revision: Option<String>,
    ) -> CardPyResult<(Self, ModelInterface)> {
        Ok((
            Self {
                model_subtype: model_subtype(py, model.as_ref())?,
                model,
                processor,
                framework_version: package_version(py, "transformers")?,
                hf_task: parse_huggingface_task(hf_task)?,
                repo_id,
                revision,
            },
            ModelInterface::marker("Huggingface"),
        ))
    }

    /// Return the canonical Hugging Face task token.
    #[getter]
    fn hf_task(&self) -> &'static str {
        huggingface_task_token(self.hf_task)
    }

    /// Return the optional repository id.
    #[getter]
    fn repo_id(&self) -> Option<&str> {
        self.repo_id.as_deref()
    }

    /// Return the optional revision.
    #[getter]
    fn revision(&self) -> Option<&str> {
        self.revision.as_deref()
    }
});

#[cfg(feature = "python")]
fn package_version(py: Python<'_>, package: &str) -> CardPyResult<String> {
    Ok(module_version(py, package)?.unwrap_or_else(|| "unknown".to_string()))
}

#[cfg(feature = "python")]
fn model_subtype(py: Python<'_>, model: Option<&Py<PyAny>>) -> CardPyResult<Option<String>> {
    model
        .map(|model| crate::model::interfaces::helpers::qualname_of(py, model.bind(py)))
        .transpose()
}

#[cfg(feature = "python")]
impl SklearnInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
        }
    }
}

#[cfg(feature = "python")]
impl XgboostInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
        }
    }
}

#[cfg(feature = "python")]
impl LightgbmInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
        }
    }
}

#[cfg(feature = "python")]
impl CatboostInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
        }
    }
}

#[cfg(feature = "python")]
impl TorchInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
            save_format: TorchSaveFormat::Safetensors,
        }
    }
}

#[cfg(feature = "python")]
impl LightningInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            trainer: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
        }
    }
}

#[cfg(feature = "python")]
impl TensorflowInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            preprocessor: None,
            framework_version: String::new(),
            model_subtype: None,
            save_format: TfSaveFormat::Keras,
        }
    }
}

#[cfg(feature = "python")]
impl HuggingfaceInterface {
    pub(super) fn empty_live() -> Self {
        Self {
            model: None,
            processor: None,
            framework_version: String::new(),
            model_subtype: None,
            hf_task: HuggingFaceTask::Other,
            repo_id: None,
            revision: None,
        }
    }
}
