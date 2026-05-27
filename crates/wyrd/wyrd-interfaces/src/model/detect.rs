//! Model interface auto-detection.

use crate::error::CardPyResult;

/// Auto-detectable built-in model interface variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelInterfaceKind {
    /// Hugging Face `transformers.PreTrainedModel`.
    Huggingface,
    /// `PyTorch` Lightning module.
    Lightning,
    /// Torch `nn.Module`.
    Torch,
    /// TensorFlow or Keras model.
    Tensorflow,
    /// `XGBoost` sklearn-style model.
    Xgboost,
    /// `LightGBM` sklearn-style model.
    Lightgbm,
    /// `CatBoost` model.
    Catboost,
    /// sklearn estimator.
    Sklearn,
}

impl ModelInterfaceKind {
    /// Stable Wyrd interface kind token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Huggingface => "Huggingface",
            Self::Lightning => "Lightning",
            Self::Torch => "Torch",
            Self::Tensorflow => "Tensorflow",
            Self::Xgboost => "Xgboost",
            Self::Lightgbm => "Lightgbm",
            Self::Catboost => "Catboost",
            Self::Sklearn => "Sklearn",
        }
    }
}

/// Detect the model interface variant for a raw Python framework model object.
///
/// Detection order is locked most-specific first: Hugging Face, Lightning,
/// `Torch`, `TensorFlow`/`Keras`, `XGBoost`, `LightGBM`, `CatBoost`, then sklearn.
///
/// # Errors
/// Returns `WYRD_MODEL_400_UNKNOWN_MODEL_TYPE` when no built-in model interface
/// matches the object.
#[cfg(feature = "python")]
pub fn detect_interface_variant(
    py: pyo3::Python<'_>,
    model: &pyo3::Bound<'_, pyo3::types::PyAny>,
) -> CardPyResult<ModelInterfaceKind> {
    use crate::data::dtype::is_framework_class;
    use crate::error::WyrdPyError;
    use pyo3::types::PyAnyMethods;

    if is_framework_class(py, model, "transformers", "PreTrainedModel")? {
        return Ok(ModelInterfaceKind::Huggingface);
    }
    if is_framework_class(py, model, "pytorch_lightning", "LightningModule")?
        || is_framework_class(py, model, "lightning.pytorch", "LightningModule")?
    {
        return Ok(ModelInterfaceKind::Lightning);
    }
    if is_framework_class(py, model, "torch.nn", "Module")? {
        return Ok(ModelInterfaceKind::Torch);
    }
    if is_framework_class(py, model, "tensorflow.keras", "Model")?
        || is_framework_class(py, model, "keras", "Model")?
    {
        return Ok(ModelInterfaceKind::Tensorflow);
    }
    if is_framework_class(py, model, "xgboost", "XGBModel")? {
        return Ok(ModelInterfaceKind::Xgboost);
    }
    if is_framework_class(py, model, "lightgbm", "LGBMModel")? {
        return Ok(ModelInterfaceKind::Lightgbm);
    }
    if is_framework_class(py, model, "catboost", "CatBoost")? {
        return Ok(ModelInterfaceKind::Catboost);
    }
    if is_framework_class(py, model, "sklearn.base", "BaseEstimator")? {
        return Ok(ModelInterfaceKind::Sklearn);
    }

    let ty = model.get_type();
    let module = ty
        .getattr("__module__")
        .and_then(|value| value.extract::<String>())
        .unwrap_or_else(|_| "unknown".to_string());
    let type_name = ty
        .getattr("__qualname__")
        .and_then(|value| value.extract::<String>())
        .unwrap_or_else(|_| "unknown".to_string());
    Err(WyrdPyError::unknown_model_type(module, type_name))
}

#[cfg(test)]
mod tests {
    use super::ModelInterfaceKind;

    #[test]
    fn model_interface_kind_tokens_match_spec_variants() {
        assert_eq!(ModelInterfaceKind::Huggingface.as_str(), "Huggingface");
        assert_eq!(ModelInterfaceKind::Lightning.as_str(), "Lightning");
        assert_eq!(ModelInterfaceKind::Torch.as_str(), "Torch");
        assert_eq!(ModelInterfaceKind::Tensorflow.as_str(), "Tensorflow");
        assert_eq!(ModelInterfaceKind::Xgboost.as_str(), "Xgboost");
        assert_eq!(ModelInterfaceKind::Lightgbm.as_str(), "Lightgbm");
        assert_eq!(ModelInterfaceKind::Catboost.as_str(), "Catboost");
        assert_eq!(ModelInterfaceKind::Sklearn.as_str(), "Sklearn");
    }
}
