use crate::error::CardPyResult;

#[cfg(feature = "python")]
use {
    pyo3::prelude::*,
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
pub(crate) fn qualname_of(_py: Python<'_>, obj: &Bound<'_, PyAny>) -> CardPyResult<String> {
    Ok(obj
        .get_type()
        .getattr("__qualname__")?
        .extract::<String>()?)
}

#[cfg(feature = "python")]
macro_rules! clone_for_handle {
    ($type:ty, { $($py_field:ident),* $(,)? }, { $($field:ident),* $(,)? }) => {
        impl $type {
            pub(super) fn clone_for_handle(&self, py: Python<'_>) -> Self {
                Self {
                    $(
                        $py_field: self.$py_field.as_ref().map(|value| value.clone_ref(py)),
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
