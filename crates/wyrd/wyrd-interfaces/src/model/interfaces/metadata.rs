use crate::error::{CardPyResult, WyrdPyError};
use crate::model::interfaces::kinds::{
    CatboostInterface, HuggingfaceInterface, LightgbmInterface, LightningInterface,
    SklearnInterface, TensorflowInterface, TorchInterface, XgboostInterface,
};
use wyrd_spec::card::model::{
    CatboostMeta, HuggingfaceMeta, LightgbmMeta, LightningMeta,
    ModelInterface as RustModelInterface, SklearnMeta, TensorflowMeta, TorchMeta, XgboostMeta,
};

#[cfg(feature = "python")]
use pyo3::prelude::*;

macro_rules! impl_to_spec {
    ($type:ty, $meta:ty, $variant:ident, $message:literal, $body:expr) => {
        impl $type {
            /// Convert this local Python holder into Rust-only metadata.
            #[cfg(feature = "python")]
            pub fn to_rust(&self, py: Python<'_>) -> CardPyResult<$meta> {
                $body(self, py)
            }

            /// Convert this local Python holder into a Rust-only interface enum.
            #[cfg(feature = "python")]
            pub fn to_spec_interface(&self, py: Python<'_>) -> CardPyResult<RustModelInterface> {
                Ok(RustModelInterface::$variant(self.to_rust(py)?))
            }

            /// Rebuild a sourceless Python holder from Rust-only metadata.
            pub fn from_spec_inner(interface: &RustModelInterface) -> CardPyResult<Self> {
                match interface {
                    RustModelInterface::$variant(meta) => Ok(Self::from_meta(meta)),
                    _ => Err(WyrdPyError::validation($message)),
                }
            }
        }
    };
}

impl_to_spec!(
    SklearnInterface,
    SklearnMeta,
    Sklearn,
    "SklearnInterface metadata must contain Sklearn metadata",
    |value: &SklearnInterface, _py| {
        Ok(SklearnMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
        })
    }
);

impl_to_spec!(
    XgboostInterface,
    XgboostMeta,
    Xgboost,
    "XgboostInterface metadata must contain Xgboost metadata",
    |value: &XgboostInterface, _py| {
        Ok(XgboostMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
        })
    }
);

impl_to_spec!(
    LightgbmInterface,
    LightgbmMeta,
    Lightgbm,
    "LightgbmInterface metadata must contain Lightgbm metadata",
    |value: &LightgbmInterface, _py| {
        Ok(LightgbmMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
        })
    }
);

impl_to_spec!(
    CatboostInterface,
    CatboostMeta,
    Catboost,
    "CatboostInterface metadata must contain Catboost metadata",
    |value: &CatboostInterface, _py| {
        Ok(CatboostMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
        })
    }
);

impl_to_spec!(
    TorchInterface,
    TorchMeta,
    Torch,
    "TorchInterface metadata must contain Torch metadata",
    |value: &TorchInterface, _py| {
        Ok(TorchMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
            save_format: value.save_format,
        })
    }
);

impl_to_spec!(
    LightningInterface,
    LightningMeta,
    Lightning,
    "LightningInterface metadata must contain Lightning metadata",
    |value: &LightningInterface, _py| {
        Ok(LightningMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
        })
    }
);

impl_to_spec!(
    TensorflowInterface,
    TensorflowMeta,
    Tensorflow,
    "TensorflowInterface metadata must contain Tensorflow metadata",
    |value: &TensorflowInterface, _py| {
        Ok(TensorflowMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
            save_format: value.save_format,
        })
    }
);

impl_to_spec!(
    HuggingfaceInterface,
    HuggingfaceMeta,
    Huggingface,
    "HuggingfaceInterface metadata must contain Huggingface metadata",
    |value: &HuggingfaceInterface, _py| {
        Ok(HuggingfaceMeta {
            framework_version: value.framework_version.clone(),
            model_subtype: value.model_subtype.clone(),
            hf_task: value.hf_task,
            repo_id: value.repo_id.clone(),
            revision: value.revision.clone(),
        })
    }
);
