use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::error::{CardPyResult, WyrdPyError};
use crate::model::interfaces::ModelInterface;
use crate::model::interfaces::kinds::{
    CatboostInterface, HuggingfaceInterface, LightgbmInterface, LightningInterface,
    SklearnInterface, TensorflowInterface, TorchInterface, XgboostInterface,
};
use wyrd_spec::card::model::{CustomMeta, ModelInterface as RustModelInterface};

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
    pub fn from_interface(interface: &Bound<'_, PyAny>) -> CardPyResult<Self> {
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

        Err(WyrdPyError::validation(
            "ModelCard requires a supported model interface",
        ))
    }

    /// Convert this holder into Rust-only interface metadata.
    ///
    /// # Errors
    /// Returns a Python-boundary error when the held configuration cannot be
    /// converted.
    pub fn to_spec_interface(&self, py: Python<'_>) -> CardPyResult<RustModelInterface> {
        match self {
            Self::Sklearn(value) => value.to_spec_interface(py),
            Self::Xgboost(value) => value.to_spec_interface(py),
            Self::Lightgbm(value) => value.to_spec_interface(py),
            Self::Catboost(value) => value.to_spec_interface(py),
            Self::Torch(value) => value.to_spec_interface(py),
            Self::Lightning(value) => value.to_spec_interface(py),
            Self::Tensorflow(value) => value.to_spec_interface(py),
            Self::Huggingface(value) => value.to_spec_interface(py),
            Self::Subclass(_) => Ok(RustModelInterface::Custom(CustomMeta {
                framework_version: String::new(),
                model_subtype: None,
                loader_module: String::new(),
                loader_class: String::new(),
                extra: BTreeMap::new(),
            })),
        }
    }
}
