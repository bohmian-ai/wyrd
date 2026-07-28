//! Model Card spec types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::field::FieldSpec;
use crate::reference::Ref;

/// Locked ModelCard validation rules.
pub mod validate;

pub use validate::ModelCardError;

/// Body of a Model card.
///
/// This pure contract describes which framework loads the model, which task it
/// performs, which fields it consumes and produces, and which Artifact cards
/// hold model-related bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ModelSpec {
    /// Framework interface tag and per-interface config metadata.
    pub interface: ModelInterface,
    /// Closed Wyrd ML task taxonomy.
    pub task_type: TaskType,
    /// Typed input and output schema.
    pub signature: ModelSignature,
    /// Optional canonical sample input description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_input: Option<SampleInput>,
    /// Durable Artifact card references linked to this model card.
    #[serde(default)]
    pub card_refs: Vec<Ref>,
}

impl ModelSpec {
    /// Build a Model spec and run the locked validator.
    ///
    /// # Errors
    /// Returns a [`ModelCardError`] when any locked ModelCard invariant fails.
    pub fn new(
        interface: ModelInterface,
        task_type: TaskType,
        signature: ModelSignature,
        sample_input: Option<SampleInput>,
        card_refs: Vec<Ref>,
    ) -> Result<Self, ModelCardError> {
        let spec = Self {
            interface,
            task_type,
            signature,
            sample_input,
            card_refs,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Re-run locked validation invariants on this spec.
    ///
    /// # Errors
    /// Returns a [`ModelCardError`] when any locked ModelCard invariant fails.
    pub fn validate(&self) -> Result<(), ModelCardError> {
        validate::validate_model_spec(self)
    }

    /// Return the stable interface kind string used by Python, MCP, and docs.
    #[must_use]
    pub fn interface_kind(&self) -> &'static str {
        self.interface.kind()
    }

    /// Return true when this spec declares a generative task.
    #[must_use]
    pub const fn is_generation(&self) -> bool {
        matches!(self.task_type, TaskType::Generation)
    }

    /// Iterate durable Artifact card references linked to this model card.
    pub fn card_refs(&self) -> impl Iterator<Item = &Ref> {
        self.card_refs.iter()
    }
}

/// Framework interface tag for a Model spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "meta")]
pub enum ModelInterface {
    /// scikit-learn estimator persisted through the sklearn loader.
    Sklearn(SklearnMeta),
    /// XGBoost model persisted through the xgboost loader.
    Xgboost(XgboostMeta),
    /// LightGBM model persisted through the lightgbm loader.
    Lightgbm(LightgbmMeta),
    /// CatBoost model persisted through the catboost loader.
    Catboost(CatboostMeta),
    /// PyTorch module persisted through the torch loader.
    Torch(TorchMeta),
    /// PyTorch Lightning module persisted as a trainer checkpoint.
    Lightning(LightningMeta),
    /// TensorFlow or Keras model persisted through the tensorflow loader.
    Tensorflow(TensorflowMeta),
    /// Hugging Face model persisted through a local snapshot or pinned repo.
    Huggingface(HuggingfaceMeta),
    /// User-supplied loader metadata.
    Custom(CustomMeta),
}

impl ModelInterface {
    /// Stable variant name.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Sklearn(_) => "Sklearn",
            Self::Xgboost(_) => "Xgboost",
            Self::Lightgbm(_) => "Lightgbm",
            Self::Catboost(_) => "Catboost",
            Self::Torch(_) => "Torch",
            Self::Lightning(_) => "Lightning",
            Self::Tensorflow(_) => "Tensorflow",
            Self::Huggingface(_) => "Huggingface",
            Self::Custom(_) => "Custom",
        }
    }

    /// Human-readable loader family used in docs, errors, and MCP output.
    #[must_use]
    pub const fn loader_family(&self) -> &'static str {
        match self {
            Self::Sklearn(_) => "sklearn",
            Self::Xgboost(_) => "xgboost",
            Self::Lightgbm(_) => "lightgbm",
            Self::Catboost(_) => "catboost",
            Self::Torch(_) => "torch",
            Self::Lightning(_) => "pytorch-lightning",
            Self::Tensorflow(_) => "tensorflow",
            Self::Huggingface(_) => "huggingface-transformers",
            Self::Custom(_) => "custom-python-loader",
        }
    }

    /// Default media type produced by the model save path for this interface.
    #[must_use]
    pub const fn default_media_type(&self) -> &'static str {
        match self {
            Self::Sklearn(_) | Self::Xgboost(_) | Self::Lightgbm(_) | Self::Catboost(_) => {
                "application/x-joblib"
            }
            Self::Torch(_) => "application/vnd.safetensors",
            Self::Lightning(_) => "application/x-pytorch-ckpt",
            Self::Tensorflow(_) => "application/vnd.keras+zip",
            Self::Huggingface(_) => "application/vnd.huggingface+bundle",
            Self::Custom(_) => "application/octet-stream",
        }
    }

    /// Default file extension produced by the model save path.
    #[must_use]
    pub const fn default_extension(&self) -> &'static str {
        match self {
            Self::Sklearn(_) | Self::Xgboost(_) | Self::Lightgbm(_) | Self::Catboost(_) => "joblib",
            Self::Torch(meta) => match meta.save_format {
                TorchSaveFormat::Safetensors => "safetensors",
                TorchSaveFormat::Pickle => "pt",
            },
            Self::Lightning(_) => "ckpt",
            Self::Tensorflow(meta) => match meta.save_format {
                TfSaveFormat::Keras => "keras",
                TfSaveFormat::SavedModel => "savedmodel",
            },
            Self::Huggingface(_) => "huggingface",
            Self::Custom(_) => "bin",
        }
    }
}

