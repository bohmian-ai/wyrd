//! Validation for the locked ModelCard contract.

use std::collections::HashSet;

use serde_json::json;

use crate::card::field::{Dim, FieldSpec, is_canonical_dtype};
use crate::card::model::{
    CatboostMeta, CustomMeta, HuggingfaceMeta, LightgbmMeta, LightningMeta, ModelInterface,
    ModelSignature, ModelSpec, SklearnMeta, TensorflowMeta, TorchMeta, XgboostMeta,
};
use crate::error::WyrdError;

/// ModelCard validation failures raised by the pure Rust validator.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelCardError {
    /// `signature.inputs` was empty.
    #[error("signature.inputs must be non-empty")]
    EmptyInputs,
    /// `signature.outputs` was empty.
    #[error("signature.outputs must be non-empty")]
    EmptyOutputs,
    /// Two fields declared the same name within one side of the signature.
    #[error("duplicate {side} field name: {name}")]
    DuplicateFieldName {
        /// Signature side containing the duplicate.
        side: &'static str,
        /// Duplicate field name.
        name: String,
    },
    /// Interface metadata had an empty framework version.
    #[error("{variant}.framework_version must be non-empty")]
    EmptyFrameworkVersion {
        /// Interface variant containing the empty version.
        variant: &'static str,
    },
    /// A signature dtype was not canonical.
    #[error("dtype string `{dtype}` is not a canonical Arrow logical dtype")]
    DtypeNormalizeFailed {
        /// Offending dtype string.
        dtype: String,
    },
    /// A fixed shape dimension was not positive.
    #[error("shape dim Fixed must be > 0 (got {value} in field `{field}`)")]
    ShapeFixedNonPositive {
        /// Field containing the invalid shape.
        field: String,
        /// Offending fixed dimension value.
        value: i64,
    },
    /// A Hugging Face revision was not a 7-40 character lowercase hex string.
    #[error("HuggingfaceMeta.revision must match ^[a-f0-9]{{7,40}}$: {value}")]
    HuggingfaceRevisionInvalid {
        /// Offending revision string.
        value: String,
    },
    /// A present Hugging Face repo id was empty.
    #[error("HuggingfaceMeta.repo_id must be non-empty when present")]
    HuggingfaceRepoIdEmpty,
    /// A Hugging Face task was missing.
    #[error("HuggingfaceMeta.hf_task must be set")]
    HuggingfaceTaskMissing,
    /// A custom loader field was empty.
    #[error("CustomMeta.{field} must be non-empty")]
    CustomLoaderInvalid {
        /// Offending custom loader field.
        field: &'static str,
    },
}

/// Run every locked client-side invariant against a Model spec.
///
/// # Errors
/// Returns the first locked invariant failure in deterministic order.
pub fn validate_model_spec(spec: &ModelSpec) -> Result<(), ModelCardError> {
    validate_signature(&spec.signature)?;
    validate_interface_meta(&spec.interface)?;
    Ok(())
}

/// Validate locked signature invariants.
///
/// # Errors
/// Returns the first empty-side, duplicate-name, dtype, or shape failure.
pub fn validate_signature(signature: &ModelSignature) -> Result<(), ModelCardError> {
    if signature.inputs.is_empty() {
        return Err(ModelCardError::EmptyInputs);
    }
    if signature.outputs.is_empty() {
        return Err(ModelCardError::EmptyOutputs);
    }
    validate_field_uniqueness("inputs", &signature.inputs)?;
    validate_field_uniqueness("outputs", &signature.outputs)?;
    for field in signature.inputs.iter().chain(signature.outputs.iter()) {
        validate_field_dtype(field)?;
        validate_shape(field)?;
    }
    Ok(())
}

/// Enforce unique field names within one signature side.
///
/// # Errors
/// Returns the first duplicate field name.
pub fn validate_field_uniqueness(
    side: &'static str,
    fields: &[FieldSpec],
) -> Result<(), ModelCardError> {
    let mut seen = HashSet::new();
    for field in fields {
        if !seen.insert(field.name.as_str()) {
            return Err(ModelCardError::DuplicateFieldName {
                side,
                name: field.name.to_string(),
            });
        }
    }
    Ok(())
}

/// Enforce canonical Arrow logical dtype strings.
///
/// # Errors
/// Returns a dtype normalization failure when the field dtype is not canonical.
pub fn validate_field_dtype(field: &FieldSpec) -> Result<(), ModelCardError> {
    if !is_canonical_dtype(field.dtype.as_str()) {
        return Err(ModelCardError::DtypeNormalizeFailed {
            dtype: field.dtype.clone(),
        });
    }
    Ok(())
}

/// Enforce positive fixed shape dimensions.
///
/// # Errors
/// Returns the first fixed dimension with a value less than one.
pub fn validate_shape(field: &FieldSpec) -> Result<(), ModelCardError> {
    for dim in &field.shape {
        if let Dim::Fixed(value) = dim
            && *value <= 0
        {
            return Err(ModelCardError::ShapeFixedNonPositive {
                field: field.name.to_string(),
                value: *value,
            });
        }
    }
    Ok(())
}

