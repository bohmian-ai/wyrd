#[cfg(feature = "python")]
use wyrd_utils::py::WyrdPyResult;

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
    std::sync::Arc,
    wyrd_spec::card::model::{
        CatboostMeta, HuggingfaceMeta, LightgbmMeta, LightningMeta, SklearnMeta, TensorflowMeta,
        TorchMeta, XgboostMeta,
    },
};

/// Return `type(obj).__qualname__` for model subtype capture.
///
/// # Errors
/// Returns a Python-boundary error when the type metadata cannot be read.
#[cfg(feature = "python")]
pub(crate) fn qualname_of(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> WyrdPyResult<String> {
    Ok(obj
        .get_type()
        .getattr("__qualname__")?
        .extract::<String>()?)
}

/// Ensure every package required by a model interface path is importable.
///
/// # Errors
/// Returns `WYRD_MODEL_501_SERIALIZER_UNAVAILABLE` when a required Python
/// import is unavailable in the active interpreter.
#[cfg(feature = "python")]
pub(crate) fn ensure_extras(py: Python<'_>, extras: &str, required: &[&str]) -> WyrdPyResult<()> {
    for package in required {
        if py.import(package).is_err() {
            return Err(crate::error::serializer_unavailable(&format!(
                "wyrd[{extras}] (missing import: {package})"
            ))
            .into());
        }
    }
    Ok(())
}

/// Required Python imports for model interface local IO paths.
#[cfg(feature = "python")]
pub(crate) mod required {
    /// Imports required by the sklearn model interface.
    pub(crate) const SKLEARN: (&str, &[&str]) = ("sklearn", &["joblib", "sklearn"]);
    /// Imports required by the `XGBoost` model interface.
    pub(crate) const XGBOOST: (&str, &[&str]) = ("xgboost", &["joblib", "xgboost"]);
    /// Imports required by the `LightGBM` model interface.
    pub(crate) const LIGHTGBM: (&str, &[&str]) = ("lightgbm", &["joblib", "lightgbm"]);
    /// Imports required by the `CatBoost` model interface.
    pub(crate) const CATBOOST: (&str, &[&str]) = ("catboost", &["joblib", "catboost"]);
    /// Imports required by the Torch safetensors model interface path.
    pub(crate) const TORCH_SAFETENSORS: (&str, &[&str]) =
        ("torch", &["torch", "safetensors.torch"]);
    /// Imports required by the Torch pickle model interface path.
    pub(crate) const TORCH_PICKLE: (&str, &[&str]) = ("torch", &["torch"]);
    /// Imports required by the Lightning model interface path.
    pub(crate) const LIGHTNING: (&str, &[&str]) = ("lightning", &["pytorch_lightning", "torch"]);
    /// Imports required by the TensorFlow model interface path.
    pub(crate) const TENSORFLOW: (&str, &[&str]) = ("tensorflow", &["tensorflow"]);
    /// Imports required by the Hugging Face model interface path.
    pub(crate) const HUGGINGFACE: (&str, &[&str]) = ("huggingface", &["transformers"]);
}

#[cfg(feature = "python")]
macro_rules! clone_for_handle {
    ($type:ty, { $($py_field:ident),* $(,)? }, { $($field:ident),* $(,)? }) => {
        impl $type {
            pub(super) fn clone_for_handle(&self, _py: Python<'_>) -> Self {
                Self {
                    $(
                        $py_field: self.$py_field.as_ref().map(Arc::clone),
                    )*
                    $(
                        $field: self.$field.clone(),
                    )*
                }
            }
        }
    };
}

#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::SklearnInterface,
    { model, preprocessor },
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::XgboostInterface,
    { model, preprocessor },
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::LightgbmInterface,
    { model, preprocessor },
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::CatboostInterface,
    { model, preprocessor },
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::TorchInterface,
    { model, preprocessor },
    { framework_version, model_subtype, save_format }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::LightningInterface,
    { model, trainer, preprocessor },
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::TensorflowInterface,
    { model, preprocessor },
    { framework_version, model_subtype, save_format }
);
#[cfg(feature = "python")]
clone_for_handle!(
    crate::model::interfaces::kinds::HuggingfaceInterface,
    { model, processor },
    { framework_version, model_subtype, hf_task, repo_id, revision }
);
#[cfg(feature = "python")]
macro_rules! from_meta {
    ($type:ty, $meta:ty, { $($field:ident),* $(,)? }) => {
        impl $type {
            pub(super) fn from_meta(meta: &$meta) -> Self {
                Self {
                    $($field: meta.$field.clone(),)*
                    ..Self::empty_live()
                }
            }
        }
    };
}

#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::SklearnInterface,
    SklearnMeta,
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::XgboostInterface,
    XgboostMeta,
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::LightgbmInterface,
    LightgbmMeta,
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::CatboostInterface,
    CatboostMeta,
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::TorchInterface,
    TorchMeta,
    { framework_version, model_subtype, save_format }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::LightningInterface,
    LightningMeta,
    { framework_version, model_subtype }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::TensorflowInterface,
    TensorflowMeta,
    { framework_version, model_subtype, save_format }
);
#[cfg(feature = "python")]
from_meta!(
    crate::model::interfaces::kinds::HuggingfaceInterface,
    HuggingfaceMeta,
    { framework_version, model_subtype, hf_task, repo_id, revision }
);