/// Config for the Sklearn model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SklearnMeta {
    /// Installed sklearn version captured for reproducibility.
    pub framework_version: String,
    /// Concrete estimator class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Xgboost model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct XgboostMeta {
    /// Installed xgboost version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model subtype, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Lightgbm model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LightgbmMeta {
    /// Installed lightgbm version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model subtype, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Catboost model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CatboostMeta {
    /// Installed catboost version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model subtype, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Torch model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TorchMeta {
    /// Installed torch version captured for reproducibility.
    pub framework_version: String,
    /// Concrete module class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Save format for the torch artifact.
    pub save_format: TorchSaveFormat,
}

/// Config for the Lightning model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LightningMeta {
    /// Installed pytorch-lightning version captured for reproducibility.
    pub framework_version: String,
    /// Concrete LightningModule class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Tensorflow model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TensorflowMeta {
    /// Installed tensorflow or keras version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Save format for the tensorflow artifact.
    pub save_format: TfSaveFormat,
}

/// Config for the Huggingface model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HuggingfaceMeta {
    /// Installed transformers or diffusers version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Hugging Face task identifier required for rehydration.
    pub hf_task: HuggingFaceTask,
    /// Optional Hugging Face Hub repo id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Optional pinned revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// Config for the Custom model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CustomMeta {
    /// Reported framework version of the user-supplied loader environment.
    pub framework_version: String,
    /// Optional informational subtype string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Python module path containing the user-supplied loader.
    pub loader_module: String,
    /// Loader class or callable name.
    pub loader_class: String,
    /// Free-form string metadata that does not change the Wyrd contract.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

/// Closed Wyrd ML task taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TaskType {
    /// Two-class classification.
    BinaryClassification,
    /// Classification with more than two classes.
    MultiClassClassification,
    /// Continuous-valued regression.
    Regression,
    /// Unsupervised clustering.
    Clustering,
    /// Anomaly or outlier detection.
    AnomalyDetection,
    /// Time-series forecasting.
    Forecasting,
    /// Generative model task.
    Generation,
    /// Long-tail task described by interface metadata.
    Other,
}

/// Save format for the Torch model interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TorchSaveFormat {
    /// Tensors-only safetensors state dict.
    Safetensors,
    /// Pickle-backed torch save format.
    Pickle,
}

/// Save format for the Tensorflow model interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TfSaveFormat {
    /// Keras v3 single-file archive.
    Keras,
    /// SavedModel directory layout.
    SavedModel,
}