/// Validate per-interface metadata.
///
/// # Errors
/// Returns the first framework-version or variant-specific metadata failure.
pub fn validate_interface_meta(interface: &ModelInterface) -> Result<(), ModelCardError> {
    match interface {
        ModelInterface::Sklearn(meta) => validate_sklearn_meta(meta),
        ModelInterface::Xgboost(meta) => validate_xgboost_meta(meta),
        ModelInterface::Lightgbm(meta) => validate_lightgbm_meta(meta),
        ModelInterface::Catboost(meta) => validate_catboost_meta(meta),
        ModelInterface::Torch(meta) => validate_torch_meta(meta),
        ModelInterface::Lightning(meta) => validate_lightning_meta(meta),
        ModelInterface::Tensorflow(meta) => validate_tensorflow_meta(meta),
        ModelInterface::Huggingface(meta) => validate_huggingface_meta(meta),
        ModelInterface::Custom(meta) => validate_custom_meta(meta),
    }
}

/// Validate Sklearn interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_sklearn_meta(meta: &SklearnMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Sklearn", &meta.framework_version)
}

/// Validate Xgboost interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_xgboost_meta(meta: &XgboostMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Xgboost", &meta.framework_version)
}

/// Validate Lightgbm interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_lightgbm_meta(meta: &LightgbmMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Lightgbm", &meta.framework_version)
}

/// Validate Catboost interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_catboost_meta(meta: &CatboostMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Catboost", &meta.framework_version)
}

/// Validate Torch interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_torch_meta(meta: &TorchMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Torch", &meta.framework_version)
}

/// Validate Lightning interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_lightning_meta(meta: &LightningMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Lightning", &meta.framework_version)
}

/// Validate Tensorflow interface metadata.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_tensorflow_meta(meta: &TensorflowMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Tensorflow", &meta.framework_version)
}

/// Enforce a non-empty framework version.
///
/// # Errors
/// Returns an empty framework-version error.
pub fn validate_framework_version_nonempty(
    variant: &'static str,
    framework_version: &str,
) -> Result<(), ModelCardError> {
    if framework_version.trim().is_empty() {
        return Err(ModelCardError::EmptyFrameworkVersion { variant });
    }
    Ok(())
}

/// Validate Huggingface interface metadata.
///
/// # Errors
/// Returns an empty framework-version, empty repo id, or invalid revision error.
pub fn validate_huggingface_meta(meta: &HuggingfaceMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Huggingface", &meta.framework_version)?;
    if meta
        .repo_id
        .as_ref()
        .is_some_and(|repo| repo.trim().is_empty())
    {
        return Err(ModelCardError::HuggingfaceRepoIdEmpty);
    }
    if let Some(revision) = meta.revision.as_ref() {
        validate_huggingface_revision(revision)?;
    }
    Ok(())
}

/// Validate a Hugging Face revision string.
///
/// # Errors
/// Returns an invalid revision error when the revision is not lowercase hex.
pub fn validate_huggingface_revision(revision: &str) -> Result<(), ModelCardError> {
    if !(7..=40).contains(&revision.len())
        || !revision
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        return Err(ModelCardError::HuggingfaceRevisionInvalid {
            value: revision.to_string(),
        });
    }
    Ok(())
}

/// Validate Custom interface metadata.
///
/// # Errors
/// Returns an empty framework-version or custom loader field error.
pub fn validate_custom_meta(meta: &CustomMeta) -> Result<(), ModelCardError> {
    validate_framework_version_nonempty("Custom", &meta.framework_version)?;
    if meta.loader_module.trim().is_empty() {
        return Err(ModelCardError::CustomLoaderInvalid {
            field: "loader_module",
        });
    }
    if meta.loader_class.trim().is_empty() {
        return Err(ModelCardError::CustomLoaderInvalid {
            field: "loader_class",
        });
    }
    Ok(())
}

impl From<ModelCardError> for WyrdError {
    fn from(error: ModelCardError) -> Self {
        let message = error.to_string();
        match error {
            ModelCardError::EmptyInputs => WyrdError::ModelMissingSignature {
                message,
                details: json!({ "side": "inputs" }),
            },
            ModelCardError::EmptyOutputs => WyrdError::ModelMissingSignature {
                message,
                details: json!({ "side": "outputs" }),
            },
            ModelCardError::DuplicateFieldName { side, name } => WyrdError::ModelValidation {
                message,
                details: json!({
                    "side": side,
                    "name": name,
                    "rule": "signature.field_uniqueness"
                }),
            },
            ModelCardError::EmptyFrameworkVersion { variant } => WyrdError::ModelValidation {
                message,
                details: json!({
                    "interface_kind": variant,
                    "rule": "framework_version.non_empty"
                }),
            },
            ModelCardError::DtypeNormalizeFailed { dtype } => {
                WyrdError::ModelDtypeNormalizeFailed {
                    message,
                    details: json!({ "dtype": dtype }),
                }
            }
            ModelCardError::ShapeFixedNonPositive { field, value } => {
                WyrdError::ModelShapeInvalid {
                    message,
                    details: json!({ "field": field, "value": value }),
                }
            }
            ModelCardError::HuggingfaceRevisionInvalid { value } => {
                WyrdError::ModelHfRevisionInvalid {
                    message,
                    details: json!({
                        "revision": value,
                        "pattern": "^[a-f0-9]{7,40}$"
                    }),
                }
            }
            ModelCardError::HuggingfaceRepoIdEmpty => WyrdError::ModelValidation {
                message,
                details: json!({
                    "field": "repo_id",
                    "rule": "non_empty_when_some"
                }),
            },
            ModelCardError::HuggingfaceTaskMissing => WyrdError::ModelHfTaskMissing {
                message,
                details: json!({ "field": "hf_task" }),
            },
            ModelCardError::CustomLoaderInvalid { field } => WyrdError::ModelCustomLoaderInvalid {
                message,
                details: json!({ "field": field }),
            },
        }
    }
}