/// Hugging Face task identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum HuggingFaceTask {
    /// text-classification
    TextClassification,
    /// token-classification
    TokenClassification,
    /// question-answering
    QuestionAnswering,
    /// summarization
    Summarization,
    /// translation
    Translation,
    /// text-generation
    TextGeneration,
    /// fill-mask
    FillMask,
    /// zero-shot-classification
    ZeroShotClassification,
    /// image-classification
    ImageClassification,
    /// object-detection
    ObjectDetection,
    /// image-segmentation
    ImageSegmentation,
    /// image-to-text
    ImageToText,
    /// image-to-image
    ImageToImage,
    /// text-to-image
    TextToImage,
    /// depth-estimation
    DepthEstimation,
    /// audio-classification
    AudioClassification,
    /// automatic-speech-recognition
    AutomaticSpeechRecognition,
    /// audio-to-audio
    AudioToAudio,
    /// text-to-speech
    TextToSpeech,
    /// tabular-classification
    TabularClassification,
    /// tabular-regression
    TabularRegression,
    /// feature-extraction
    FeatureExtraction,
    /// sentence-similarity
    SentenceSimilarity,
    /// conversational
    Conversational,
    /// document-question-answering
    DocumentQuestionAnswering,
    /// visual-question-answering
    VisualQuestionAnswering,
    /// table-question-answering
    TableQuestionAnswering,
    /// embedding
    Embedding,
    /// multiple-choice
    MultipleChoice,
    /// Long-tail task identifier not yet first-class.
    Other,
}

/// Typed input and output schema for a Model spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ModelSignature {
    /// Ordered input field schema.
    pub inputs: Vec<FieldSpec>,
    /// Ordered output field schema.
    pub outputs: Vec<FieldSpec>,
}

impl ModelSignature {
    /// Build a signature from ordered input and output field lists.
    #[must_use]
    pub fn new(inputs: Vec<FieldSpec>, outputs: Vec<FieldSpec>) -> Self {
        Self { inputs, outputs }
    }

    /// Build a signature and validate locked signature invariants.
    ///
    /// # Errors
    /// Returns a [`ModelCardError`] when the signature is empty, duplicated, or
    /// uses a non-canonical dtype or invalid shape.
    pub fn from_fields(
        inputs: Vec<FieldSpec>,
        outputs: Vec<FieldSpec>,
    ) -> Result<Self, ModelCardError> {
        let signature = Self::new(inputs, outputs);
        signature.validate()?;
        Ok(signature)
    }

    /// Borrow input fields in declaration order.
    #[must_use]
    pub fn inputs(&self) -> &[FieldSpec] {
        &self.inputs
    }

    /// Borrow output fields in declaration order.
    #[must_use]
    pub fn outputs(&self) -> &[FieldSpec] {
        &self.outputs
    }

    /// Re-run locked signature invariants.
    ///
    /// # Errors
    /// Returns a [`ModelCardError`] when any signature invariant fails.
    pub fn validate(&self) -> Result<(), ModelCardError> {
        validate::validate_signature(self)
    }
}

/// Sample input description for loader-side rehydration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SampleInput {
    /// Closed kind tag selecting the loader-side deserializer.
    pub kind: SampleInputKind,
}

impl SampleInput {
    /// Build a sample input description from a kind tag.
    #[must_use]
    pub const fn new(kind: SampleInputKind) -> Self {
        Self { kind }
    }

    /// Return the closed kind tag selecting loader-side deserialization.
    #[must_use]
    pub const fn kind(&self) -> SampleInputKind {
        self.kind
    }
}

/// Closed set of sample input shapes supported by Wyrd at the contract layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum SampleInputKind {
    /// pandas dataframe serialized as parquet.
    Pandas,
    /// polars dataframe serialized as parquet.
    Polars,
    /// pyarrow table or record batch serialized as IPC.
    Arrow,
    /// numpy array serialized as npz.
    Numpy,
    /// torch tensor dict serialized as safetensors.
    Torch,
    /// TensorFlow tensor dict serialized as npz.
    Tf,
    /// JSON-safe Python dict serialized as json.
    Dict,
    /// JSON-safe Python list serialized as json.
    List,
    /// JSON-safe Python tuple serialized as json.
    Tuple,
    /// Plain UTF-8 string serialized as text.
    Str,
    /// No sample input declared.
    None,
}
